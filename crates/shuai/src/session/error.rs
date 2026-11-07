use miette::{Diagnostic, Report};
use thiserror::Error;

/// Why a gameplay function returned without observing `GameOver`.
///
/// The variant states what the server may still believe, because that decides
/// whether Session can requeue. A `?` on a [`Report`] classifies as
/// [`MatchError::Fatal`], so an unclassified failure can never trigger a requeue.
#[derive(Debug)]
pub enum MatchError {
    /// The match may still be active, but leaving it is safe. Session surrenders
    /// and requeues only after the server confirms the exit.
    Abandon(Report),
    /// The server reported `GameLost`, so the local player is already out, but
    /// `GameOver` did not arrive. Session leaves the game and requeues.
    Eliminated(Report),
    /// The state or the connection cannot be trusted. Session stops.
    Fatal(Report),
}

impl From<Report> for MatchError {
    fn from(report: Report) -> Self {
        Self::Fatal(report)
    }
}

/// A failure of Session's own phase transitions.
#[derive(Debug, Error, Diagnostic)]
pub enum LifecycleError {
    /// The account has no username and none was configured.
    #[error("account has no username and none was configured")]
    #[diagnostic(code(shuai::session::username_not_set))]
    UsernameNotSet,
    /// The server took the session out of the queue, so no game will start.
    #[error("server removed the session from the queue")]
    #[diagnostic(code(shuai::session::removed_from_queue))]
    RemovedFromQueue,
    /// A surrender was submitted, but the server never confirmed the exit.
    #[error("server did not confirm the match exit after surrender")]
    #[diagnostic(code(shuai::session::exit_unconfirmed))]
    ExitUnconfirmed,
    /// Disconnecting and joining the client did not finish within the grace period.
    #[error("connection cleanup did not finish within the grace period")]
    #[diagnostic(code(shuai::session::cleanup_timed_out))]
    CleanupTimedOut,
}
