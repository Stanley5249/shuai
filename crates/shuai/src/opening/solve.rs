//! Runs the synchronous solver off the event path, with cooperative cancellation.

use std::time::Duration;

use miette::{IntoDiagnostic, WrapErr, miette};
use shuai_cpsat::opening::{self, Outcome, Problem, SolveOptions, StopHandle};
use tokio::task::{JoinError, JoinHandle};

type SolveResult = Result<Result<Outcome, opening::Error>, JoinError>;

/// One match's solver job. Dropping it requests a stop, so no match leaves a solver
/// running at full speed, but only [`OpeningSolve::reap`] observes that it ended.
pub(super) struct OpeningSolve {
    stop: StopHandle,
    worker: JoinHandle<Result<Outcome, opening::Error>>,
    /// Whether [`OpeningSolve::finished`] returned the worker's result.
    collected: bool,
}

impl OpeningSolve {
    /// Starts solving on a blocking thread.
    #[must_use]
    pub(super) fn start(problem: Problem, options: SolveOptions) -> Self {
        let stop = StopHandle::new();
        let worker_stop = stop.clone();
        let worker = tokio::task::spawn_blocking(move || {
            opening::solve_with_stop(&problem, &options, &worker_stop)
        });
        Self {
            stop,
            worker,
            collected: false,
        }
    }

    /// Wraps a worker that a test controls, so that no real solver runs.
    #[cfg(test)]
    pub(super) fn from_worker(worker: JoinHandle<Result<Outcome, opening::Error>>) -> Self {
        Self {
            stop: StopHandle::new(),
            worker,
            collected: false,
        }
    }

    /// Asks the solver to stop, without waiting for it.
    pub(super) fn request_stop(&self) {
        self.stop.stop();
    }

    /// Waits for the solver's result. Cancel safe, because it only polls the worker. Call
    /// it no more after it returns, because the worker has no second result.
    pub(super) async fn finished(&mut self) -> SolveResult {
        let result = (&mut self.worker).await;
        self.collected = true;
        result
    }

    /// Requests a stop and waits for the worker, up to `grace`.
    ///
    /// # Errors
    /// Returns an error if the worker failed, or did not stop in time. A native solve
    /// cannot be killed, so that worker keeps running until its budget ends, and the
    /// caller must not start another match.
    pub(super) async fn reap(mut self, grace: Duration) -> miette::Result<()> {
        if self.collected {
            return Ok(());
        }
        self.stop.stop();
        match tokio::time::timeout(grace, &mut self.worker).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(error)
                .into_diagnostic()
                .wrap_err("solver worker failed"),
            Err(_) => Err(miette!("solver did not stop within {grace:?}")),
        }
    }
}

impl Drop for OpeningSolve {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
