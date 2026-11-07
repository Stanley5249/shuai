//! Runs the synchronous solver off the event path, with cooperative cancellation.

use std::time::Duration;

use miette::miette;
use shuai_cpsat::opening::{self, Outcome, Problem, SolveOptions, StopHandle};
use tokio::task::{JoinError, JoinHandle};

type SolveResult = Result<Result<Outcome, opening::Error>, JoinError>;

/// One match's solver job. Dropping it requests a stop, so no match leaves a solver
/// running at full speed, but only [`OpeningSolve::reap`] observes that it ended.
pub struct OpeningSolve {
    stop: StopHandle,
    worker: JoinHandle<Result<Outcome, opening::Error>>,
}

impl OpeningSolve {
    /// Starts solving on a blocking thread.
    #[must_use]
    pub fn start(problem: Problem, options: SolveOptions) -> Self {
        let stop = StopHandle::new();
        let worker_stop = stop.clone();
        let worker = tokio::task::spawn_blocking(move || {
            opening::solve_with_stop(&problem, &options, &worker_stop)
        });
        Self { stop, worker }
    }

    /// Asks the solver to stop, without waiting for it.
    pub fn request_stop(&self) {
        self.stop.stop();
    }

    /// Waits for the solver's result. Cancel safe, because it only polls the worker.
    pub async fn finished(&mut self) -> SolveResult {
        (&mut self.worker).await
    }

    /// Requests a stop and waits for the worker, up to `grace`.
    ///
    /// # Errors
    /// Returns an error if the worker failed, or did not stop in time. A native solve
    /// cannot be killed, so that worker keeps running until its budget ends, and the
    /// caller must not start another match.
    pub async fn reap(mut self, grace: Duration) -> miette::Result<()> {
        self.stop.stop();
        match tokio::time::timeout(grace, &mut self.worker).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(miette!("solver worker failed: {error}")),
            Err(_) => Err(miette!("solver did not stop within {grace:?}")),
        }
    }
}

impl Drop for OpeningSolve {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
