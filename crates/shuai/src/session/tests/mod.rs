mod fake_server;

use std::collections::HashSet;
use std::time::Duration;

use miette::miette;
use serde_json::{Value, json};
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use self::fake_server::{CLIENT_DISCONNECT, FakeServer};
use super::*;

const LONG: Duration = Duration::from_secs(60);
const SHORT: Duration = Duration::from_millis(50);
const SURRENDER: &str = "surrender";

fn game_start(room: &str) -> Value {
    json!(["game_start", {
        "playerIndex": 0, "chat_room": room, "usernames": ["a", "b"],
        "game_type": "custom", "options": {}, "replay_id": null, "team_chat_room": null,
        "team_2v2_play_again": null, "big_team_play_again": null,
        "chessClock": null, "chessClockConfig": null,
    }])
}

fn game_over() -> Value {
    json!(["game_over"])
}

fn disable_rematch() -> Value {
    json!(["disable_rematch"])
}

fn fixed() -> GameMode {
    GameMode::FixedPrivate("room".to_owned())
}

/// A gameplay stub that plays until `GameOver`, or until the test makes it fail.
async fn scripted_play(
    start: Box<GameStartData>,
    rx: &mut SocketReceiver,
    played: &mpsc::UnboundedSender<String>,
    failures: &mut mpsc::UnboundedReceiver<MatchError>,
    shutdown: &CancellationToken,
) -> Result<(), MatchError> {
    played.send(start.chat_room).unwrap();
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return Ok(()),
            Some(error) = failures.recv() => return Err(error),
            event = next_event(rx) => {
                if let GeneralsIoEvent::GameOver(_) = event? {
                    return Ok(());
                }
            }
        }
    }
}

/// Runs a session against the fake server.
struct Harness {
    server: FakeServer,
    shutdown: CancellationToken,
    played: mpsc::UnboundedReceiver<String>,
    failures: mpsc::UnboundedSender<MatchError>,
    run: JoinHandle<miette::Result<()>>,
}

impl Harness {
    /// Starts a session and consumes its initial username request.
    async fn start(mode: GameMode) -> Self {
        Self::start_with_timeouts(mode, LONG, LONG).await
    }

    /// Like [`Harness::start`], with explicit rematch and exit timeouts.
    async fn start_with_timeouts(
        mode: GameMode,
        rematch_timeout: Duration,
        exit_timeout: Duration,
    ) -> Self {
        let server = FakeServer::start().await;
        let mut h = Self::start_on(server, mode, rematch_timeout, exit_timeout).await;
        h.server.expect("get_username").await;
        h
    }

    async fn start_on(
        server: FakeServer,
        mode: GameMode,
        rematch_timeout: Duration,
        exit_timeout: Duration,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let session = Session::connect(server.url.clone(), "user".to_owned(), shutdown.clone())
            .await
            .unwrap()
            .with_mode(mode)
            .with_rematch_timeout(rematch_timeout)
            .with_exit_timeout(exit_timeout);
        let (played_tx, played) = mpsc::unbounded_channel();
        let (failures, mut failures_rx) = mpsc::unbounded_channel();
        let run = tokio::spawn(session.run(
            async move |start: Box<GameStartData>,
                        rx: &mut SocketReceiver,
                        _tx: &GeneralsIoSender,
                        shutdown: &CancellationToken| {
                scripted_play(start, rx, &played_tx, &mut failures_rx, shutdown).await
            },
        ));
        Self {
            server,
            shutdown,
            played,
            failures,
            run,
        }
    }

    /// Starts a match and returns once gameplay has received it.
    async fn start_match(&mut self, chat_room: &str) {
        self.server.send(&game_start(chat_room));
        let played = tokio::time::timeout(LONG, self.played.recv())
            .await
            .expect("a match should start")
            .expect("the session should still run");
        assert_eq!(played, chat_room);
    }

    /// Fails the running match, as gameplay would.
    fn fail_match(&self, error: MatchError) {
        self.failures.send(error).unwrap();
    }

    /// Returns the session result after requiring the commands that end the transcript.
    async fn finish(self, remaining: &[&str]) -> miette::Result<()> {
        let result = tokio::time::timeout(LONG, self.run)
            .await
            .expect("Session::run should return")
            .unwrap();
        self.server.expect_only(remaining).await;
        result
    }
}

#[tokio::test]
async fn fixed_room_id_persists_across_requeues() {
    let mut h = Harness::start(fixed()).await;
    for round in 0..3 {
        assert_eq!(h.server.expect_private_join().await, "room");
        h.start_match(&format!("game{round}")).await;
        h.server.send(&game_over());
        h.server.expect("rematch").await;
        h.server.send(&disable_rematch());
        h.server.expect("leave_game").await;
    }
    assert_eq!(h.server.expect_private_join().await, "room");
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn random_room_id_changes_on_requeue_but_not_within_a_rematch() {
    let mut h = Harness::start(GameMode::RandomPrivate).await;
    let mut room_ids = HashSet::new();
    for round in 0..3 {
        let room_id = h.server.expect_private_join().await;
        room_ids.insert(room_id.as_str().unwrap().to_owned());
        h.start_match(&format!("game{round}")).await;
        h.server.send(&game_over());
        h.server.expect("rematch").await;

        // An accepted rematch stays in the room, so strict ordering proves that no
        // leave or join precedes this second match.
        h.start_match(&format!("rematch{round}")).await;
        h.server.send(&game_over());
        h.server.expect("rematch").await;

        h.server.send(&disable_rematch());
        h.server.expect("leave_game").await;
    }
    h.server.expect_private_join().await;
    assert!(room_ids.len() > 1, "requeues must create new room IDs");
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn one_vs_one_rejoins_its_queue() {
    let mut h = Harness::start(GameMode::OneVsOne).await;
    assert_eq!(h.server.expect("join_1v1").await[0], "user");
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    h.server.send(&disable_rematch());
    h.server.expect("leave_game").await;
    h.server.expect("join_1v1").await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn rematch_timeout_requeues() {
    let mut h = Harness::start_with_timeouts(fixed(), SHORT, LONG).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    h.server.expect("leave_game").await;
    h.server.expect_private_join().await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn game_start_during_rematch_reaches_the_next_match() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    // These belong to the finished match, so the rematch phase must consume them
    // rather than end the next match with the stale GameOver.
    h.server.send(&json!(["rematch_update", 1, true]));
    h.server.send(&game_over());
    h.start_match("game1").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn shutdown_in_queue_only_disconnects() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn shutdown_in_game_surrenders_then_disconnects() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.shutdown.cancel();
    h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn shutdown_in_rematch_only_disconnects() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn fatal_match_error_cleans_up_without_requeueing() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Fatal(miette!("invariant broken")));
    let error = h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap_err();
    assert!(error.to_string().contains("invariant broken"));
}

#[tokio::test]
async fn abandon_requeues_only_after_game_over() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Abandon(miette!("no plan")));
    h.server.expect(SURRENDER).await;
    h.server.send(&game_over());
    h.server.expect("leave_game").await;
    h.server.expect_private_join().await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn abandon_without_server_confirmation_is_fatal() {
    let mut h = Harness::start_with_timeouts(fixed(), LONG, SHORT).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Abandon(miette!("no plan")));
    // The match may still be active, so cleanup makes its own best-effort attempt.
    let error = h
        .finish(&[SURRENDER, SURRENDER, CLIENT_DISCONNECT])
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref(),
        Some(LifecycleError::ExitUnconfirmed)
    ));
}

#[tokio::test]
async fn abandon_confirmed_by_game_lost_requeues_without_game_over() {
    let mut h = Harness::start_with_timeouts(fixed(), LONG, SHORT).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Abandon(miette!("no plan")));
    h.server.expect(SURRENDER).await;
    h.server.send(&json!(["game_lost", {"surrender": true}]));
    h.server.expect("leave_game").await;
    h.server.expect_private_join().await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn eliminated_requeues_without_surrendering_again() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Eliminated(miette!("no game over")));
    h.server.expect("leave_game").await;
    h.server.expect_private_join().await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn missing_username_fails_early_and_still_releases_the_client() {
    let server = FakeServer::start_with_username(None).await;
    let h = Harness::start_on(server, fixed(), LONG, LONG).await;
    let error = h
        .finish(&["get_username", CLIENT_DISCONNECT])
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref(),
        Some(LifecycleError::UsernameNotSet)
    ));
}

#[tokio::test]
async fn server_disconnect_in_queue_is_an_error_and_does_not_hang() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.server.disconnect_client();
    let error = h.finish(&[]).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref(),
        Some(SignalError::Disconnected)
    ));
}

#[tokio::test]
async fn signal_policy_error_in_queue_cleans_up() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.server.send(&json!(["game_start", {"playerIndex": "x"}]));
    let error = h.finish(&[CLIENT_DISCONNECT]).await.unwrap_err();
    assert!(error.to_string().contains("game_start"));
}

/// Plays one match that returns `Fatal` once `prepare` is done, after the server
/// has followed `GameStart` with `extra_events` unrelated events.
async fn run_one_failing_match(
    server: &mut FakeServer,
    extra_events: usize,
    prepare: impl AsyncFnOnce(&mut SocketReceiver, &GeneralsIoSender),
) -> miette::Result<()> {
    let session = Session::connect(
        server.url.clone(),
        "user".to_owned(),
        CancellationToken::new(),
    )
    .await
    .unwrap()
    .with_mode(fixed());
    let mut prepare = Some(prepare);
    let run = session.run(
        async |_start: Box<GameStartData>,
               rx: &mut SocketReceiver,
               tx: &GeneralsIoSender,
               _shutdown: &CancellationToken| {
            (prepare.take().expect("only one match runs"))(rx, tx).await;
            Err(MatchError::Fatal(miette!("stop")))
        },
    );
    let script = async {
        server.expect("get_username").await;
        server.expect_private_join().await;
        server.send(&game_start("game0"));
        for _ in 0..extra_events {
            server.send(&json!(["pong_server"]));
        }
    };
    let (result, ()) = tokio::join!(run, script);
    result
}

#[tokio::test]
async fn cleanup_finishes_while_the_receiver_channel_is_full() {
    let mut server = FakeServer::start().await;
    let run = run_one_failing_match(&mut server, 64, async |rx, _tx| {
        // The manager now blocks on this channel, and only draining can release it.
        while rx.capacity() > 0 {
            tokio::task::yield_now().await;
        }
    });
    let result = tokio::time::timeout(LONG, run)
        .await
        .expect("cleanup should drain the receiver");
    assert!(result.is_err());
    server.expect_only(&[SURRENDER, CLIENT_DISCONNECT]).await;
}

#[tokio::test]
async fn sender_clone_kept_by_gameplay_does_not_hang_cleanup() {
    let mut server = FakeServer::start().await;
    let mut kept_sender = None;
    let run = run_one_failing_match(&mut server, 0, async |_rx, tx| {
        kept_sender = Some(tx.clone());
    });
    let result = tokio::time::timeout(LONG, run)
        .await
        .expect("cleanup should not wait for the kept sender");
    assert!(result.is_err());
    assert!(kept_sender.is_some());
    server.expect_only(&[SURRENDER, CLIENT_DISCONNECT]).await;
}
