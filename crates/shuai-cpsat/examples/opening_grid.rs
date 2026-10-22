//! Repeats an offline grid fixture to compare model revisions.

use std::time::Duration;

use shuai_cpsat::opening::{self, Outcome, Problem, SolveOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width = 8;
    let mut edges = Vec::new();
    for row in 0..width {
        for column in 0..width {
            let vertex = row * width + column;
            if column + 1 < width {
                edges.push((vertex, vertex + 1));
            }
            if row + 1 < width {
                edges.push((vertex, vertex + width));
            }
        }
    }
    let problem = Problem {
        vertex_count: width * width,
        edges,
        start: 0,
        radius: 16,
        horizon: 50,
        path_count: 5,
        generation_turns: 2,
        generation_seconds: 1.0,
        minimum_lands: 8,
    };
    let seconds = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1);
    let options = SolveOptions {
        budget: Duration::from_secs(seconds),
        ..SolveOptions::default()
    };
    for _ in 0..3 {
        match opening::solve(&problem, &options)? {
            Outcome::Optimal { solution, timings } => {
                println!("optimal {} {timings:?}", solution.objective);
            }
            Outcome::Feasible { solution, timings } => {
                println!("feasible {} {timings:?}", solution.objective);
            }
            outcome => println!("{outcome:?}"),
        }
    }
    Ok(())
}
