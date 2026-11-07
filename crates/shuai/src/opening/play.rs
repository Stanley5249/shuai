//! The match-scoped event and deadline loop.

use std::future::{Future, pending};
use std::pin::Pin;
use std::time::Duration;

use miette::{IntoDiagnostic, Report, miette};
use shuai_client::prelude::*;
use shuai_cpsat::opening::{self, Outcome, Path, Solution};
use sioc::prelude::*;
use tokio::task::JoinError;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::execution::{Execution, Progress, Step, Tile, WaitReason};
use super::problem::{build_problem, check_supported, plan_is_applicable, solve_options};
use super::solve::OpeningSolve;
use crate::game::{GameState, PlayerId, Terrain};
use crate::session::{MatchContext, MatchError, SurrenderOnce, next_event};

const SOLVE_BUDGET: Duration = Duration::from_millis(5700);
const GAME_OVER_TIMEOUT: Duration = Duration::from_secs(10);
const SOLVER_GRACE: Duration = Duration::from_secs(5);
const REACTION_TARGET: Duration = Duration::from_millis(300);

/// The time limits of one Opening match.
#[derive(Debug, Clone, Copy)]
pub struct OpeningConfig {
    /// The solver's budget, which also sets the earliest possible first path.
    pub solve_budget: Duration,
    /// How long to wait for `game_over` once the match should be ending.
    pub game_over_timeout: Duration,
    /// How long to wait for a stopped solver before reporting it.
    pub solver_grace: Duration,
}

impl Default for OpeningConfig {
    fn default() -> Self {
        Self {
            solve_budget: SOLVE_BUDGET,
            game_over_timeout: GAME_OVER_TIMEOUT,
            solver_grace: SOLVER_GRACE,
        }
    }
}

/// Where one match's opening stands.
enum Stage {
    /// No map has arrived, so there is nothing to solve.
    AwaitingMap,
    /// The solver is running while events keep flowing.
    Solving(OpeningSolve),
    /// The plan is being submitted and reconciled.
    Executing(Execution),
    /// No more attacks. Reading until `game_over`, up to the deadline, while a solver
    /// that was asked to stop finishes.
    AwaitingGameOver {
        deadline: Instant,
        stopping: Option<OpeningSolve>,
    },
}

impl Stage {
    fn name(&self) -> &'static str {
        match self {
            Self::AwaitingMap => "awaiting_map",
            Self::Solving(_) => "solving",
            Self::Executing(_) => "executing",
            Self::AwaitingGameOver { .. } => "awaiting_game_over",
        }
    }
}

/// The one local send that may be in progress, so backpressure cannot stop event reads.
struct PendingSend<'a> {
    send: Pin<Box<dyn Future<Output = sioc::error::Result<()>> + Send + 'a>>,
    /// When the update behind an attack arrived, and when the attack was decided.
    attack_timing: Option<(Instant, Instant)>,
}

/// Plays one match with the default limits. See [`play_opening_with`].
///
/// # Errors
/// See [`play_opening_with`].
pub async fn play_opening(context: MatchContext<'_>) -> Result<(), MatchError> {
    play_opening_with(OpeningConfig::default(), context).await
}

/// Solves, executes and surrenders the opening of one match.
///
/// Returns `Ok(())` after `game_over`, or when `shutdown` is requested, and only once
/// any solver has been stopped and seen to end.
///
/// # Errors
/// Returns a classified [`MatchError`] when the opening cannot continue. A solver that
/// fails to stop is [`MatchError::Fatal`], whatever the match did, because the next
/// match must not start while it runs.
#[instrument(skip_all)]
pub async fn play_opening_with(
    config: OpeningConfig,
    context: MatchContext<'_>,
) -> Result<(), MatchError> {
    let MatchContext {
        start,
        rx,
        tx,
        surrender,
        shutdown,
    } = context;
    let mut run = OpeningRun {
        config,
        rx,
        tx,
        surrender,
        shutdown,
        stage: Stage::AwaitingMap,
        reactions: Reactions::default(),
        last_wait: None,
    };
    let result = run.drive(start).await;
    run.finish(result).await
}

/// One match's opening. Its state outlives [`OpeningRun::drive`], so that
/// [`OpeningRun::finish`] cleans up however that returned.
struct OpeningRun<'a> {
    config: OpeningConfig,
    rx: &'a mut SocketReceiver,
    tx: &'a GeneralsIoSender,
    surrender: &'a SurrenderOnce,
    shutdown: &'a CancellationToken,
    stage: Stage,
    reactions: Reactions,
    /// The reason last logged, so a wait is reported when it begins.
    last_wait: Option<WaitReason>,
}

impl<'a> OpeningRun<'a> {
    async fn drive(&mut self, start: Box<GameStartData>) -> Result<(), MatchError> {
        check_supported(&start).map_err(abandon)?;
        let mut game = GameState::default();
        game.start(start)
            .into_diagnostic()
            .map_err(MatchError::Fatal)?;
        let mut pending_send: Option<PendingSend<'a>> = None;
        // What the next decision reacts to: the latest update, or a plan being accepted.
        let mut decision_trigger = Instant::now();
        let mut eliminated = false;
        loop {
            let game_over_deadline = match &self.stage {
                Stage::AwaitingGameOver { deadline, .. } => Some(*deadline),
                _ => None,
            };
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => {
                    tracing::info!("stopping the opening for shutdown");
                    return Ok(());
                }
                event = next_event(self.rx) => match event? {
                    GeneralsIoEvent::GameOver(_) => return Ok(()),
                    GeneralsIoEvent::GameUpdate(Event { payload, .. }) => {
                        decision_trigger = Instant::now();
                        game
                            .update(payload.data)
                            .into_diagnostic()
                            .map_err(MatchError::Fatal)?;
                        if matches!(self.stage, Stage::AwaitingMap) {
                            let budget = self.config.solve_budget;
                            let problem = build_problem(&game, budget).map_err(abandon)?;
                            tracing::info!(turn = current_turn(&game), ?budget, "solve started");
                            self.stage = Stage::Solving(OpeningSolve::start(problem, solve_options(budget)));
                        }
                    }
                    GeneralsIoEvent::GameLost(Event { payload, .. }) => {
                        tracing::info!(?payload.data, "local player eliminated");
                        eliminated = true;
                        end_opening(&mut self.stage, &mut pending_send, self.config);
                    }
                    GeneralsIoEvent::GameWon(_) => {
                        tracing::info!("local player won");
                        end_opening(&mut self.stage, &mut pending_send, self.config);
                    }
                    GeneralsIoEvent::GameStart(_) => {
                        return Err(MatchError::Fatal(miette!("game_start during an active match")));
                    }
                    _ => {}
                },
                result = solver_finished(&mut self.stage) => {
                    if let Stage::AwaitingGameOver { stopping, .. } = &mut self.stage {
                        *stopping = None;
                        log_stopped_solver(&result);
                    } else {
                        let execution = accept_outcome(result, &game)?;
                        tracing::info!(
                            turn = current_turn(&game),
                            first_planned_turn = execution.planned_turn(),
                            move_count = execution.move_count(),
                            "plan accepted"
                        );
                        decision_trigger = Instant::now();
                        self.stage = Stage::Executing(execution);
                    }
                }
                result = send_finished(&mut pending_send) => {
                    result.map_err(|error| MatchError::Fatal(Report::new(error)))?;
                    let finished = pending_send.take().expect("only a pending send finishes");
                    if let Some((received, decided)) = finished.attack_timing {
                        self.reactions.record(decided - received, received.elapsed());
                    }
                }
                () = sleep_until_deadline(game_over_deadline) => {
                    return Err(if eliminated {
                        MatchError::Eliminated(miette!("game_over did not follow game_lost"))
                    } else {
                        MatchError::Fatal(miette!("the match end was not confirmed in time"))
                    });
                }
            }
            if pending_send.is_none() {
                pending_send = self.next_send(&game, decision_trigger)?;
            }
        }
    }

    /// Asks the plan what the latest update allows, and returns the send it requires.
    fn next_send(
        &mut self,
        game: &GameState,
        decision_trigger: Instant,
    ) -> Result<Option<PendingSend<'a>>, MatchError> {
        let Stage::Executing(execution) = &mut self.stage else {
            return Ok(None);
        };
        let Some(step) = observe(execution, game) else {
            return Ok(None);
        };
        let turn = current_turn(game);
        match step {
            Step::Wait(reason) => {
                if self.last_wait != Some(reason) {
                    tracing::debug!(turn, ?reason, "plan waiting");
                    self.last_wait = Some(reason);
                }
                Ok(None)
            }
            Step::Attack(attack) => {
                self.last_wait = None;
                if let Some(planned_turn) = execution.planned_turn() {
                    self.reactions.record_lateness(turn - planned_turn);
                    tracing::debug!(
                        turn,
                        planned_turn,
                        attack.from,
                        attack.to,
                        attack.attack_index,
                        "submitting attack"
                    );
                }
                let from = i32::try_from(attack.from).expect("map tiles fit the wire type");
                let to = i32::try_from(attack.to).expect("map tiles fit the wire type");
                Ok(Some(PendingSend {
                    send: Box::pin(self.tx.attack(from, to, false, attack.attack_index)),
                    attack_timing: Some((decision_trigger, Instant::now())),
                }))
            }
            Step::Complete => {
                tracing::info!(turn, "opening completed, surrendering");
                self.stage = Stage::AwaitingGameOver {
                    deadline: Instant::now() + self.config.game_over_timeout,
                    stopping: None,
                };
                Ok(Some(PendingSend {
                    send: Box::pin(self.surrender.send(self.tx)),
                    attack_timing: None,
                }))
            }
            Step::Deviated(deviation) => Err(abandon(miette!(
                "plan deviated at turn {turn}, on {}: {deviation:?}",
                execution.current_move()
            ))),
        }
    }

    /// Waits for any solver and reports how the opening ended. A failure to stop the
    /// solver replaces `result`, because the next match must not start.
    async fn finish(self, result: Result<(), MatchError>) -> Result<(), MatchError> {
        self.reactions.log(self.stage.name());
        let cleanup = match self.stage {
            Stage::Solving(solve)
            | Stage::AwaitingGameOver {
                stopping: Some(solve),
                ..
            } => solve.reap(self.config.solver_grace).await,
            _ => Ok(()),
        };
        match (result, cleanup) {
            (result, Ok(())) => result,
            (Ok(()), Err(cleanup)) => Err(MatchError::Fatal(cleanup)),
            (Err(error), Err(cleanup)) => {
                tracing::error!(?error, "match error replaced by a solver cleanup failure");
                Err(MatchError::Fatal(cleanup))
            }
        }
    }
}

fn current_turn(game: &GameState) -> i64 {
    game.observation().map_or(0, |observation| observation.turn)
}

fn observe(execution: &mut Execution, game: &GameState) -> Option<Step> {
    let observation = game.observation()?;
    let map = game.map()?;
    let player = PlayerId::try_from(game.metadata()?.player_index).ok()?;
    let armies = map.view().armies;
    let width = armies.width();
    Some(execution.observe(&Progress {
        turn: observation.turn,
        attack_index: observation.attack_index,
        tile: |tile: usize| Tile {
            owned: map.terrain(tile / width, tile % width) == Some(Terrain::Player(player)),
            army: armies[tile],
        },
    }))
}

/// Stops further attacks and any solve, and starts the deadline for `game_over` unless
/// the opening's own surrender already started it. A blocked attack is dropped without
/// a retry, because it can no longer serve a match with a known result.
fn end_opening(
    stage: &mut Stage,
    pending_send: &mut Option<PendingSend<'_>>,
    config: OpeningConfig,
) {
    if matches!(stage, Stage::AwaitingGameOver { .. }) {
        return;
    }
    *pending_send = None;
    let deadline = Instant::now() + config.game_over_timeout;
    let awaiting = Stage::AwaitingGameOver {
        deadline,
        stopping: None,
    };
    if let Stage::Solving(solve) = std::mem::replace(stage, awaiting) {
        solve.request_stop();
        *stage = Stage::AwaitingGameOver {
            deadline,
            stopping: Some(solve),
        };
    }
}

/// Reports a solver that ended after the opening stopped needing it. Its result has no
/// use, and a failure cannot affect the next match, because the worker has ended.
fn log_stopped_solver(result: &Result<Result<Outcome, opening::Error>, JoinError>) {
    match result {
        Ok(Ok(_)) => tracing::debug!("stopped solver ended"),
        Ok(Err(error)) => tracing::warn!(%error, "stopped solver failed"),
        Err(error) => tracing::warn!(%error, "stopped solver worker failed"),
    }
}

async fn solver_finished(stage: &mut Stage) -> Result<Result<Outcome, opening::Error>, JoinError> {
    match stage {
        Stage::Solving(solve)
        | Stage::AwaitingGameOver {
            stopping: Some(solve),
            ..
        } => solve.finished().await,
        _ => pending().await,
    }
}

async fn send_finished(pending_send: &mut Option<PendingSend<'_>>) -> sioc::error::Result<()> {
    match pending_send {
        Some(pending) => pending.send.as_mut().await,
        None => pending().await,
    }
}

async fn sleep_until_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => pending().await,
    }
}

/// Accepts a solved plan if it still applies to the latest map.
fn accept_outcome(
    result: Result<Result<Outcome, opening::Error>, JoinError>,
    game: &GameState,
) -> Result<Execution, MatchError> {
    let outcome = result
        .map_err(|error| MatchError::Fatal(miette!("solver worker failed: {error}")))?
        .map_err(|error| match error {
            opening::Error::Input(_) => abandon(error),
            error => MatchError::Fatal(miette!("solver failed: {error}")),
        })?;
    let solution = match outcome {
        Outcome::Optimal { solution, timings } => {
            tracing::info!(objective = solution.objective, ?timings, "solved optimally");
            solution
        }
        Outcome::Feasible { solution, timings } => {
            tracing::info!(objective = solution.objective, ?timings, "solved feasibly");
            solution
        }
        Outcome::Infeasible { timings } => {
            return Err(abandon(miette!("no feasible opening, after {timings:?}")));
        }
        Outcome::NoIncumbent { status, timings } => {
            return Err(abandon(miette!(
                "no incumbent opening: {status:?}, after {timings:?}"
            )));
        }
        Outcome::BudgetExhausted { timings } => {
            return Err(abandon(miette!(
                "solve budget exhausted, after {timings:?}"
            )));
        }
    };
    let Solution { paths, .. } = &solution;
    check_applicable(paths, game)?;
    Ok(Execution::new(paths))
}

fn check_applicable(paths: &[Path], game: &GameState) -> Result<(), MatchError> {
    let applicable = game.map().is_some_and(|map| {
        game.metadata()
            .and_then(|start| PlayerId::try_from(start.player_index).ok())
            .is_some_and(|player| {
                plan_is_applicable(paths.iter().flat_map(|path| &path.vertices), map, player)
            })
    });
    if applicable {
        Ok(())
    } else {
        Err(abandon(miette!("solved plan no longer fits the map")))
    }
}

fn abandon(reason: impl std::fmt::Display) -> MatchError {
    MatchError::Abandon(miette!("{reason}"))
}

/// Summarizes the attacks of one opening. A decision is when the bot chose and began
/// submitting, after the update or the accepted plan that allowed it. Acceptance is when
/// `sioc` took the packet, which can be later under backpressure. Neither says when the
/// server received or ran the move. `most_ticks_late` is the departure from the solver's
/// schedule, which includes legitimate waits for armies, so it is not a reaction delay.
#[derive(Debug, Default)]
struct Reactions {
    attacks: u32,
    missed_target: u32,
    slowest_decision: Duration,
    slowest_acceptance: Duration,
    most_ticks_late: i64,
}

impl Reactions {
    fn record(&mut self, decision: Duration, acceptance: Duration) {
        self.attacks += 1;
        self.slowest_decision = self.slowest_decision.max(decision);
        self.slowest_acceptance = self.slowest_acceptance.max(acceptance);
        if decision > REACTION_TARGET {
            self.missed_target += 1;
            tracing::warn!(?decision, "local decision missed its target");
        }
    }

    fn record_lateness(&mut self, ticks_late: i64) {
        self.most_ticks_late = self.most_ticks_late.max(ticks_late);
    }

    fn log(&self, stage: &str) {
        tracing::info!(
            stage,
            attacks = self.attacks,
            missed_target = self.missed_target,
            slowest_decision = ?self.slowest_decision,
            slowest_acceptance = ?self.slowest_acceptance,
            most_ticks_late = self.most_ticks_late,
            "opening ended"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;

    use bytestring::ByteString;
    use serde_json::{Value, json};
    use shuai_cpsat::opening::{Solution, Timings};

    use super::*;
    use crate::test_support::FakeServer;

    /// Longer than `REACTION_TARGET`, so measuring from the wrong event misses the target.
    const GAP: Duration = Duration::from_millis(400);
    const START_TURN: i64 = 5;

    fn start_payload() -> Box<GameStartData> {
        Box::new(
            serde_json::from_value(json!({
                "playerIndex": 0, "chat_room": "room", "usernames": ["a", "b"],
                "game_type": "custom", "options": {}, "replay_id": "replay",
                "team_chat_room": null, "team_2v2_play_again": null,
                "big_team_play_again": null, "chessClock": null, "chessClockConfig": null,
            }))
            .unwrap(),
        )
    }

    fn update(turn: i64) -> Value {
        // A 2x2 map, with a general on tile 0 that has armies to move.
        json!(["game_update", {
            "turn": turn, "scores": [], "attackIndex": 0, "generals": [0, -1],
            "map_diff": [0, 14, 2, 2, 3, 0, 0, 0, 0, -3, -3, -3, 0, 0, 0, 0],
            "cities_diff": [0, 0], "pings": null, "chessClock": null,
        }])
    }

    /// Plays with a solver worker that finishes only when told to, and returns the
    /// measurements after `script` has run and ended the match.
    async fn reactions_during(
        script: impl AsyncFnOnce(&mut FakeServer, mpsc::Sender<()>),
    ) -> Reactions {
        let mut server = FakeServer::start().await;
        let client = ClientBuilder::new(server.url.clone())
            .transport(TransportStrategy::WebSocket)
            .open()
            .unwrap();
        let (socket_tx, mut rx) = client.connect(ByteString::from_static("/")).await.unwrap();
        assert!(matches!(rx.recv().await, Some(Signal::Connect(_))));
        let tx = GeneralsIoSender::new(socket_tx);

        let (release, released) = mpsc::channel();
        let worker = tokio::task::spawn_blocking(move || {
            released.recv().unwrap();
            Ok(Outcome::Optimal {
                solution: Solution {
                    paths: vec![Path {
                        start_turn: START_TURN,
                        duration: 1,
                        vertices: vec![0, 1],
                    }],
                    objective: 1,
                },
                timings: Timings::default(),
            })
        });
        let surrender = SurrenderOnce::default();
        let shutdown = CancellationToken::new();
        let mut run = OpeningRun {
            config: OpeningConfig::default(),
            rx: &mut rx,
            tx: &tx,
            surrender: &surrender,
            shutdown: &shutdown,
            stage: Stage::Solving(OpeningSolve::from_worker(worker)),
            reactions: Reactions::default(),
            last_wait: None,
        };
        let (result, ()) = tokio::join!(run.drive(start_payload()), script(&mut server, release));
        result.unwrap();
        run.reactions
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_decision_at_plan_acceptance_is_not_timed_from_the_older_update() {
        let reactions = reactions_during(async |server, release| {
            server.send(&update(START_TURN));
            tokio::time::sleep(GAP).await;
            release.send(()).unwrap();
            assert_eq!(server.expect_next_name().await, "attack");
            server.send(&json!(["game_over"]));
        })
        .await;
        assert_eq!(reactions.attacks, 1);
        assert_eq!(reactions.missed_target, 0, "{reactions:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_decision_at_a_later_update_is_not_timed_from_plan_acceptance() {
        let reactions = reactions_during(async |server, release| {
            server.send(&update(START_TURN - 1));
            // The plan is checked against the map, so the update must arrive first.
            tokio::time::sleep(GAP).await;
            release.send(()).unwrap();
            tokio::time::sleep(GAP).await;
            server.send(&update(START_TURN));
            assert_eq!(server.expect_next_name().await, "attack");
            server.send(&json!(["game_over"]));
        })
        .await;
        assert_eq!(reactions.attacks, 1);
        assert_eq!(reactions.missed_target, 0, "{reactions:?}");
    }

    /// A send that never finishes, as under backpressure, holding `token` until dropped.
    fn blocked_send(
        token: &Arc<()>,
        attack_timing: Option<(Instant, Instant)>,
    ) -> PendingSend<'static> {
        let token = Arc::clone(token);
        PendingSend {
            send: Box::pin(async move {
                pending::<()>().await;
                drop(token);
                Ok(())
            }),
            attack_timing,
        }
    }

    #[test]
    fn a_result_event_drops_a_blocked_attack_without_a_retry() {
        let token = Arc::new(());
        let now = Instant::now();
        let mut pending_send = Some(blocked_send(&token, Some((now, now))));
        let mut stage = Stage::Executing(Execution::new(&[]));
        end_opening(&mut stage, &mut pending_send, OpeningConfig::default());
        assert!(pending_send.is_none());
        assert_eq!(
            Arc::strong_count(&token),
            1,
            "the blocked attack should be dropped"
        );
        assert!(matches!(stage, Stage::AwaitingGameOver { .. }));
    }

    #[test]
    fn a_result_event_keeps_a_surrender_that_is_being_sent() {
        let token = Arc::new(());
        let deadline = Instant::now();
        let mut pending_send = Some(blocked_send(&token, None));
        let mut stage = Stage::AwaitingGameOver {
            deadline,
            stopping: None,
        };
        end_opening(&mut stage, &mut pending_send, OpeningConfig::default());
        assert!(pending_send.is_some());
        assert!(matches!(
            stage,
            Stage::AwaitingGameOver { deadline: kept, .. } if kept == deadline
        ));
    }
}
