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

use super::error::{LifecycleError, MatchError};
use super::events::{discard_received_events, log_discarded, next_event};
use super::room::{GameMode, random_room_id};
use super::surrender::SurrenderOnce;

const REMATCH_TIMEOUT: Duration = Duration::from_secs(30);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const CLEANUP_GRACE: Duration = Duration::from_secs(5);

/// Cancels `shutdown` on the first Ctrl+C. This is the single shutdown trigger.
pub async fn cancel_on_ctrl_c(shutdown: CancellationToken) {
    match tokio::signal::ctrl_c().await {
        Ok(()) => tracing::info!("received ctrl+c"),
        Err(error) => tracing::error!(%error, "failed to listen for ctrl+c"),
    }
    shutdown.cancel();
}

/// What follows a finished match.
enum Handoff {
    /// A `GameStart` that must reach the next gameplay call.
    Rematch(Box<GameStartData>),
    /// Leave the finished game and join the configured queue.
    Requeue,
    /// Shutdown was requested.
    Shutdown,
}

/// Owns one connection through repeated queue, gameplay, and rematch phases.
pub struct Session {
    client: Client,
    tx: GeneralsIoSender,
    rx: SocketReceiver,
    user_id: String,
    username: Option<String>,
    mode: GameMode,
    shutdown: CancellationToken,
    rematch_timeout: Duration,
    exit_timeout: Duration,
    match_limit: Option<NonZeroU32>,
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
            mode: GameMode::RandomPrivate,
            shutdown,
            rematch_timeout: REMATCH_TIMEOUT,
            exit_timeout: EXIT_TIMEOUT,
            match_limit: None,
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
    pub fn with_mode(mut self, mode: GameMode) -> Self {
        self.mode = mode;
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
    pub fn with_exit_timeout(mut self, timeout: Duration) -> Self {
        self.exit_timeout = timeout;
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
    /// `play` borrows the receiver and sender for one match and must observe the
    /// shutdown token, because this function never drops a running gameplay future. It
    /// must surrender only through its [`SurrenderOnce`].
    ///
    /// # Errors
    /// Returns the failure that stopped the session, or else a cleanup failure.
    pub async fn run(
        mut self,
        mut play: impl AsyncFnMut(
            Box<GameStartData>,
            &mut SocketReceiver,
            &GeneralsIoSender,
            &SurrenderOnce,
            &CancellationToken,
        ) -> Result<(), MatchError>,
    ) -> Result<()> {
        let span = tracing::info_span!("session", username = tracing::field::Empty);
        async move {
            let mut active_match = None;
            let lifecycle = self.lifecycle(&mut play, &mut active_match).await;
            let cleanup = self.close(active_match).await;
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
        play: &mut impl AsyncFnMut(
            Box<GameStartData>,
            &mut SocketReceiver,
            &GeneralsIoSender,
            &SurrenderOnce,
            &CancellationToken,
        ) -> Result<(), MatchError>,
        active_match: &mut Option<SurrenderOnce>,
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
            let surrender = active_match.insert(SurrenderOnce::default());
            let span = tracing::info_span!(
                "match",
                number = started,
                replay_id = start.replay_id.as_deref()
            );
            let result = play(start, &mut self.rx, &self.tx, surrender, &self.shutdown)
                .instrument(span)
                .await;
            if self.shutdown.is_cancelled() {
                return shutdown_result(result);
            }
            let handoff = match result {
                Ok(()) => {
                    *active_match = None;
                    if last_match {
                        return Ok(());
                    }
                    self.next_after_game_over().await?
                }
                Err(MatchError::Abandon(error)) => {
                    tracing::warn!(%error, "abandoning match");
                    self.exit_match(surrender).await?;
                    if self.shutdown.is_cancelled() {
                        return Ok(());
                    }
                    *active_match = None;
                    if last_match {
                        return Ok(());
                    }
                    Handoff::Requeue
                }
                Err(MatchError::Eliminated(error)) => {
                    tracing::warn!(%error, "leaving match after elimination");
                    *active_match = None;
                    if last_match {
                        return Ok(());
                    }
                    Handoff::Requeue
                }
                Err(MatchError::Fatal(error)) => return Err(error),
            };
            start = match handoff {
                Handoff::Rematch(start) => start,
                Handoff::Requeue => {
                    let Some(start) = self.requeue().await? else {
                        return Ok(());
                    };
                    start
                }
                Handoff::Shutdown => return Ok(()),
            };
        }
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
                        tracing::info!("game started");
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
        let room_id = match &self.mode {
            GameMode::FixedPrivate(room_id) => room_id.clone(),
            GameMode::RandomPrivate => random_room_id(),
            GameMode::OneVsOne => {
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

    /// Leaves the finished game, then waits in the configured queue.
    async fn requeue(&mut self) -> Result<Option<Box<GameStartData>>> {
        if self.leave_match().await? {
            self.queue().await
        } else {
            Ok(None)
        }
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

    /// Only the matchmaking queue offers rematches. A custom game's end screen offers
    /// Play Again instead, which leaves and rejoins, and a private room ignores votes.
    async fn next_after_game_over(&mut self) -> Result<Handoff> {
        match self.mode {
            GameMode::OneVsOne => self.rematch().await,
            GameMode::FixedPrivate(_) | GameMode::RandomPrivate => Ok(Handoff::Requeue),
        }
    }

    /// Votes for a rematch and waits for its outcome.
    ///
    /// A `disable_rematch` that gameplay consumed before `GameOver` is not replayed
    /// here, and costs only the wait until `rematch_timeout`.
    #[instrument(skip_all)]
    async fn rematch(&mut self) -> Result<Handoff> {
        let voted = self
            .shutdown
            .run_until_cancelled(self.tx.rematch(true))
            .await;
        if voted.transpose().into_diagnostic()?.is_none() {
            return Ok(Handoff::Shutdown);
        }
        let mut timeout = pin!(tokio::time::sleep(self.rematch_timeout));
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return Ok(Handoff::Shutdown),
                event = next_event(&mut self.rx) => match event? {
                    GeneralsIoEvent::GameStart(Event { payload, .. }) => {
                        tracing::info!("rematch started");
                        return Ok(Handoff::Rematch(payload.data));
                    }
                    GeneralsIoEvent::DisableRematch(_) => {
                        tracing::info!("rematch disabled");
                        return Ok(Handoff::Requeue);
                    }
                    GeneralsIoEvent::RematchUpdate(Event { payload, .. }) => {
                        tracing::debug!(num_rematch = payload.num_rematch, "rematch votes updated");
                    }
                    _ => {}
                },
                () = &mut timeout => {
                    tracing::info!("rematch timed out");
                    return Ok(Handoff::Requeue);
                }
            }
        }
    }

    /// Surrenders and waits until the server confirms the match no longer needs us:
    /// `GameOver` ends the match at once, and `GameLost` allows leaving after the
    /// deadline, because the match can outlive the local player's elimination.
    #[instrument(skip_all)]
    async fn exit_match(&mut self, surrender: &SurrenderOnce) -> Result<()> {
        let surrendered = self
            .shutdown
            .run_until_cancelled(surrender.send(&self.tx))
            .await;
        if surrendered.transpose().into_diagnostic()?.is_none() {
            return Ok(());
        }
        let mut timeout = pin!(tokio::time::sleep(self.exit_timeout));
        let mut eliminated = false;
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return Ok(()),
                event = next_event(&mut self.rx) => match event? {
                    GeneralsIoEvent::GameOver(_) => return Ok(()),
                    GeneralsIoEvent::GameLost(_) => eliminated = true,
                    _ => {}
                },
                () = &mut timeout => {
                    return if eliminated {
                        Ok(())
                    } else {
                        Err(LifecycleError::ExitUnconfirmed.into())
                    };
                }
            }
        }
    }

    /// Releases the connection. The receiver keeps draining meanwhile, because the
    /// manager cannot process the disconnect while it waits for receiver capacity.
    #[instrument(skip_all)]
    async fn close(self, active_match: Option<SurrenderOnce>) -> Result<()> {
        let Self {
            client, tx, mut rx, ..
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
            tracing::warn!(%error, "gameplay stopped with an error during shutdown");
            Ok(())
        }
        Err(MatchError::Fatal(error)) => Err(error),
    }
}
