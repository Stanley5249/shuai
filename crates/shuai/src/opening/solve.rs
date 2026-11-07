//! Runs the synchronous solver off the event path, with cooperative cancellation.

use std::time::Duration;

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

    /// Requests a stop and waits for the worker, up to `grace`. A native solve cannot
    /// be killed, so a worker that ignores the stop is reported and left to its budget.
    pub async fn reap(mut self, grace: Duration) {
        self.stop.stop();
        match tokio::time::timeout(grace, &mut self.worker).await {
            Ok(Ok(_)) => tracing::debug!("solver stopped"),
            Ok(Err(error)) => tracing::error!(%error, "solver worker failed"),
            Err(_) => tracing::error!(?grace, "solver did not stop within the grace period"),
        }
    }
}

impl Drop for OpeningSolve {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
