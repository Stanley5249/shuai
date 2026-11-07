//! Runs bots together in private rooms on a live server.
//!
//! A bot is a gameplay function, as [`Session::run`] takes it, so different bots in a
//! room are different arguments to [`LiveRoom::run_bot`]. Rooms and their bots run
//! concurrently in one task, through `tokio::join!`. That is enough, because bots wait
//! on I/O and CP-SAT runs on blocking threads, and it needs no `Send` futures.

use std::num::NonZeroU32;
use std::pin::pin;
use std::time::Duration;

use miette::{Result, miette};
use shuai::prelude::*;
use shuai_client::prelude::*;
use tokio_util::sync::CancellationToken;
use url::Url;

const REPLAY_PAGE: &str = "https://generals.io/replays/";

/// One account on one server.
pub struct Account {
    pub endpoint: Url,
    pub user_id: String,
}

/// How one requested match ended for one bot.
#[derive(Debug, PartialEq, Eq)]
pub enum MatchOutcome {
    /// Gameplay returned `Ok(())` without a shutdown request, so it observed `GameOver`.
    Completed,
    /// Gameplay stopped because of a shutdown request, so the match may not have ended.
    Interrupted,
    /// Gameplay returned this `MatchError`, as text because Session needs the error too.
    Failed(String),
}

/// One match as one bot saw it.
#[derive(Debug)]
pub struct PlayedMatch {
    pub replay_id: Option<String>,
    pub outcome: MatchOutcome,
}

/// Everything one bot did in a room.
#[derive(Debug)]
pub struct BotRun {
    pub matches: Vec<PlayedMatch>,
    pub session: Result<()>,
}

/// One private room that bots share, and the means to stop them together. With several
/// matches, the bots rejoin this same room after each one. For a new room per match, use
/// a new `LiveRoom` per match.
pub struct LiveRoom {
    room_id: String,
    matches: NonZeroU32,
    shutdown: CancellationToken,
}

impl LiveRoom {
    /// Creates a randomly named room. Cancelling `parent` stops it.
    pub fn new(matches: NonZeroU32, parent: &CancellationToken) -> Self {
        Self {
            room_id: format!("shuai{:08x}", fastrand::u32(..)),
            matches,
            shutdown: parent.child_token(),
        }
    }

    /// Runs one bot until it played the room's matches, and records each one.
    ///
    /// Any failure cancels this room, so that no other bot waits in it forever. A bot
    /// finishing normally does not, because its peer may still be reading the same
    /// match's end.
    pub async fn run_bot(
        &self,
        account: &Account,
        mut play: impl AsyncFnMut(MatchContext<'_>) -> Result<(), MatchError>,
    ) -> BotRun {
        let mut matches = Vec::new();
        let session = match Session::connect(
            account.endpoint.clone(),
            account.user_id.clone(),
            self.shutdown.clone(),
        )
        .await
        {
            Ok(session) => {
                session
                    .with_queue_mode(QueueMode::FixedPrivate(self.room_id.clone()))
                    .with_match_limit(self.matches)
                    .run(async |context: MatchContext<'_>| {
                        let replay_id = context.start.replay_id.clone();
                        let shutdown = context.shutdown;
                        let result = play(context).await;
                        let outcome = match &result {
                            Err(error) => {
                                tracing::error!(?error, "bot's match failed");
                                self.shutdown.cancel();
                                MatchOutcome::Failed(format!("{error:?}"))
                            }
                            Ok(()) if shutdown.is_cancelled() => MatchOutcome::Interrupted,
                            Ok(()) => MatchOutcome::Completed,
                        };
                        matches.push(PlayedMatch { replay_id, outcome });
                        result
                    })
                    .await
            }
            Err(error) => Err(error),
        };
        if let Err(error) = &session {
            tracing::error!(?error, "bot's session failed");
            self.shutdown.cancel();
        }
        BotRun { matches, session }
    }

    /// Awaits the room's bots. At the time limit it cancels the room and still awaits
    /// them, so gameplay and solver cleanup finish. It spawns nothing.
    pub async fn with_time_limit<F: Future>(&self, limit: Duration, bots: F) -> F::Output {
        let mut bots = pin!(bots);
        tokio::select! {
            output = &mut bots => output,
            () = tokio::time::sleep(limit) => {
                self.shutdown.cancel();
                bots.await
            }
        }
    }
}

/// Plays until `GameOver`, or until shutdown, and never moves.
///
/// # Errors
/// Returns a fatal error when the connection or match state cannot be trusted.
pub async fn play_idle(context: MatchContext<'_>) -> Result<(), MatchError> {
    loop {
        tokio::select! {
            biased;
            () = context.shutdown.cancelled() => return Ok(()),
            event = next_event(context.rx) => {
                if let GeneralsIoEvent::GameOver(_) = event? {
                    return Ok(());
                }
            }
        }
    }
}

/// Builds a replay's page URL.
pub fn replay_url(replay_id: &str) -> String {
    format!("{REPLAY_PAGE}{replay_id}")
}

/// Requires every bot to report `expected` completed matches, with the same replay IDs
/// in the same order, and returns those IDs. Agreement is also the proof that the bots
/// really met, because a wrong room would give different IDs, or no game.
///
/// # Errors
/// Returns the first reason the run does not count as complete.
pub fn verified_replays(runs: Vec<BotRun>, expected: u32) -> Result<Vec<String>> {
    let expected = usize::try_from(expected).expect("a match count fits usize");
    let mut agreed: Option<Vec<String>> = None;
    for (bot, run) in runs.into_iter().enumerate() {
        run.session
            .map_err(|error| error.wrap_err(format!("bot {bot}'s session failed")))?;
        if run.matches.len() != expected {
            return Err(miette!(
                "bot {bot} played {} of {expected} matches",
                run.matches.len()
            ));
        }
        let mut replays = Vec::with_capacity(expected);
        for (number, played) in run.matches.into_iter().enumerate() {
            if played.outcome != MatchOutcome::Completed {
                return Err(miette!(
                    "bot {bot}'s match {number} did not complete: {:?}",
                    played.outcome
                ));
            }
            let replay_id = played
                .replay_id
                .ok_or_else(|| miette!("bot {bot}'s match {number} has no replay ID"))?;
            replays.push(replay_id);
        }
        match &agreed {
            Some(agreed) if *agreed != replays => {
                return Err(miette!(
                    "bots disagree on replays: {agreed:?} and {replays:?}"
                ));
            }
            _ => agreed = Some(replays),
        }
    }
    agreed.ok_or_else(|| miette!("no bots ran"))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use tokio::sync::Notify;

    use super::*;
    use crate::test_support::{CLIENT_DISCONNECT, FakeServer, game_start_data};

    const SURRENDER: &str = "surrender";
    const SHORT: Duration = Duration::from_millis(100);

    fn one() -> NonZeroU32 {
        NonZeroU32::MIN
    }

    fn game_start(replay_id: &str) -> Value {
        let mut data = game_start_data();
        data["replay_id"] = json!(replay_id);
        json!(["game_start", data])
    }

    fn account(server: &FakeServer, user_id: &str) -> Account {
        Account {
            endpoint: server.url.clone(),
            user_id: user_id.to_owned(),
        }
    }

    /// Consumes a bot's join, and returns the room and user it joined with.
    async fn expect_join(server: &mut FakeServer) -> (Value, Value) {
        server.expect("get_username").await;
        let join = server.expect("join_private").await;
        assert_eq!(server.expect("set_force_start").await[0], join[0]);
        (join[0].clone(), join[1].clone())
    }

    /// Plays like `play_idle`, after signalling that the bot received its game.
    async fn signal_then_idle(
        started: &Notify,
        context: MatchContext<'_>,
    ) -> Result<(), MatchError> {
        started.notify_one();
        play_idle(context).await
    }

    fn completed(replay_id: &str) -> BotRun {
        BotRun {
            matches: vec![PlayedMatch {
                replay_id: Some(replay_id.to_owned()),
                outcome: MatchOutcome::Completed,
            }],
            session: Ok(()),
        }
    }

    #[tokio::test]
    async fn two_bots_join_one_room_and_report_one_replay() {
        let mut first = FakeServer::start().await;
        let mut second = FakeServer::start().await;
        let accounts = (account(&first, "first"), account(&second, "second"));
        let room = LiveRoom::new(one(), &CancellationToken::new());
        let script = async {
            let first_join = expect_join(&mut first).await;
            let second_join = expect_join(&mut second).await;
            assert_eq!(first_join.0, second_join.0, "bots must share a room");
            assert_ne!(
                first_join.1, second_join.1,
                "bots must use their own accounts"
            );
            for server in [&first, &second] {
                server.send(&game_start("replay"));
                server.send(&json!(["game_over"]));
            }
        };
        let (first_run, second_run, ()) = tokio::join!(
            room.run_bot(&accounts.0, play_idle),
            room.run_bot(&accounts.1, play_idle),
            script
        );
        let replays = verified_replays(vec![first_run, second_run], 1).unwrap();
        assert_eq!(replays, ["replay"]);
        first.expect_only(&[CLIENT_DISCONNECT]).await;
        second.expect_only(&[CLIENT_DISCONNECT]).await;
    }

    #[tokio::test]
    async fn a_bot_finishing_first_does_not_cancel_its_peer() {
        let mut first = FakeServer::start().await;
        let mut second = FakeServer::start().await;
        let accounts = (account(&first, "first"), account(&second, "second"));
        let room = LiveRoom::new(one(), &CancellationToken::new());
        let script = async {
            expect_join(&mut first).await;
            expect_join(&mut second).await;
            first.send(&game_start("replay"));
            second.send(&game_start("replay"));
            first.send(&json!(["game_over"]));
            first.expect(CLIENT_DISCONNECT).await;
            second.send(&json!(["disable_rematch"]));
            second.send(&json!(["game_won"]));
            second.send(&json!(["game_over"]));
        };
        let (first_run, second_run, ()) = tokio::join!(
            room.run_bot(&accounts.0, play_idle),
            room.run_bot(&accounts.1, play_idle),
            script
        );
        verified_replays(vec![first_run, second_run], 1).unwrap();
    }

    #[tokio::test]
    async fn one_bots_failure_stops_the_room() {
        let mut first = FakeServer::start().await;
        let mut second = FakeServer::start().await;
        let accounts = (account(&first, "first"), account(&second, "second"));
        let room = LiveRoom::new(one(), &CancellationToken::new());
        let peer_started = Notify::new();
        let script = async {
            expect_join(&mut first).await;
            expect_join(&mut second).await;
            second.send(&game_start("replay"));
            peer_started.notified().await;
            first.send(&game_start("replay"));
        };
        let (first_run, second_run, ()) = tokio::join!(
            room.run_bot(&accounts.0, async |_context: MatchContext<'_>| Err(
                MatchError::Abandon(miette!("no plan"))
            ),),
            room.run_bot(&accounts.1, async |context: MatchContext<'_>| {
                signal_then_idle(&peer_started, context).await
            },),
            script
        );
        assert!(matches!(
            first_run.matches[0].outcome,
            MatchOutcome::Failed(_)
        ));
        assert_eq!(second_run.matches[0].outcome, MatchOutcome::Interrupted);
        verified_replays(vec![first_run, second_run], 1).unwrap_err();
    }

    #[tokio::test]
    async fn a_session_that_cannot_start_stops_the_room() {
        let mut first = FakeServer::start_without_username_ack().await;
        let mut second = FakeServer::start().await;
        let accounts = (account(&first, "first"), account(&second, "second"));
        let room = LiveRoom::new(one(), &CancellationToken::new());
        let script = async {
            // The peer must be running before the first bot's failure can stop the room.
            let request = first.expect_username_request().await;
            second.expect("get_username").await;
            first.acknowledge_username(&request, None);
        };
        let (first_run, second_run, ()) = tokio::join!(
            room.run_bot(&accounts.0, play_idle),
            room.run_bot(&accounts.1, play_idle),
            script
        );
        assert!(first_run.session.is_err());
        assert!(second_run.matches.is_empty());
        verified_replays(vec![first_run, second_run], 1).unwrap_err();
    }

    #[tokio::test]
    async fn a_time_limit_during_the_final_match_is_not_a_completion() {
        let mut server = FakeServer::start().await;
        let bot = account(&server, "bot");
        let room = LiveRoom::new(one(), &CancellationToken::new());
        let started = Notify::new();
        let script = async {
            expect_join(&mut server).await;
            server.send(&game_start("replay"));
            // The limit must expire during the match, however slow the setup was.
            started.notified().await;
            room.with_time_limit(SHORT, room.shutdown.cancelled()).await;
        };
        let (run, ()) = tokio::join!(
            room.run_bot(&bot, async |context: MatchContext<'_>| signal_then_idle(
                &started, context
            )
            .await,),
            script
        );
        assert_eq!(run.matches[0].outcome, MatchOutcome::Interrupted);
        verified_replays(vec![run], 1).unwrap_err();
        server.expect_only(&[SURRENDER, CLIENT_DISCONNECT]).await;
    }

    #[tokio::test]
    async fn a_time_limit_with_no_game_reports_no_matches() {
        let server = FakeServer::start().await;
        let bot = account(&server, "bot");
        let room = LiveRoom::new(one(), &CancellationToken::new());
        let run = room
            .with_time_limit(SHORT, room.run_bot(&bot, play_idle))
            .await;
        assert!(run.matches.is_empty());
        verified_replays(vec![run], 1).unwrap_err();
    }

    #[tokio::test]
    async fn cancelling_one_room_leaves_another_running() {
        let parent = CancellationToken::new();
        let mut stopped = FakeServer::start().await;
        let mut running = FakeServer::start().await;
        let accounts = (account(&stopped, "stopped"), account(&running, "running"));
        let stopped_room = LiveRoom::new(one(), &parent);
        let running_room = LiveRoom::new(one(), &parent);
        let stopped_started = Notify::new();
        let script = async {
            let stopped_join = expect_join(&mut stopped).await;
            let running_join = expect_join(&mut running).await;
            assert_ne!(stopped_join.0, running_join.0, "rooms must be distinct");
            stopped.send(&game_start("stopped"));
            running.send(&game_start("running"));
            stopped_started.notified().await;
            stopped_room.shutdown.cancel();
            stopped.expect(SURRENDER).await;
            running.send(&json!(["game_over"]));
        };
        let (stopped_run, running_run, ()) = tokio::join!(
            stopped_room.run_bot(&accounts.0, async |context: MatchContext<'_>| {
                signal_then_idle(&stopped_started, context).await
            },),
            running_room.run_bot(&accounts.1, play_idle),
            script
        );
        verified_replays(vec![stopped_run], 1).unwrap_err();
        assert_eq!(verified_replays(vec![running_run], 1).unwrap(), ["running"]);
    }

    #[tokio::test]
    async fn cancelling_the_parent_stops_every_room() {
        let parent = CancellationToken::new();
        let mut first = FakeServer::start().await;
        let mut second = FakeServer::start().await;
        let accounts = (account(&first, "first"), account(&second, "second"));
        let first_room = LiveRoom::new(one(), &parent);
        let second_room = LiveRoom::new(one(), &parent);
        let script = async {
            expect_join(&mut first).await;
            expect_join(&mut second).await;
            parent.cancel();
        };
        let (first_run, second_run, ()) = tokio::join!(
            first_room.run_bot(&accounts.0, play_idle),
            second_room.run_bot(&accounts.1, play_idle),
            script
        );
        assert!(first_run.matches.is_empty());
        assert!(second_run.matches.is_empty());
    }

    #[test]
    fn verified_replays_accepts_agreeing_completed_runs() {
        let replays = verified_replays(vec![completed("a"), completed("a")], 1).unwrap();
        assert_eq!(replays, ["a"]);
    }

    #[test]
    fn verified_replays_rejects_each_kind_of_incomplete_run() {
        verified_replays(vec![], 1).unwrap_err();
        verified_replays(vec![completed("a")], 2).unwrap_err();
        verified_replays(vec![completed("a"), completed("b")], 1).unwrap_err();

        let mut no_replay = completed("a");
        no_replay.matches[0].replay_id = None;
        verified_replays(vec![no_replay], 1).unwrap_err();

        let mut failed = completed("a");
        failed.matches[0].outcome = MatchOutcome::Failed("no plan".to_owned());
        verified_replays(vec![failed], 1).unwrap_err();

        let mut interrupted = completed("a");
        interrupted.matches[0].outcome = MatchOutcome::Interrupted;
        verified_replays(vec![interrupted], 1).unwrap_err();

        let mut session_failed = completed("a");
        session_failed.session = Err(miette!("connection closed"));
        verified_replays(vec![session_failed], 1).unwrap_err();
    }

    #[test]
    fn replay_url_points_at_the_replay_page() {
        assert_eq!(replay_url("abc"), "https://generals.io/replays/abc");
    }
}
