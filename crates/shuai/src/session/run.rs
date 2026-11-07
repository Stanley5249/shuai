use std::future::pending;
use std::num::NonZeroU32;
use std::pin::{Pin, pin};
use std::time::Duration;

use bytestring::ByteString;
use miette::{IntoDiagnostic, Report, Result, WrapErr};
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, instrument};
use url::Url;

use super::context::MatchContext;
use super::error::{LifecycleError, MatchError};
use super::events::{discard_received_events, log_discarded, next_event};
use super::room::{QueueMode, random_room_id};
use super::surrender::SurrenderOnce;

const REMATCH_TIMEOUT: Duration = Duration::from_secs(30);
const SURRENDER_TIMEOUT: Duration = Duration::from_secs(10);
const CLEANUP_GRACE: Duration = Duration::from_secs(5);

/// Cancels `shutdown` on the first Ctrl+C. This is the single shutdown trigger.
pub async fn cancel_on_ctrl_c(shutdown: CancellationToken) {
    match tokio::signal::ctrl_c().await {
        Ok(()) => tracing::info!("received ctrl+c"),
        Err(error) => tracing::error!(%error, "failed to listen for ctrl+c"),
    }
    shutdown.cancel();
}

/// What follows a settled match.
enum Handoff {
    /// A `GameStart` that must reach the next gameplay call.
    Rematch(Box<GameStartData>),
    /// The match has been left, so join the configured queue.
    Queue,
    /// The session has nothing more to play.
    Stop,
}

/// Owns one connection through repeated queue, gameplay, and rematch phases.
pub struct Session {
    client: Client,
    tx: GeneralsIoSender,
    rx: SocketReceiver,
    user_id: String,
    username: Option<String>,
    queue_mode: QueueMode,
    shutdown: CancellationToken,
    rematch_timeout: Duration,
    surrender_timeout: Duration,
    match_limit: Option<NonZeroU32>,
    /// The match that still needs an exit, and so a surrender if the session closes.
    active_match: Option<SurrenderOnce>,
}

impl Session {
    /// Opens the connection. Nothing joins a queue until [`Session::run`].
    ///
    /// # Errors
    /// Returns an error if the client cannot be opened or the namespace request fails.
    pub async fn connect(
        endpoint: Url,
        user_id: String,
        shutdown: CancellationToken,
    ) -> Result<Self> {
        let client = ClientBuilder::new(endpoint)
            .transport(TransportStrategy::WebSocket)
            .open()
            .into_diagnostic()?;
        let (socket_tx, rx) = client
            .connect(ByteString::from_static("/"))
            .await
            .into_diagnostic()?;
        Ok(Self {
            client,
            tx: GeneralsIoSender::new(socket_tx),
            rx,
            user_id,
            username: None,
            queue_mode: QueueMode::RandomPrivate,
            shutdown,
            rematch_timeout: REMATCH_TIMEOUT,
            surrender_timeout: SURRENDER_TIMEOUT,
            match_limit: None,
            active_match: None,
        })
    }

    /// Sets the username used when the account has none.
    #[must_use]
    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }

    /// Selects the matchmaking queue and room policy.
    #[must_use]
    pub fn with_queue_mode(mut self, queue_mode: QueueMode) -> Self {
        self.queue_mode = queue_mode;
        self
    }

    /// Sets how long to wait for a rematch in the 1v1 queue before requeueing.
    #[must_use]
    pub fn with_rematch_timeout(mut self, timeout: Duration) -> Self {
        self.rematch_timeout = timeout;
        self
    }

    /// Sets how long to wait for the server to confirm a surrender-based exit.
    #[must_use]
    pub fn with_surrender_timeout(mut self, timeout: Duration) -> Self {
        self.surrender_timeout = timeout;
        self
    }

    /// Stops after this many started matches, without a rematch vote or a requeue.
    ///
    /// The last match still exits as usual, and the session disconnects, which also
    /// leaves the room. Without a limit, the session runs until shutdown.
    #[must_use]
    pub fn with_match_limit(mut self, limit: NonZeroU32) -> Self {
        self.match_limit = Some(limit);
        self
    }

    /// Runs matches until shutdown, a fatal error, or the match limit, then always releases the
    /// connection. A shutdown that completes cleanup returns `Ok(())`.
    ///
    /// # Errors
    /// Returns the failure that stopped the session, or else a cleanup failure.
    pub async fn run(
        mut self,
        mut play: impl AsyncFnMut(MatchContext<'_>) -> Result<(), MatchError>,
    ) -> Result<()> {
        let span = tracing::info_span!("session", username = tracing::field::Empty);
        async move {
            let lifecycle = self.lifecycle(&mut play).await;
            let cleanup = self.close().await;
            match (lifecycle, cleanup) {
                (Err(error), Err(cleanup)) => {
                    tracing::error!(error = %cleanup, "failed to clean up after session error");
                    Err(error)
                }
                (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
                (Ok(()), Ok(())) => Ok(()),
            }
        }
        .instrument(span)
        .await
    }

    async fn lifecycle(
        &mut self,
        play: &mut impl AsyncFnMut(MatchContext<'_>) -> Result<(), MatchError>,
    ) -> Result<()> {
        if let Some(username) = self.ensure_username().await? {
            tracing::Span::current().record("username", username.as_str());
        }
        let Some(mut start) = self.queue().await? else {
            return Ok(());
        };
        let mut started = 0;
        loop {
            started += 1;
            let last_match = self.match_limit.is_some_and(|limit| started >= limit.get());
            let span = tracing::info_span!(
                "match",
                number = started,
                replay_id = start.replay_id.as_deref()
            );
            let handoff = async {
                let handoff = self.play_match(start, play, last_match).await;
                if let Err(error) = &handoff {
                    // The caller reports this too, but outside this match's span.
                    tracing::error!(?error, "match failed");
                }
                handoff
            }
            .instrument(span)
            .await?;
            start = match handoff {
                Handoff::Rematch(start) => start,
                Handoff::Queue => {
                    let Some(start) = self.queue().await? else {
                        return Ok(());
                    };
                    start
                }
                Handoff::Stop => return Ok(()),
            };
        }
    }

    /// Plays one match and settles everything it leaves behind, so the returned handoff
    /// only needs the session's next phase. On a shutdown or a fatal error `active_match`
    /// stays set, so that [`Session::close`] surrenders.
    async fn play_match(
        &mut self,
        start: Box<GameStartData>,
        play: &mut impl AsyncFnMut(MatchContext<'_>) -> Result<(), MatchError>,
        last_match: bool,
    ) -> Result<Handoff> {
        let surrender = self.active_match.insert(SurrenderOnce::default());
        let result = play(MatchContext {
            start,
            rx: &mut self.rx,
            tx: &self.tx,
            surrender,
            shutdown: &self.shutdown,
        })
        .await;
        if self.shutdown.is_cancelled() {
            return shutdown_result(result).map(|()| Handoff::Stop);
        }
        let game_over = match result {
            Ok(()) => {
                tracing::info!("match ended with game over");
                true
            }
            Err(MatchError::Abandon(error)) => {
                tracing::warn!(?error, "abandoning match");
                self.surrender_and_wait().await?;
                if self.shutdown.is_cancelled() {
                    return Ok(Handoff::Stop);
                }
                false
            }
            Err(MatchError::Eliminated(error)) => {
                tracing::warn!(?error, "leaving match after elimination");
                false
            }
            Err(MatchError::Fatal(error)) => return Err(error),
        };
        self.active_match = None;
        if last_match {
            tracing::info!("match limit reached");
            return Ok(Handoff::Stop);
        }
        if game_over && matches!(self.queue_mode, QueueMode::OneVsOne) {
            // Only the matchmaking queue offers rematches. A custom game's end screen
            // offers Play Again instead, which leaves and rejoins, and a private room
            // ignores votes.
            if let Some(start) = self.rematch().await? {
                return Ok(Handoff::Rematch(start));
            }
        }
        Ok(if self.leave_match().await? {
            Handoff::Queue
        } else {
            Handoff::Stop
        })
    }

    /// Returns the account's username, or `None` on shutdown.
    ///
    /// Keeps reading while the acknowledgement is pending, because a full receiver
    /// would stop sioc from routing the acknowledgement, and a connection failure
    /// should be reported as itself rather than as an acknowledgement timeout.
    #[instrument(skip_all)]
    async fn ensure_username(&mut self) -> Result<Option<String>> {
        let Self {
            tx,
            rx,
            user_id,
            username,
            shutdown,
            ..
        } = self;
        let mut request = pin!(async {
            let current = tx.get_username(user_id.clone()).await;
            match (current.into_diagnostic()?, &*username) {
                (Some(current), _) => {
                    tracing::info!(username = %current, "username already set");
                    Ok(Some(current))
                }
                (None, Some(username)) => {
                    tracing::info!(%username, "setting username");
                    tx.set_username(user_id.clone(), username.clone())
                        .await
                        .into_diagnostic()?;
                    Ok(Some(username.clone()))
                }
                (None, None) => Err(LifecycleError::UsernameNotSet.into()),
            }
        });
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(None),
                result = &mut request => return result,
                event = next_event(rx) => {
                    event?;
                }
            }
        }
    }

    /// Joins the configured queue and waits for `GameStart`. Returns `None` on shutdown.
    #[instrument(skip_all)]
    async fn queue(&mut self) -> Result<Option<Box<GameStartData>>> {
        if self.shutdown.is_cancelled() {
            return Ok(None);
        }
        let joined = self.shutdown.run_until_cancelled(self.join_queue()).await;
        let Some(private_room) = joined.transpose()? else {
            return Ok(None);
        };
        // A vote still being sent lands after the join that prompted it, so a later
        // update needs no second one. Dropping it with this phase cancels it.
        let mut pending_vote = None;
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return Ok(None),
                event = next_event(&mut self.rx) => match event? {
                    GeneralsIoEvent::GameStart(Event { payload, .. }) => {
                        tracing::info!(
                            game_type = payload.data.game_type,
                            usernames = ?payload.data.usernames,
                            "game started"
                        );
                        return Ok(Some(payload.data));
                    }
                    GeneralsIoEvent::QueueUpdate(Event { payload, .. }) => {
                        tracing::debug!(
                            num_players = ?payload.data.num_players,
                            is_forcing = ?payload.data.is_forcing,
                            "queue updated"
                        );
                        // Any player joining clears every force-start vote.
                        if let Some(room) = &private_room
                            && payload.data.is_forcing == Some(false)
                            && pending_vote.is_none()
                        {
                            pending_vote =
                                Some(Box::pin(self.tx.set_force_start(room.clone(), true)));
                        }
                    }
                    GeneralsIoEvent::RemovedFromQueue(_) | GeneralsIoEvent::QueueLeft(_) => {
                        return Err(LifecycleError::RemovedFromQueue.into());
                    }
                    _ => {}
                },
                result = vote_sent(&mut pending_vote) => {
                    pending_vote = None;
                    result.into_diagnostic()?;
                }
            }
        }
    }

    /// Joins the configured queue, and returns the room ID if it is a private room.
    async fn join_queue(&self) -> Result<Option<String>> {
        let room_id = match &self.queue_mode {
            QueueMode::FixedPrivate(room_id) => room_id.clone(),
            QueueMode::RandomPrivate => random_room_id(),
            QueueMode::OneVsOne => {
                tracing::info!("joining 1v1");
                self.tx
                    .join_1v1(self.user_id.clone(), None, true)
                    .await
                    .into_diagnostic()?;
                return Ok(None);
            }
        };
        tracing::info!(url = %format!("https://generals.io/games/{room_id}"), "joining private room");
        self.tx
            .join_private(room_id.clone(), self.user_id.clone())
            .await
            .into_diagnostic()?;
        self.tx
            .set_force_start(room_id.clone(), true)
            .await
            .into_diagnostic()?;
        Ok(Some(room_id))
    }

    /// Leaves the match and discards everything it can still send. Returns `false` on
    /// shutdown.
    ///
    /// Packets from the server stay in order, so everything the match sent before the
    /// server handled `leave_game` arrives before the acknowledgement of a later
    /// request. Nothing from the next queue can arrive either, because it has not been
    /// joined yet. So a late rematch `GameStart` is discarded here rather than played.
    #[instrument(skip_all)]
    async fn leave_match(&mut self) -> Result<bool> {
        let Self {
            tx,
            rx,
            user_id,
            shutdown,
            ..
        } = self;
        let mut barrier = pin!(async {
            tx.leave_game().await.into_diagnostic()?;
            tx.get_username(user_id.clone()).await.into_diagnostic()
        });
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(false),
                result = &mut barrier => {
                    result?;
                    break;
                }
                event = next_event(rx) => log_discarded(Some(event?)),
            }
        }
        discard_received_events(rx)?;
        Ok(!shutdown.is_cancelled())
    }

    /// Votes for a rematch and waits for its outcome. Returns the `GameStart` of the
    /// rematch, or `None` when the session should leave the match, or on shutdown.
    ///
    /// A `disable_rematch` that gameplay consumed before `GameOver` is not replayed
    /// here, and costs only the wait until `rematch_timeout`.
    #[instrument(skip_all)]
    async fn rematch(&mut self) -> Result<Option<Box<GameStartData>>> {
        let voted = self
            .shutdown
            .run_until_cancelled(self.tx.rematch(true))
            .await;
        if voted.transpose().into_diagnostic()?.is_none() {
            return Ok(None);
        }
        let mut timeout = pin!(tokio::time::sleep(self.rematch_timeout));
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return Ok(None),
                event = next_event(&mut self.rx) => match event? {
                    GeneralsIoEvent::GameStart(Event { payload, .. }) => {
                        tracing::info!("rematch started");
                        return Ok(Some(payload.data));
                    }
                    GeneralsIoEvent::DisableRematch(_) => {
                        tracing::info!("rematch disabled");
                        return Ok(None);
                    }
                    GeneralsIoEvent::RematchUpdate(Event { payload, .. }) => {
                        tracing::debug!(num_rematch = payload.num_rematch, "rematch votes updated");
                    }
                    _ => {}
                },
                () = &mut timeout => {
                    tracing::info!("rematch timed out");
                    return Ok(None);
                }
            }
        }
    }

    /// Surrenders and waits until the server confirms the match no longer needs us:
    /// `GameOver` ends the match at once, and `GameLost` allows leaving after the
    /// deadline, because the match can outlive the local player's elimination.
    #[instrument(skip_all)]
    async fn surrender_and_wait(&mut self) -> Result<()> {
        let Some(surrender) = &self.active_match else {
            return Ok(());
        };
        let surrendered = self
            .shutdown
            .run_until_cancelled(surrender.send(&self.tx))
            .await;
        if surrendered.transpose().into_diagnostic()?.is_none() {
            return Ok(());
        }
        let mut timeout = pin!(tokio::time::sleep(self.surrender_timeout));
        let mut eliminated = false;
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return Ok(()),
                event = next_event(&mut self.rx) => match event? {
                    GeneralsIoEvent::GameOver(_) => {
                        tracing::info!("game over confirmed the exit");
                        return Ok(());
                    }
                    GeneralsIoEvent::GameLost(_) => {
                        tracing::info!("local player eliminated");
                        eliminated = true;
                    }
                    _ => {}
                },
                () = &mut timeout => {
                    return if eliminated {
                        tracing::info!("leaving without game over, after elimination");
                        Ok(())
                    } else {
                        Err(LifecycleError::SurrenderUnconfirmed.into())
                    };
                }
            }
        }
    }

    /// Releases the connection. The receiver keeps draining meanwhile, because the
    /// manager cannot process the disconnect while it waits for receiver capacity.
    #[instrument(skip_all)]
    async fn close(self) -> Result<()> {
        let Self {
            client,
            tx,
            mut rx,
            active_match,
            ..
        } = self;
        let close = async move {
            if let Some(surrender) = active_match {
                // A failure here is only logged: the disconnect that follows also
                // ends this player's participation on the server.
                match surrender.send(&tx).await {
                    Ok(()) => tracing::info!("best-effort surrender done"),
                    Err(error) => tracing::warn!(%error, "failed to submit surrender"),
                }
            }
            tx.disconnect().await;
            drop(tx);
            client
                .join()
                .await
                .into_diagnostic()
                .wrap_err("connection task failed")
        };
        let cleanup = async {
            let mut close = pin!(close);
            let mut receiving = true;
            loop {
                tokio::select! {
                    result = &mut close => return result,
                    signal = rx.recv(), if receiving => receiving = signal.is_some(),
                }
            }
        };
        tokio::time::timeout(CLEANUP_GRACE, cleanup)
            .await
            .map_err(|_| Report::new(LifecycleError::CleanupTimedOut))?
    }
}

/// Waits for the force-start vote being sent, or forever if there is none.
async fn vote_sent<F>(pending_vote: &mut Option<Pin<Box<F>>>) -> sioc::error::Result<()>
where
    F: Future<Output = sioc::error::Result<()>>,
{
    match pending_vote {
        Some(vote) => vote.as_mut().await,
        None => pending().await,
    }
}

/// Under shutdown only a fatal gameplay failure remains worth reporting.
fn shutdown_result(result: Result<(), MatchError>) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(MatchError::Abandon(error) | MatchError::Eliminated(error)) => {
            tracing::warn!(?error, "gameplay stopped with an error during shutdown");
            Ok(())
        }
        Err(MatchError::Fatal(error)) => Err(error),
    }
}
