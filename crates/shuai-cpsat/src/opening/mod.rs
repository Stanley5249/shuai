//! Opening land maximization with explicit scheduling and graph contracts.
//!
//! Each input undirected edge contributes both directions to the circuit.
//! Optimality is relative to the selected scheduling and heuristic constraints.
//! Neighborhood search uses undirected adjacency and returns original vertex IDs.

mod graph;
mod model;
mod reconstruct;

use std::time::{Duration, Instant};

use cpsat::proto::{CpSolverStatus, SatParameters};
use thiserror::Error;

/// A validated-input or solver failure.
#[derive(Debug, Error)]
pub enum Error {
    /// A graph, scheduling parameter, or hint violates the public contract.
    #[error("invalid opening input: {0}")]
    Input(&'static str),
    /// The CP-SAT bridge or model validator failed.
    #[error(transparent)]
    Solver(#[from] cpsat::Error),
    /// The solver rejected its model or parameters.
    #[error("solver rejected opening model: {0}")]
    InvalidModel(String),
    /// A selected solution failed independent reconstruction validation.
    #[error("inconsistent opening solution: {0}")]
    Reconstruction(&'static str),
}

/// An undirected graph and opening schedule, independent of game coordinates.
#[derive(Debug)]
pub struct Problem {
    /// Vertices are identified by `0..vertex_count`.
    pub vertex_count: usize,
    /// Unique undirected edges. Tuple order has no effect on feasible routes.
    pub edges: Vec<(usize, usize)>,
    /// Original vertex identifier of the general.
    pub start: usize,
    /// Maximum undirected BFS distance from the start.
    pub radius: usize,
    /// Final planning turn, strictly positive.
    pub horizon: i64,
    /// Number of paths, including permitted empty paths.
    pub path_count: usize,
    /// Army-generation period in turns, strictly positive.
    pub generation_turns: i64,
    /// Army-generation period in seconds, finite and strictly positive.
    pub generation_seconds: f64,
    /// Lower bound on each cumulative land count, independent of solve budget.
    pub minimum_lands: i64,
}

/// Per-path increments used as optional search hints.
#[derive(Debug, Default)]
pub struct Hints {
    /// New land counts. Each cumulative sum must fit its variable domain.
    pub lands: Option<Vec<i64>>,
    /// End-turn increments from zero. The first includes the generation wait.
    pub turns: Option<Vec<i64>>,
}

/// Search settings. Heuristics change feasibility, while hints guide search.
#[derive(Debug)]
pub struct SolveOptions {
    /// Cooperative budget covering validation, preprocessing, and model build.
    pub budget: Duration,
    /// Positive CP-SAT worker count within `i32`.
    pub workers: usize,
    /// Enforces nonincreasing land and duration increments when true.
    pub heuristics: bool,
    /// Optional validated hints, with one increment per path.
    pub hints: Hints,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            budget: Duration::from_secs(5),
            workers: 1,
            heuristics: true,
            hints: Hints::default(),
        }
    }
}

/// A path starts after army generation and excludes the artificial closing arc.
#[derive(Debug, PartialEq, Eq)]
pub struct Path {
    /// Turn at which the first move may be made.
    pub start_turn: i64,
    /// Available turns, including any waiting after the last move.
    pub duration: i64,
    /// Original vertex IDs beginning at the problem's start.
    pub vertices: Vec<usize>,
}

/// Independently checked paths and their number of distinct non-start tiles.
#[derive(Debug)]
pub struct Solution {
    /// Paths in schedule order.
    pub paths: Vec<Path>,
    /// Union of visited non-start vertices.
    pub objective: i64,
}

/// Phase timings. Reconstruction follows the cooperative solve budget.
#[derive(Debug, Default)]
pub struct Timings {
    /// Validation and neighborhood construction.
    pub preprocessing: Duration,
    /// CP-SAT variable and constraint construction, including model validation.
    pub model: Duration,
    /// Solver call, including shutdown.
    pub search: Duration,
    /// Path reconstruction and independent verification.
    pub reconstruction: Duration,
}

/// Valid solve outcomes preserve the solver status when a search was run.
#[derive(Debug)]
pub enum Outcome {
    /// A proven optimum under the configured scheduling and heuristics.
    Optimal {
        solution: Solution,
        timings: Timings,
    },
    /// An incumbent without an optimality proof.
    Feasible {
        solution: Solution,
        timings: Timings,
    },
    /// A proof that the configured formulation has no feasible solution.
    Infeasible { timings: Timings },
    /// Search ended with no incumbent. The precise stopping reason is unknown.
    NoIncumbent {
        status: CpSolverStatus,
        timings: Timings,
    },
    /// The cooperative budget was exhausted before search.
    BudgetExhausted { timings: Timings },
}

/// Solves an opening synchronously and verifies its returned paths.
///
/// # Errors
/// Returns an error for invalid input, solver/model failures, or an inconsistent
/// incumbent. A zero budget is a valid immediate exhausted outcome after input
/// validation. Solver shutdown and reconstruction can exceed a positive budget.
pub fn solve(problem: &Problem, options: &SolveOptions) -> Result<Outcome, Error> {
    let started = Instant::now();
    graph::validate(problem, options)?;
    let mut timings = Timings::default();
    if options.budget.is_zero() {
        timings.preprocessing = started.elapsed();
        return Ok(Outcome::BudgetExhausted { timings });
    }
    let graph = graph::neighborhood(problem);
    timings.preprocessing = started.elapsed();
    if started.elapsed() >= options.budget {
        return Ok(Outcome::BudgetExhausted { timings });
    }
    let phase = Instant::now();
    let model = model::build(problem, options, &graph)?;
    model.model.validate()?;
    timings.model = phase.elapsed();
    let Some(remaining) = options
        .budget
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
    else {
        return Ok(Outcome::BudgetExhausted { timings });
    };
    let parameters = SatParameters::default()
        .with_max_time_in_seconds(remaining.as_secs_f64())
        .with_num_workers(
            i32::try_from(options.workers).map_err(|_| Error::Input("worker count overflow"))?,
        )
        .with_cp_model_presolve(false);
    let phase = Instant::now();
    let response = model.model.solve(&parameters)?;
    timings.search = phase.elapsed();
    let status = response.status();
    match status {
        CpSolverStatus::OPTIMAL | CpSolverStatus::FEASIBLE => {
            let phase = Instant::now();
            let solution = reconstruct::read(problem, options, &graph, &model, &response)?;
            timings.reconstruction = phase.elapsed();
            Ok(if status == CpSolverStatus::OPTIMAL {
                Outcome::Optimal { solution, timings }
            } else {
                Outcome::Feasible { solution, timings }
            })
        }
        CpSolverStatus::INFEASIBLE => Ok(Outcome::Infeasible { timings }),
        CpSolverStatus::MODEL_INVALID => Err(Error::InvalidModel(response.solution_info)),
        CpSolverStatus::UNKNOWN => Ok(Outcome::NoIncumbent { status, timings }),
    }
}

#[cfg(test)]
mod tests;
