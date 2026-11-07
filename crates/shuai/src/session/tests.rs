use std::collections::HashSet;
use std::num::NonZeroU32;
use std::time::Duration;

use miette::miette;
use serde_json::{Value, json};
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::test_support::{CLIENT_DISCONNECT, FakeServer};

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

fn fixed() -> QueueMode {
    QueueMode::FixedPrivate("room".to_owned())
}

/// A gameplay stub that plays until `GameOver`, or until the test makes it fail.
async fn scripted_play(
    context: MatchContext<'_>,
    played: &mpsc::UnboundedSender<String>,
    failures: &mut mpsc::UnboundedReceiver<MatchError>,
) -> Result<(), MatchError> {
    played.send(context.start.chat_room).unwrap();
    loop {
        tokio::select! {
            biased;
            () = context.shutdown.cancelled() => return Ok(()),
            Some(error) = failures.recv() => return Err(error),
            event = next_event(context.rx) => {
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
    async fn start(mode: QueueMode) -> Self {
        Self::start_with_timeouts(mode, LONG, LONG).await
    }

    /// Like [`Harness::start`], with explicit rematch and exit timeouts.
    async fn start_with_timeouts(
        mode: QueueMode,
        rematch_timeout: Duration,
        surrender_timeout: Duration,
    ) -> Self {
        let server = FakeServer::start().await;
        let mut h = Self::start_on(server, mode, rematch_timeout, surrender_timeout).await;
        h.server.expect("get_username").await;
        h
    }

    /// Starts a session that stops after one match, with a short exit timeout.
    async fn start_with_one_match_limit() -> Self {
        let server = FakeServer::start().await;
        let mut h = Self::start_limited(server, Some(NonZeroU32::MIN), fixed(), LONG, SHORT).await;
        h.server.expect("get_username").await;
        h.server.expect_private_join().await;
        h
    }

    async fn start_on(
        server: FakeServer,
        mode: QueueMode,
        rematch_timeout: Duration,
        surrender_timeout: Duration,
    ) -> Self {
        Self::start_limited(server, None, mode, rematch_timeout, surrender_timeout).await
    }

    async fn start_limited(
        server: FakeServer,
        match_limit: Option<NonZeroU32>,
        mode: QueueMode,
        rematch_timeout: Duration,
        surrender_timeout: Duration,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let mut session = Session::connect(server.url.clone(), "user".to_owned(), shutdown.clone())
            .await
            .unwrap()
            .with_queue_mode(mode)
            .with_rematch_timeout(rematch_timeout)
            .with_surrender_timeout(surrender_timeout);
        if let Some(limit) = match_limit {
            session = session.with_match_limit(limit);
        }
        let (played_tx, played) = mpsc::unbounded_channel();
        let (failures, mut failures_rx) = mpsc::unbounded_channel();
        let run = tokio::spawn(session.run(async move |context: MatchContext<'_>| {
            scripted_play(context, &played_tx, &mut failures_rx).await
        }));
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
        h.server.expect_leave().await;
    }
    assert_eq!(h.server.expect_private_join().await, "room");
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn random_room_id_changes_for_every_match() {
    let mut h = Harness::start(QueueMode::RandomPrivate).await;
    let mut room_ids = HashSet::new();
    for round in 0..3 {
        let room_id = h.server.expect_private_join().await;
        room_ids.insert(room_id.as_str().unwrap().to_owned());
        h.start_match(&format!("game{round}")).await;
        h.server.send(&game_over());
        h.server.expect_leave().await;
    }
    h.server.expect_private_join().await;
    assert!(room_ids.len() > 1, "requeues must create new room IDs");
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn one_vs_one_rejoins_its_queue() {
    let mut h = Harness::start(QueueMode::OneVsOne).await;
    assert_eq!(h.server.expect("join_1v1").await[0], "user");
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    h.server.send(&disable_rematch());
    h.server.expect_leave().await;
    h.server.expect("join_1v1").await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn rematch_timeout_requeues() {
    let mut h = Harness::start_with_timeouts(QueueMode::OneVsOne, SHORT, LONG).await;
    h.server.expect("join_1v1").await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("rematch").await;
    h.server.expect_leave().await;
    h.server.expect("join_1v1").await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn game_start_during_rematch_reaches_the_next_match() {
    let mut h = Harness::start(QueueMode::OneVsOne).await;
    h.server.expect("join_1v1").await;
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
    let mut h = Harness::start(QueueMode::OneVsOne).await;
    h.server.expect("join_1v1").await;
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
    h.server.expect_leave().await;
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
    let error = h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref(),
        Some(LifecycleError::SurrenderUnconfirmed)
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
    h.server.expect_leave().await;
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
    h.server.expect_leave().await;
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
async fn successful_registration_answer_does_not_stop_the_session() {
    let server = FakeServer::start_with_username(None).await;
    let shutdown = CancellationToken::new();
    let session = Session::connect(server.url.clone(), "user".to_owned(), shutdown.clone())
        .await
        .unwrap()
        .with_username("name".to_owned())
        .with_queue_mode(fixed());
    let mut server = server;
    let run = tokio::spawn(session.run(async |_context: MatchContext<'_>| Ok(())));
    server.expect("get_username").await;
    assert_eq!(server.expect("set_username").await[1], "name");
    server.send(&json!(["error_set_username", ""]));
    server.expect_private_join().await;
    shutdown.cancel();
    tokio::time::timeout(LONG, run)
        .await
        .expect("the session should stop")
        .unwrap()
        .unwrap();
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

#[tokio::test]
async fn late_game_start_from_a_left_match_is_discarded() {
    let server = FakeServer::start_without_username_ack().await;
    let mut h = Harness::start_on(server, fixed(), SHORT, LONG).await;
    let id = h.server.expect("get_username").await[1].clone();
    h.server.acknowledge_username(&id, Some("bot"));
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("leave_game").await;
    let id = h.server.expect("get_username").await[1].clone();

    // The server emitted this before it handled the request, so it is from the old match.
    h.server.send(&game_start("late"));
    h.server.acknowledge_username(&id, Some("bot"));
    h.server.expect_private_join().await;

    // `start_match` requires this to be the next match that gameplay receives.
    h.start_match("game1").await;
    h.shutdown.cancel();
    h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn game_start_after_requeue_is_played_without_a_queue_update() {
    for mode in [fixed(), QueueMode::OneVsOne] {
        let join = if mode == fixed() {
            "join_private"
        } else {
            "join_1v1"
        };
        let mut h = Harness::start_with_timeouts(mode, SHORT, LONG).await;
        h.server.expect(join).await;
        if join == "join_private" {
            h.server.expect("set_force_start").await;
        }
        h.start_match("game0").await;
        h.server.send(&game_over());
        if join == "join_1v1" {
            h.server.expect("rematch").await;
        }
        h.server.expect_leave().await;
        h.server.expect(join).await;
        if join == "join_private" {
            h.server.expect("set_force_start").await;
        }
        h.start_match("game1").await;
        h.shutdown.cancel();
        h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap();
    }
}

#[tokio::test]
async fn abandon_and_elimination_requeues_resume_gameplay() {
    for failure in [
        MatchError::Abandon(miette!("no plan")),
        MatchError::Eliminated(miette!("no game over")),
    ] {
        let abandon = matches!(failure, MatchError::Abandon(_));
        let mut h = Harness::start(fixed()).await;
        h.server.expect_private_join().await;
        h.start_match("game0").await;
        h.fail_match(failure);
        if abandon {
            h.server.expect(SURRENDER).await;
            h.server.send(&game_over());
        }
        h.server.expect_leave().await;
        h.server.expect_private_join().await;
        h.start_match("game1").await;
        h.shutdown.cancel();
        h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap();
    }
}

#[tokio::test]
async fn shutdown_during_the_leave_boundary_never_rejoins() {
    let server = FakeServer::start_without_username_ack().await;
    let mut h = Harness::start_on(server, fixed(), SHORT, LONG).await;
    let id = h.server.expect("get_username").await[1].clone();
    h.server.acknowledge_username(&id, Some("bot"));
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect("leave_game").await;
    h.server.expect("get_username").await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn private_room_votes_again_when_a_join_clears_the_force_start() {
    let mut h = Harness::start(fixed()).await;
    h.server.expect_private_join().await;
    h.server
        .send(&json!(["queue_update", {"numPlayers": 2, "isForcing": false}]));
    assert_eq!(h.server.expect("set_force_start").await[0], "room");
    h.server
        .send(&json!(["queue_update", {"numPlayers": 2, "isForcing": true}]));
    h.start_match("game0").await;
    h.shutdown.cancel();
    h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn one_vs_one_never_votes_for_force_start() {
    let mut h = Harness::start(QueueMode::OneVsOne).await;
    h.server.expect("join_1v1").await;
    h.server
        .send(&json!(["queue_update", {"numPlayers": 1, "isForcing": false}]));
    h.start_match("game0").await;
    h.shutdown.cancel();
    h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn removal_from_the_queue_stops_an_otherwise_idle_session() {
    for removal in [json!(["queue_left"]), json!(["removed_from_queue", "team"])] {
        let mut h = Harness::start(fixed()).await;
        h.server.expect_private_join().await;
        h.server.send(&removal);
        let error = h.finish(&[CLIENT_DISCONNECT]).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref(),
            Some(LifecycleError::RemovedFromQueue)
        ));
    }
}

#[tokio::test]
async fn server_error_during_the_username_request_is_reported_as_itself() {
    let server = FakeServer::start_without_username_ack().await;
    let mut h = Harness::start_on(server, fixed(), LONG, LONG).await;
    h.server.expect("get_username").await;
    h.server.send(&json!(["error_user_id"]));
    let error = h.finish(&[CLIENT_DISCONNECT]).await.unwrap_err();
    assert!(error.to_string().contains("already connected"), "{error}");
}

/// Plays one match that returns `Fatal` once `prepare` is done, after the server
/// has followed `GameStart` with `extra_events` unrelated events.
async fn run_one_failing_match(
    server: &mut FakeServer,
    extra_events: usize,
    prepare: impl AsyncFnOnce(&mut SocketReceiver, &GeneralsIoSender, &SurrenderOnce),
) -> miette::Result<()> {
    let session = Session::connect(
        server.url.clone(),
        "user".to_owned(),
        CancellationToken::new(),
    )
    .await
    .unwrap()
    .with_queue_mode(fixed());
    let mut prepare = Some(prepare);
    let run = session.run(async |context: MatchContext<'_>| {
        (prepare.take().expect("only one match runs"))(context.rx, context.tx, context.surrender)
            .await;
        Err(MatchError::Fatal(miette!("stop")))
    });
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
    let run = run_one_failing_match(&mut server, 64, async |rx, _tx, _surrender| {
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
    let run = run_one_failing_match(&mut server, 0, async |_rx, tx, _surrender| {
        kept_sender = Some(tx.clone());
    });
    let result = tokio::time::timeout(LONG, run)
        .await
        .expect("cleanup should not wait for the kept sender");
    assert!(result.is_err());
    assert!(kept_sender.is_some());
    server.expect_only(&[SURRENDER, CLIENT_DISCONNECT]).await;
}

#[tokio::test]
async fn gameplay_surrender_is_not_repeated_by_cleanup_after_a_failure() {
    let mut server = FakeServer::start().await;
    let run = run_one_failing_match(&mut server, 0, async |_rx, tx, surrender| {
        surrender.send(tx).await.unwrap();
        surrender.send(tx).await.unwrap();
    });
    let result = tokio::time::timeout(LONG, run).await.unwrap();
    assert!(result.is_err());
    server.expect_only(&[SURRENDER, CLIENT_DISCONNECT]).await;
}

#[tokio::test]
async fn gameplay_surrender_is_not_repeated_by_cleanup_on_shutdown() {
    let mut server = FakeServer::start().await;
    let shutdown = CancellationToken::new();
    let session = Session::connect(server.url.clone(), "user".to_owned(), shutdown.clone())
        .await
        .unwrap()
        .with_queue_mode(fixed());
    let run = tokio::spawn(session.run(async |context: MatchContext<'_>| {
        context.surrender.send(context.tx).await.unwrap();
        context.shutdown.cancelled().await;
        Ok(())
    }));
    server.expect("get_username").await;
    server.expect_private_join().await;
    server.send(&game_start("game0"));
    server.expect(SURRENDER).await;
    shutdown.cancel();
    tokio::time::timeout(LONG, run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server.expect_only(&[CLIENT_DISCONNECT]).await;
}

#[tokio::test]
async fn match_limit_stops_after_game_over_without_a_vote_or_surrender() {
    let mut h = Harness::start_with_one_match_limit().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn match_limit_counts_every_started_match() {
    let server = FakeServer::start().await;
    let limit = NonZeroU32::new(2).unwrap();
    let mut h = Harness::start_limited(server, Some(limit), fixed(), LONG, LONG).await;
    h.server.expect("get_username").await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.server.send(&game_over());
    h.server.expect_leave().await;
    h.server.expect_private_join().await;
    h.start_match("game1").await;
    h.server.send(&game_over());
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn match_limit_still_exits_an_abandoned_match_without_requeueing() {
    let mut h = Harness::start_with_one_match_limit().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Abandon(miette!("no plan")));
    h.server.expect(SURRENDER).await;
    h.server.send(&game_over());
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn match_limit_reports_an_unconfirmed_exit() {
    let mut h = Harness::start_with_one_match_limit().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Abandon(miette!("no plan")));
    let error = h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref(),
        Some(LifecycleError::SurrenderUnconfirmed)
    ));
}

#[tokio::test]
async fn match_limit_stops_after_elimination_without_surrendering() {
    let mut h = Harness::start_with_one_match_limit().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Eliminated(miette!("no game over")));
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}

#[tokio::test]
async fn match_limit_leaves_a_fatal_match_to_cleanup() {
    let mut h = Harness::start_with_one_match_limit().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Fatal(miette!("invariant broken")));
    h.finish(&[SURRENDER, CLIENT_DISCONNECT]).await.unwrap_err();
}

#[tokio::test]
async fn match_limit_keeps_shutdown_priority_during_the_last_abandoned_exit() {
    let server = FakeServer::start().await;
    let mut h = Harness::start_limited(server, Some(NonZeroU32::MIN), fixed(), LONG, LONG).await;
    h.server.expect("get_username").await;
    h.server.expect_private_join().await;
    h.start_match("game0").await;
    h.fail_match(MatchError::Abandon(miette!("no plan")));
    h.server.expect(SURRENDER).await;
    h.shutdown.cancel();
    h.finish(&[CLIENT_DISCONNECT]).await.unwrap();
}
