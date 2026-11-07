use std::pin::pin;
use std::time::Duration;

use bytestring::ByteString;
use miette::{IntoDiagnostic, Report, Result, WrapErr};
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use url::Url;

use super::error::{LifecycleError, MatchError};
use super::events::next_event;
use super::room::{GameMode, random_room_id};

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

/// How Session arrives at the queue, which decides whose `GameStart` it can trust.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueEntry {
    /// No match came before, so any `GameStart` belongs to this queue.
    FirstJoin,
    /// A rematch may still start for the match just left. A `GameStart` before the
    /// first `queue_update` could be that match, so it cannot be attributed.
    Requeue,
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

    /// Sets how long to wait for a rematch before requeueing.
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

    /// Runs matches until shutdown or a fatal error, then always releases the
    /// connection. A shutdown that completes cleanup returns `Ok(())`.
    ///
    /// `play` borrows the receiver and sender for one match and must observe the
    /// shutdown token, because this function never drops a running gameplay future.
    ///
    /// # Errors
    /// Returns the failure that stopped the session, or else a cleanup failure.
    pub async fn run(
        mut self,
        mut play: impl AsyncFnMut(
            Box<GameStartData>,
            &mut SocketReceiver,
            &GeneralsIoSender,
            &CancellationToken,
        ) -> Result<(), MatchError>,
    ) -> Result<()> {
        let mut match_active = false;
        let lifecycle = self.lifecycle(&mut play, &mut match_active).await;
        let cleanup = self.close(match_active).await;
        match (lifecycle, cleanup) {
            (Err(error), Err(cleanup)) => {
                tracing::error!(error = %cleanup, "failed to clean up after session error");
                Err(error)
            }
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn lifecycle(
        &mut self,
        play: &mut impl AsyncFnMut(
            Box<GameStartData>,
            &mut SocketReceiver,
            &GeneralsIoSender,
            &CancellationToken,
        ) -> Result<(), MatchError>,
        match_active: &mut bool,
    ) -> Result<()> {
        self.ensure_username().await?;
        let Some(mut start) = self.queue(QueueEntry::FirstJoin).await? else {
            return Ok(());
        };
        loop {
            *match_active = true;
            let result = play(start, &mut self.rx, &self.tx, &self.shutdown).await;
            if self.shutdown.is_cancelled() {
                return shutdown_result(result);
            }
            let handoff = match result {
                Ok(()) => {
                    *match_active = false;
                    self.rematch().await?
                }
                Err(MatchError::Abandon(error)) => {
                    tracing::warn!(%error, "abandoning match");
                    self.exit_match().await?;
                    if self.shutdown.is_cancelled() {
                        return Ok(());
                    }
                    *match_active = false;
                    Handoff::Requeue
                }
                Err(MatchError::Eliminated(error)) => {
                    tracing::warn!(%error, "leaving match after elimination");
                    *match_active = false;
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

    /// Keeps reading while the acknowledgement is pending, because a full receiver
    /// would stop sioc from routing the acknowledgement, and a connection failure
    /// should be reported as itself rather than as an acknowledgement timeout.
    #[instrument(skip_all, err)]
    async fn ensure_username(&mut self) -> Result<()> {
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
                    Ok(())
                }
                (None, Some(username)) => {
                    tracing::info!(%username, "setting username");
                    tx.set_username(user_id.clone(), username.clone())
                        .await
                        .into_diagnostic()
                }
                (None, None) => Err(LifecycleError::UsernameNotSet.into()),
            }
        });
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(()),
                result = &mut request => return result,
                event = next_event(rx) => {
                    event?;
                }
            }
        }
    }

    /// Joins the configured queue and waits for `GameStart`. Returns `None` on shutdown.
    #[instrument(skip_all, err)]
    async fn queue(&mut self, entry: QueueEntry) -> Result<Option<Box<GameStartData>>> {
        if self.shutdown.is_cancelled() {
            return Ok(None);
        }
        let joined = self.shutdown.run_until_cancelled(self.join_queue()).await;
        if joined.transpose()?.is_none() {
            return Ok(None);
        }
        let mut entered_queue = entry == QueueEntry::FirstJoin;
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => return Ok(None),
                event = next_event(&mut self.rx) => match event? {
                    GeneralsIoEvent::GameStart(Event { payload, .. }) => {
                        if !entered_queue {
                            return Err(LifecycleError::AmbiguousGameStart.into());
                        }
                        tracing::info!("game started");
                        return Ok(Some(payload.data));
                    }
                    GeneralsIoEvent::QueueUpdate(Event { payload, .. }) => {
                        entered_queue = true;
                        tracing::debug!(num_players = ?payload.data.num_players, "queue updated");
                    }
                    GeneralsIoEvent::RemovedFromQueue(_) | GeneralsIoEvent::QueueLeft(_) => {
                        return Err(LifecycleError::RemovedFromQueue.into());
                    }
                    _ => {}
                },
            }
        }
    }

    async fn join_queue(&self) -> Result<()> {
        match &self.mode {
            GameMode::FixedPrivate(room_id) => self.join_private(room_id.clone()).await,
            GameMode::RandomPrivate => self.join_private(random_room_id()).await,
            GameMode::OneVsOne => {
                tracing::info!("joining 1v1");
                self.tx
                    .join_1v1(self.user_id.clone(), None, true)
                    .await
                    .into_diagnostic()
            }
        }
    }

    async fn join_private(&self, room_id: String) -> Result<()> {
        tracing::info!(url = %format!("https://generals.io/games/{room_id}"), "joining private room");
        self.tx
            .join_private(room_id.clone(), self.user_id.clone())
            .await
            .into_diagnostic()?;
        self.tx
            .set_force_start(room_id, true)
            .await
            .into_diagnostic()
    }

    /// Leaves the finished game, then waits in the configured queue.
    async fn requeue(&mut self) -> Result<Option<Box<GameStartData>>> {
        if self.shutdown.is_cancelled() {
            return Ok(None);
        }
        let left = self
            .shutdown
            .run_until_cancelled(self.tx.leave_game())
            .await;
        if left.transpose().into_diagnostic()?.is_none() {
            return Ok(None);
        }
        self.queue(QueueEntry::Requeue).await
    }

    /// Votes for a rematch and waits for its outcome.
    ///
    /// A `disable_rematch` that gameplay consumed before `GameOver` is not replayed
    /// here, and costs only the wait until `rematch_timeout`.
    #[instrument(skip_all, err)]
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

    /// Surrenders and waits until the server confirms the match no longer needs us.
    #[instrument(skip_all, err)]
    async fn exit_match(&mut self) -> Result<()> {
        let surrendered = self.shutdown.run_until_cancelled(self.tx.surrender()).await;
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
    #[instrument(skip_all, err)]
    async fn close(self, match_active: bool) -> Result<()> {
        let Self {
            client, tx, mut rx, ..
        } = self;
        let close = async move {
            if match_active {
                // A failure here is only logged: the disconnect that follows also
                // ends this player's participation on the server.
                match tx.surrender().await {
                    Ok(()) => tracing::info!("submitted best-effort surrender"),
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
