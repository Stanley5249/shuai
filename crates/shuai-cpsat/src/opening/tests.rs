use std::collections::HashSet;

use super::*;

fn chain() -> Problem {
    Problem {
        vertex_count: 3,
        edges: vec![(0, 1), (1, 2)],
        start: 0,
        radius: 8,
        horizon: 4,
        path_count: 1,
        generation_turns: 1,
        generation_seconds: 1.0,
        minimum_lands: 0,
    }
}

fn objective(problem: &Problem, options: &SolveOptions) -> Option<i64> {
    match solve(problem, options).unwrap() {
        Outcome::Optimal { solution, .. } => Some(solution.objective),
        Outcome::Infeasible { .. } => None,
        other => panic!("tiny case must finish with a proof: {other:?}"),
    }
}

#[test]
fn tiny_chain_and_nontrivial_vertex_mapping() {
    let mut problem = chain();
    let options = SolveOptions::default();
    assert_eq!(objective(&problem, &options), Some(2));
    problem.vertex_count = 7;
    problem.start = 4;
    problem.edges = vec![(4, 1), (1, 3), (0, 6)];
    let graph = graph::neighborhood(&problem);
    assert_eq!(graph.original, [4, 1, 3]);
    assert_eq!(graph.edges, [(0, 1), (1, 0), (1, 2), (2, 1)]);
    let Outcome::Optimal { solution, .. } = solve(&problem, &options).unwrap() else {
        panic!("chain is optimal");
    };
    assert_eq!(
        solution.paths[0],
        Path {
            start_turn: 2,
            duration: 2,
            vertices: vec![4, 1, 3]
        }
    );
    problem.radius = 1;
    assert_eq!(objective(&problem, &options), None);
}

#[test]
fn undirected_edge_order_start_only_and_disconnected_contracts() {
    let mut problem = chain();
    let options = SolveOptions::default();
    problem.edges[0] = (1, 0);
    assert_eq!(objective(&problem, &options), Some(2));
    problem.vertex_count = 1;
    problem.edges.clear();
    assert_eq!(objective(&problem, &options), None);
    problem.vertex_count = 4;
    problem.edges = vec![(1, 2), (2, 3)];
    assert_eq!(objective(&problem, &options), None);
    problem.horizon = 1;
    problem.generation_turns = 2;
    assert_eq!(objective(&problem, &options), None);
}

#[test]
fn reverse_internal_edge_and_empty_route_cycle_regressions() {
    let mut problem = chain();
    problem.edges = vec![(0, 2), (1, 2)];
    let options = SolveOptions::default();
    let Outcome::Optimal { solution, .. } = solve(&problem, &options).unwrap() else {
        panic!("reverse internal edge is traversable");
    };
    assert_eq!(solution.objective, 2);
    assert_eq!(solution.paths[0].vertices, [0, 2, 1]);
    let graph = graph::neighborhood(&problem);
    let mut model = model::build(&problem, &options, &graph).unwrap();
    model.model.add(model.sinks[0][0].eq(1));
    // With the start skipped, the 1<->2 cycle must not collect two lands.
    let response = model.model.solve(&SatParameters::default()).unwrap();
    assert_eq!(response.status(), CpSolverStatus::INFEASIBLE);
}

#[test]
fn bidirectional_model_sizes_match_the_documented_comparison() {
    let mut problem = chain();
    problem.vertex_count = 5;
    problem.edges = vec![(0, 1), (1, 2), (0, 3), (3, 4)];
    problem.path_count = 2;
    problem.horizon = 6;
    let options = SolveOptions::default();
    let graph = graph::neighborhood(&problem);
    let model = model::build(&problem, &options, &graph).unwrap();
    assert_eq!(graph.edges.len(), 8);
    assert_eq!(model.model.proto().variables.len(), 50);
    assert_eq!(model.model.proto().constraints.len(), 51);
    assert_eq!(model.edges.iter().map(Vec::len).sum::<usize>(), 16);
}

#[test]
fn validates_scheduling_graph_workers_and_hints_before_budget() {
    let mut options = SolveOptions {
        budget: Duration::ZERO,
        ..SolveOptions::default()
    };
    assert!(matches!(
        solve(&chain(), &options).unwrap(),
        Outcome::BudgetExhausted { .. }
    ));
    for mutation in 0..12 {
        let mut problem = chain();
        match mutation {
            0 => problem.vertex_count = 0,
            1 => problem.start = 3,
            2 => problem.path_count = 0,
            3 => problem.horizon = 0,
            4 => problem.generation_turns = 0,
            5 => problem.generation_seconds = 0.0,
            6 => problem.generation_seconds = f64::NAN,
            7 => problem.minimum_lands = -1,
            8 => problem.edges.push((1, 0)),
            9 => problem.edges.push((1, 1)),
            10 => problem.edges.push((1, 3)),
            _ => problem.horizon = i64::MAX,
        }
        solve(&problem, &options).unwrap_err();
    }
    for workers in [0, usize::MAX] {
        options.workers = workers;
        solve(&chain(), &options).unwrap_err();
    }
    options.workers = 1;
    for hints in [vec![], vec![-1], vec![4], vec![i64::MAX]] {
        options.hints.lands = Some(hints);
        solve(&chain(), &options).unwrap_err();
    }
    options.hints = Hints {
        lands: Some(vec![2]),
        turns: Some(vec![4]),
    };
    assert!(matches!(
        solve(&chain(), &options).unwrap(),
        Outcome::BudgetExhausted { .. }
    ));
    options.budget = Duration::from_secs(5);
    assert_eq!(objective(&chain(), &options), Some(2));
    options.hints = Hints::default();
    options.budget = Duration::from_nanos(1);
    assert!(matches!(
        solve(&chain(), &options).unwrap(),
        Outcome::BudgetExhausted { .. }
    ));
    let mut problem = chain();
    problem.minimum_lands = 6;
    assert_eq!(objective(&problem, &SolveOptions::default()), None);
}

#[test]
fn independent_enumeration_checks_paths_schedules_and_heuristic_modes() {
    let mut problem = chain();
    problem.vertex_count = 5;
    problem.edges = vec![(0, 1), (1, 2), (0, 3), (3, 4)];
    problem.horizon = 6;
    for paths in 1..=3 {
        problem.path_count = paths;
        for heuristics in [false, true] {
            let options = SolveOptions {
                heuristics,
                ..SolveOptions::default()
            };
            assert_eq!(
                objective(&problem, &options),
                enumerate(&problem, heuristics)
            );
        }
    }
    problem.edges = vec![(0, 2), (1, 2), (1, 3), (3, 4), (4, 0)];
    problem.path_count = 2;
    for heuristics in [false, true] {
        let options = SolveOptions {
            heuristics,
            ..SolveOptions::default()
        };
        assert_eq!(
            objective(&problem, &options),
            enumerate(&problem, heuristics)
        );
    }
    problem.edges = vec![(0, 1), (1, 2), (0, 3), (3, 4)];
    problem.path_count = 2;
    problem.minimum_lands = 3;
    assert_eq!(objective(&problem, &SolveOptions::default()), None);
}

fn enumerate(problem: &Problem, heuristics: bool) -> Option<i64> {
    let mut paths = vec![vec![problem.start]];
    let mut cursor = 0;
    while cursor < paths.len() {
        let path = &paths[cursor];
        let tail = *path.last().unwrap();
        let extensions: Vec<_> = problem
            .edges
            .iter()
            .flat_map(|&(first, second)| [(first, second), (second, first)])
            .filter_map(|(first, second)| {
                if first == tail && !path.contains(&second) {
                    let mut extended = path.clone();
                    extended.push(second);
                    Some(extended)
                } else {
                    None
                }
            })
            .collect();
        paths.extend(extensions);
        cursor += 1;
    }
    let mut best = None;
    search(
        problem,
        heuristics,
        &paths,
        &HashSet::new(),
        0,
        0,
        None,
        &mut best,
    );
    best
}

#[expect(
    clippy::too_many_arguments,
    reason = "the tiny independent oracle keeps its recursion state explicit"
)]
fn search(
    problem: &Problem,
    heuristics: bool,
    paths: &[Vec<usize>],
    old: &HashSet<usize>,
    index: usize,
    time: i64,
    previous: Option<(i64, i64)>,
    best: &mut Option<i64>,
) {
    let max_length = problem.horizon / (problem.generation_turns + 1);
    for path in paths {
        let mut union = old.clone();
        union.extend(path.iter().skip(1).copied());
        let lands = i64::try_from(union.len()).unwrap();
        let increment = lands - i64::try_from(old.len()).unwrap();
        let start = if index == 0 {
            lands * problem.generation_turns
        } else {
            time
        };
        if lands < problem.minimum_lands
            || lands > problem.horizon / problem.generation_turns - 1
            || (index > 0 && start < lands * problem.generation_turns)
        {
            continue;
        }
        for duration in i64::try_from(path.len() - 1).unwrap()..=max_length {
            let end = start + duration;
            if end > problem.horizon
                || (index > 0
                    && end < problem.horizon
                    && start - lands * problem.generation_turns >= problem.generation_turns)
                || (heuristics
                    && previous.is_some_and(|(land, turns)| increment > land || duration > turns))
            {
                continue;
            }
            if index + 1 == problem.path_count {
                if end == problem.horizon && lands >= max_length {
                    *best = Some(best.map_or(lands, |value| value.max(lands)));
                }
            } else {
                search(
                    problem,
                    heuristics,
                    paths,
                    &union,
                    index + 1,
                    end,
                    Some((increment, duration)),
                    best,
                );
            }
        }
    }
}
