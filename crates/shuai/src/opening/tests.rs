//! Runs the real solver and sioc client against the scripted server.

use std::collections::HashMap;
use std::time::Duration;

use bytestring::ByteString;
use serde_json::{Value, json};
use shuai_client::prelude::*;
use shuai_cpsat::opening::{self, Outcome};
use sioc::prelude::*;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::execution::{Execution, Progress, Step, Tile};
use super::problem::{build_problem, solve_options};
use super::*;
use crate::game::GameState;
use crate::session::{GameMode, MatchContext, MatchError, Session, SurrenderOnce};
use crate::test_support::{CLIENT_DISCONNECT, FakeServer};

const WIDTH: usize = 6;
const GENERAL: usize = 0;
const QUICK_BUDGET: Duration = Duration::from_secs(4);
/// The longest budget the reference hints allow, since a budget sets `minimum_lands`.
const LONG_BUDGET: Duration = Duration::from_secs(11);
const SHORT: Duration = Duration::from_millis(100);
/// One simulated tick when the client sends nothing. Under parallel solver load this
/// must stay well above scheduling delays, or a tick outruns the client's reply.
const REPLY_WAIT: Duration = Duration::from_millis(200);
const LONG: Duration = Duration::from_secs(60);

/// Bounds concurrent solves under plain `cargo test`, where tests share one process.
/// Each solve uses many workers, and unbounded parallel solves can starve into
/// `NoIncumbent`, which makes the bot correctly abandon. Nextest runs each test in its
/// own process, so `.config/nextest.toml` sets the same limit for it.
static SOLVER_SLOTS: Semaphore = Semaphore::const_new(2);

fn config(solve_budget: Duration) -> OpeningConfig {
    OpeningConfig {
        solve_budget,
        game_over_timeout: LONG,
        solver_grace: LONG,
    }
}

fn start_payload() -> Value {
    json!({
        "playerIndex": 0, "chat_room": "room", "usernames": ["a", "b"],
        "game_type": "custom", "options": {}, "replay_id": null, "team_chat_room": null,
        "team_2v2_play_again": null, "big_team_play_again": null,
        "chessClock": null, "chessClockConfig": null,
    })
}

/// A server-side game following the bundled simulation's rules for one player's moves,
/// generation and index. Terrain can also be edited by a test.
struct Game {
    width: usize,
    terrain: Vec<i64>,
    armies: Vec<i64>,
    turn: i64,
    attack_index: i64,
}

impl Game {
    fn open_field(width: usize) -> Self {
        let mut terrain = vec![-3; width * width];
        terrain[GENERAL] = 0;
        let mut armies = vec![0; width * width];
        armies[GENERAL] = 1;
        Self {
            width,
            terrain,
            armies,
            turn: 1,
            attack_index: 0,
        }
    }

    /// The whole map replaces the previous one, which the diff format allows.
    fn update(&self) -> Value {
        let width = i64::try_from(self.width).unwrap();
        let mut raw = vec![width, width];
        raw.extend_from_slice(&self.armies);
        raw.extend_from_slice(&self.terrain);
        let mut map_diff = vec![0, i64::try_from(raw.len()).unwrap()];
        map_diff.extend(raw);
        json!(["game_update", {
            "turn": self.turn, "scores": [], "attackIndex": self.attack_index,
            "generals": [GENERAL, -1], "map_diff": map_diff, "cities_diff": [0, 0],
            "pings": null, "chessClock": null,
        }])
    }

    /// Plays ticks until the client surrenders, resolving each attack in the next tick.
    /// Returns every attack index, in order.
    async fn play_until_surrender(&mut self, server: &mut FakeServer) -> Vec<i64> {
        let mut indices = Vec::new();
        let deadline = Instant::now() + LONG;
        loop {
            assert!(Instant::now() < deadline, "the client should surrender");
            server.send(&self.update());
            let Some(command) = server.next_within(REPLY_WAIT).await else {
                self.tick();
                continue;
            };
            if command[0] == "surrender" {
                assert_eq!(
                    Some(&self.attack_index),
                    indices.last(),
                    "surrender must follow the last move's observation"
                );
                return indices;
            }
            assert_eq!(command[0], "attack");
            indices.push(self.resolve(&command));
            self.tick();
        }
    }

    fn resolve(&mut self, attack: &[Value]) -> i64 {
        let from = usize::try_from(attack[1].as_u64().unwrap()).unwrap();
        let to = usize::try_from(attack[2].as_u64().unwrap()).unwrap();
        self.move_army(from, to);
        self.attack_index = attack[4].as_i64().unwrap();
        self.attack_index
    }

    /// Moves all but one army, as a queued move that is valid at its tick.
    fn move_army(&mut self, from: usize, to: usize) {
        if self.terrain[from] != 0 || self.armies[from] < 2 {
            return;
        }
        let moving = self.armies[from] - 1;
        self.armies[from] = 1;
        if self.terrain[to] == 0 {
            self.armies[to] += moving;
        } else if self.armies[to] >= moving {
            self.armies[to] -= moving;
        } else {
            self.armies[to] = moving - self.armies[to];
            self.terrain[to] = 0;
        }
    }

    /// Ends a tick. The general generates when the new turn is even.
    fn tick(&mut self) {
        self.turn += 1;
        if self.turn % 2 == 0 {
            self.armies[GENERAL] += 1;
        }
    }

    /// Ticks until the first attack arrives, and returns it.
    async fn tick_until_attack(&mut self, server: &mut FakeServer) -> Vec<Value> {
        let deadline = Instant::now() + LONG;
        loop {
            assert!(Instant::now() < deadline, "the client should attack");
            server.send(&self.update());
            if let Some(command) = server.next_within(REPLY_WAIT).await {
                assert_eq!(command[0], "attack");
                return command;
            }
            self.tick();
        }
    }
}

/// One `play_opening_with` call on a real connection.
struct Played {
    server: FakeServer,
    shutdown: CancellationToken,
    client: Client,
    run: JoinHandle<Result<(), MatchError>>,
    _solver_slot: SemaphorePermit<'static>,
}

impl Played {
    async fn start(config: OpeningConfig, start: Value) -> Self {
        let solver_slot = SOLVER_SLOTS.acquire().await.unwrap();
        let server = FakeServer::start().await;
        let client = ClientBuilder::new(server.url.clone())
            .transport(TransportStrategy::WebSocket)
            .open()
            .unwrap();
        let (socket_tx, mut rx) = client.connect(ByteString::from_static("/")).await.unwrap();
        let tx = GeneralsIoSender::new(socket_tx);
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();
        let start = Box::new(serde_json::from_value(start).unwrap());
        let run = tokio::spawn(async move {
            // Session reaches gameplay only after an acknowledged request, so the
            // connection is always established by now.
            assert!(matches!(rx.recv().await, Some(Signal::Connect(_))));
            let surrender = SurrenderOnce::default();
            let result = play_opening_with(
                config,
                MatchContext {
                    start,
                    rx: &mut rx,
                    tx: &tx,
                    surrender: &surrender,
                    shutdown: &token,
                },
            )
            .await;
            tx.disconnect().await;
            result
        });
        Self {
            server,
            shutdown,
            client,
            run,
            _solver_slot: solver_slot,
        }
    }

    /// Returns the result after requiring that no further game command was sent.
    ///
    /// Frames still arriving at close can stop sioc 0.5 from flushing its disconnect
    /// packet, so that one is optional here. Session's tests check it strictly.
    async fn result(self) -> Result<(), MatchError> {
        let result = tokio::time::timeout(LONG, self.run)
            .await
            .expect("play_opening should return")
            .unwrap();
        // The same known issue can fail this join, so the transcript is what is judged.
        let _ = self.client.join().await;
        let commands = self.server.remaining_commands().await;
        let commands: Vec<&str> = commands
            .iter()
            .map(String::as_str)
            .filter(|&command| command != CLIENT_DISCONNECT)
            .collect();
        assert_eq!(commands, [] as [&str; 0], "no game command should follow");
        result
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_executes_in_order_then_surrenders_and_awaits_game_over() {
    let mut played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    let indices = game.play_until_surrender(&mut played.server).await;
    assert_eq!(indices.first(), Some(&1));
    assert!(
        indices.windows(2).all(|pair| pair[0] < pair[1]),
        "indices must only increase, so none repeats: {indices:?}"
    );
    assert!(game.terrain.iter().filter(|&&t| t == 0).count() > 1);

    // GameLost is not the terminal event, so play must still be running.
    played
        .server
        .send(&json!(["game_lost", {"surrender": true}]));
    tokio::time::sleep(SHORT).await;
    assert!(
        !played.run.is_finished(),
        "game_lost must not end the phase"
    );

    played.server.send(&json!(["game_over"]));
    played.result().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_locally_sent_last_move_never_triggers_surrender() {
    let mut played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    let attack = game.tick_until_attack(&mut played.server).await;

    // The destination looks captured, but the server has not acknowledged the move. A
    // same-turn heartbeat must neither resubmit it nor advance the plan.
    let to = usize::try_from(attack[2].as_u64().unwrap()).unwrap();
    game.terrain[to] = 0;
    game.armies[to] = 1;
    for _ in 0..3 {
        played.server.send(&game.update());
    }
    assert_eq!(played.server.next_within(SHORT).await, None);

    played.shutdown.cancel();
    played.result().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn index_advance_without_ownership_abandons_without_replay() {
    let mut played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.tick_until_attack(&mut played.server).await;
    game.attack_index = 1;
    game.turn += 1;
    played.server.send(&game.update());
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Abandon(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn unacknowledged_move_abandons_without_replay() {
    let mut played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.tick_until_attack(&mut played.server).await;
    game.turn += 8;
    played.server.send(&game.update());
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Abandon(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn game_over_while_solving_stops_the_solver_and_returns() {
    let played = Played::start(config(LONG_BUDGET), start_payload()).await;
    let started = Instant::now();
    played.server.send(&Game::open_field(12).update());
    played.server.send(&json!(["game_over"]));
    played.result().await.unwrap();
    assert!(
        started.elapsed() < LONG_BUDGET / 2,
        "the stop should end the solve long before its budget"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_solver_that_does_not_stop_in_time_is_fatal() {
    let no_grace = OpeningConfig {
        solver_grace: Duration::ZERO,
        ..config(LONG_BUDGET)
    };
    let played = Played::start(no_grace, start_payload()).await;
    played.server.send(&Game::open_field(12).update());
    played.server.send(&json!(["game_over"]));
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Fatal(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_while_solving_stops_the_solver_and_returns() {
    let played = Played::start(config(LONG_BUDGET), start_payload()).await;
    let started = Instant::now();
    played.server.send(&Game::open_field(12).update());
    tokio::time::sleep(SHORT).await;
    played.shutdown.cancel();
    played.result().await.unwrap();
    assert!(started.elapsed() < LONG_BUDGET / 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn events_keep_flowing_during_a_long_solve() {
    let played = Played::start(config(LONG_BUDGET), start_payload()).await;
    let mut game = Game::open_field(12);
    for _ in 0..5 {
        played.server.send(&game.update());
        game.turn += 1;
    }
    // A blocked event loop would not reach the malformed update until the budget ended.
    let started = Instant::now();
    played.server.send(&json!(["game_update", {"turn": "x"}]));
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Fatal(_))), "{result:?}");
    assert!(started.elapsed() < LONG_BUDGET / 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_during_execution_sends_no_further_attacks() {
    let mut played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.tick_until_attack(&mut played.server).await;
    played.shutdown.cancel();
    played.result().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn game_over_during_execution_returns_without_surrender() {
    let mut played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.tick_until_attack(&mut played.server).await;
    played.server.send(&json!(["game_over"]));
    played.result().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn game_lost_without_game_over_reports_eliminated() {
    let bounded = OpeningConfig {
        game_over_timeout: SHORT,
        ..config(QUICK_BUDGET)
    };
    let mut played = Played::start(bounded, start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.tick_until_attack(&mut played.server).await;
    played.server.send(&json!(["game_lost", {"killer": 1}]));
    let result = played.result().await;
    assert!(
        matches!(result, Err(MatchError::Eliminated(_))),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn surrender_without_any_answer_is_fatal() {
    let bounded = OpeningConfig {
        game_over_timeout: SHORT,
        ..config(QUICK_BUDGET)
    };
    let mut played = Played::start(bounded, start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.play_until_surrender(&mut played.server).await;
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Fatal(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn enclosed_general_abandons_without_inventing_a_path() {
    let played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    game.terrain[1] = -2;
    game.terrain[WIDTH] = -2;
    played.server.send(&game.update());
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Abandon(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn zero_budget_abandons() {
    let played = Played::start(config(Duration::ZERO), start_payload()).await;
    played.server.send(&Game::open_field(WIDTH).update());
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Abandon(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_blocked_while_solving_abandons() {
    let played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    let mut game = Game::open_field(WIDTH);
    played.server.send(&game.update());
    // Every other tile turns into a mountain while the solver works from the old map.
    for tile in &mut game.terrain[1..] {
        *tile = -2;
    }
    played.server.send(&game.update());
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Abandon(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn chess_clock_game_abandons_before_solving() {
    let mut start = start_payload();
    start["chessClockConfig"] = json!({
        "clockIncrement": 1.0, "startingClock": 1.0, "decayBufferMs": 1.0,
        "dontLimitMoveSpeed": false,
    });
    let played = Played::start(config(QUICK_BUDGET), start).await;
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Abandon(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_map_update_is_fatal() {
    let played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    played.server.send(&json!(["game_update", {
        "turn": 1, "scores": [], "attackIndex": 0, "generals": [0, -1],
        "map_diff": [9, 9], "cities_diff": [0, 0], "pings": null, "chessClock": null,
    }]));
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Fatal(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn game_start_during_a_match_is_fatal() {
    let played = Played::start(config(QUICK_BUDGET), start_payload()).await;
    played.server.send(&json!(["game_start", start_payload()]));
    let result = played.result().await;
    assert!(matches!(result, Err(MatchError::Fatal(_))), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn session_repeats_matches_with_fresh_state_and_indices() {
    let _solver_slot = SOLVER_SLOTS.acquire().await.unwrap();
    let mut server = FakeServer::start().await;
    let shutdown = CancellationToken::new();
    let session = Session::connect(server.url.clone(), "user".to_owned(), shutdown.clone())
        .await
        .unwrap()
        .with_mode(GameMode::FixedPrivate("room".to_owned()));
    let run = tokio::spawn(session.run(async |context: MatchContext<'_>| {
        play_opening_with(config(QUICK_BUDGET), context).await
    }));
    server.expect("get_username").await;
    server.expect_private_join().await;
    for _ in 0..2 {
        server.send(&json!(["game_start", start_payload()]));
        let mut game = Game::open_field(WIDTH);
        let indices = game.play_until_surrender(&mut server).await;
        assert_eq!(indices.first(), Some(&1), "each match restarts its indices");
        server.send(&json!(["game_lost", {"surrender": true}]));
        server.send(&json!(["game_over"]));
        server.expect_leave().await;
        server.expect_private_join().await;
    }
    shutdown.cancel();
    tokio::time::timeout(LONG, run)
        .await
        .expect("the session should stop")
        .unwrap()
        .unwrap();
    server.expect_only(&[CLIENT_DISCONNECT]).await;
}

/// Runs a real plan tick by tick, reacting to each update with no latency. The solver's
/// schedule must then hold under the simulated rules, which no armies gate may delay.
#[tokio::test(flavor = "multi_thread")]
async fn a_solved_plan_runs_on_schedule_under_the_simulated_rules() {
    let _solver_slot = SOLVER_SLOTS.acquire().await.unwrap();
    let budget = Duration::from_secs(3);
    let mut game = Game::open_field(8);
    let mut state = GameState::default();
    state
        .start(Box::new(serde_json::from_value(start_payload()).unwrap()))
        .unwrap();
    state
        .update(serde_json::from_value(game.update()[1].clone()).unwrap())
        .unwrap();
    let problem = build_problem(&state, budget).unwrap();
    let solution = match opening::solve(&problem, &solve_options(budget)).unwrap() {
        Outcome::Optimal { solution, .. } | Outcome::Feasible { solution, .. } => solution,
        other => panic!("expected a plan, got {other:?}"),
    };
    let mut execution = Execution::new(&solution.paths);
    let mut attack_turns = HashMap::new();
    loop {
        assert!(game.turn <= problem.horizon + 1, "the plan overran");
        let step = execution.observe(&Progress {
            turn: game.turn,
            attack_index: Some(game.attack_index),
            tile: |tile| Tile {
                owned: game.terrain[tile] == 0,
                army: game.armies[tile],
            },
        });
        match step {
            Step::Wait(_) => {}
            Step::Attack(attack) => {
                attack_turns.insert(attack.attack_index, game.turn);
                game.move_army(attack.from, attack.to);
                game.attack_index = i64::from(attack.attack_index);
            }
            Step::Complete => break,
            Step::Deviated(deviation) => panic!("plan deviated: {deviation:?}"),
        }
        game.tick();
    }
    let mut first_move = 0;
    for path in &solution.paths {
        let moves = path.vertices.len() - 1;
        if moves > 0 {
            let attack_index = i32::try_from(first_move + 1).unwrap();
            assert_eq!(
                attack_turns[&attack_index], path.start_turn,
                "late path start"
            );
        }
        first_move += moves;
    }
    let owned = game.terrain.iter().filter(|&&terrain| terrain == 0).count();
    assert_eq!(
        i64::try_from(owned).unwrap(),
        solution.objective + 1,
        "every planned tile should be captured"
    );
}
