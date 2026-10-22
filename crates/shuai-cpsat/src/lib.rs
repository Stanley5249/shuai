//! Game formulations independent of networking, async runtimes, and game state.

pub mod opening;

/// Convenience exports for opening formulations.
pub mod prelude {
    pub use crate::opening::{
        Hints, Outcome, Path, Problem, Solution, SolveOptions, Timings, solve,
    };
}
