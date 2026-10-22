use shuai_cpsat::opening::{self, Outcome, Problem, SolveOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let problem = Problem {
        vertex_count: 5,
        edges: vec![(0, 1), (1, 2), (0, 3), (3, 4)],
        start: 0,
        radius: 8,
        horizon: 6,
        path_count: 2,
        generation_turns: 1,
        generation_seconds: 1.0,
        minimum_lands: 1,
    };
    match opening::solve(&problem, &SolveOptions::default())? {
        Outcome::Optimal { solution, timings } | Outcome::Feasible { solution, timings } => {
            println!(
                "objective {}, paths {:?}",
                solution.objective, solution.paths
            );
            println!("timings {timings:?}");
        }
        outcome => println!("{outcome:?}"),
    }
    Ok(())
}
