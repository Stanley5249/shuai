use cpsat::prelude::*;

use super::graph::Graph;
use super::{Error, Problem, SolveOptions};

pub struct Formulation {
    pub model: CpModel,
    pub lands: Vec<IntVar>,
    pub turns: Vec<IntVar>,
    pub edges: Vec<Vec<BoolVar>>,
    pub sinks: Vec<Vec<BoolVar>>,
}

pub fn build(
    problem: &Problem,
    options: &SolveOptions,
    graph: &Graph,
) -> Result<Formulation, Error> {
    let mut model = CpModel::new();
    let zero = model.new_var(0..=0);
    let max_lands = (problem.horizon / problem.generation_turns - 1).max(0);
    let max_length = problem.horizon / (problem.generation_turns + 1);
    let mut lands = vec![zero];
    for _ in 0..problem.path_count {
        let value = model.new_var(0..=max_lands);
        model.add(value.ge(problem.minimum_lands));
        lands.push(value);
    }
    let turns: Vec<_> = (0..=problem.path_count)
        .map(|_| model.new_var(0..=problem.horizon))
        .collect();
    model.add(turns[0].eq(lands[1] * problem.generation_turns));
    model.add(lands[problem.path_count].ge(max_length));
    model.add(turns[problem.path_count].eq(problem.horizon));
    let land_increments: Vec<_> = lands.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let turn_increments: Vec<_> = turns.windows(2).map(|pair| pair[1] - pair[0]).collect();
    for (land, duration) in land_increments.iter().zip(&turn_increments) {
        model.add(land.clone().le(max_length));
        model.add(duration.clone().le(max_length));
        model.add(land.clone().le(duration));
    }
    for index in 2..=problem.path_count {
        let available = turns[index - 1] - lands[index] * problem.generation_turns;
        model.add(available.clone().ge(0));
        let ended = model.new_var(Bool);
        model.add(turns[index].lt(problem.horizon).only_enforce_if(!ended));
        model.add(turns[index].eq(problem.horizon).only_enforce_if(ended));
        model.add(
            available
                .lt(problem.generation_turns)
                .only_enforce_if(!ended),
        );
    }
    add_hints(&mut model, &lands[1..], options.hints.lands.as_deref());
    add_hints(&mut model, &turns[1..], options.hints.turns.as_deref());
    if options.heuristics {
        for increments in [&land_increments, &turn_increments] {
            for pair in increments.windows(2) {
                model.add(pair[0].clone().ge(&pair[1]));
            }
        }
    }
    model.maximize(lands[problem.path_count]);

    let vertex_count = graph.original.len();
    let indices: Vec<_> = (0..vertex_count)
        .map(|index| i32::try_from(index).map_err(|_| Error::Input("local vertex overflow")))
        .collect::<Result<_, _>>()?;
    let mut visited = vec![model.new_var(false); vertex_count - 1];
    let mut edges = Vec::with_capacity(problem.path_count);
    let mut sinks = Vec::with_capacity(problem.path_count);
    for index in 0..problem.path_count {
        let vertices: Vec<_> = (1..vertex_count).map(|_| model.new_var(Bool)).collect();
        let selected: Vec<_> = graph.edges.iter().map(|_| model.new_var(Bool)).collect();
        let sink: Vec<_> = (0..vertex_count).map(|_| model.new_var(Bool)).collect();
        let new_visited: Vec<_> = (1..vertex_count).map(|_| model.new_var(Bool)).collect();
        let visit_count: LinearExpr = vertices.iter().map(|&value| LinearExpr::from(value)).sum();
        let union_count: LinearExpr = new_visited
            .iter()
            .map(|&value| LinearExpr::from(value))
            .sum();
        model.add(visit_count.le(&turn_increments[index]));
        model.add(union_count.eq(lands[index + 1]));
        model.add(Constraint::exactly_one(sink.iter().copied()));
        for ((&vertex, &old), &new) in vertices.iter().zip(&visited).zip(&new_visited) {
            model.add(Constraint::bool_or([vertex, old, !new]));
            model.add(Constraint::implication(vertex, new));
            model.add(Constraint::implication(old, new));
        }
        let arcs = vertices
            .iter()
            .enumerate()
            .map(|(index, &value)| (indices[index + 1], indices[index + 1], !value))
            .chain(
                graph
                    .edges
                    .iter()
                    .zip(&selected)
                    .map(|(&(first, second), &value)| (indices[first], indices[second], value)),
            )
            .chain(
                sink.iter()
                    .enumerate()
                    .map(|(index, &value)| (indices[index], 0, value)),
            );
        model.add(Constraint::circuit(arcs));
        visited = new_visited;
        edges.push(selected);
        sinks.push(sink);
    }
    Ok(Formulation {
        model,
        lands,
        turns,
        edges,
        sinks,
    })
}

fn add_hints(model: &mut CpModel, variables: &[IntVar], hints: Option<&[i64]>) {
    if let Some(hints) = hints {
        let mut cumulative = 0;
        for (&variable, &increment) in variables.iter().zip(hints) {
            cumulative += increment;
            model.add_hint(variable, cumulative);
        }
    }
}
