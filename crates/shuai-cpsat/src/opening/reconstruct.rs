use std::collections::HashSet;

use cpsat::proto::CpSolverResponse;

use super::graph::Graph;
use super::model::Formulation;
use super::{Error, Path, Problem, Solution, SolveOptions};

pub fn read(
    problem: &Problem,
    options: &SolveOptions,
    graph: &Graph,
    model: &Formulation,
    response: &CpSolverResponse,
) -> Result<Solution, Error> {
    let values = response
        .solution()
        .ok_or(Error::Reconstruction("incumbent missing"))?;
    let lands: Vec<_> = model
        .lands
        .iter()
        .map(|&variable| values.value(variable))
        .collect();
    let turns: Vec<_> = model
        .turns
        .iter()
        .map(|&variable| values.value(variable))
        .collect();
    let mut visited = HashSet::new();
    let mut paths = Vec::with_capacity(problem.path_count);
    for index in 0..problem.path_count {
        let selected: Vec<_> = graph
            .edges
            .iter()
            .zip(&model.edges[index])
            .filter_map(|(&edge, &variable)| values.value(variable).then_some(edge))
            .collect();
        let sinks: Vec<_> = model.sinks[index]
            .iter()
            .enumerate()
            .filter_map(|(vertex, &variable)| values.value(variable).then_some(vertex))
            .collect();
        if sinks.len() != 1 {
            return Err(Error::Reconstruction("exactly one sink required"));
        }
        let local_path = link(graph.original.len(), &selected, sinks[0])?;
        let duration = turns[index + 1] - turns[index];
        let path_length = i64::try_from(local_path.len() - 1)
            .map_err(|_| Error::Reconstruction("path size overflow"))?;
        if duration < path_length {
            return Err(Error::Reconstruction("path exceeds duration"));
        }
        for &vertex in local_path.iter().skip(1) {
            visited.insert(vertex);
        }
        if i64::try_from(visited.len()).ok() != Some(lands[index + 1]) {
            return Err(Error::Reconstruction(
                "visited union differs from cumulative lands",
            ));
        }
        paths.push(Path {
            start_turn: turns[index],
            duration,
            vertices: local_path
                .into_iter()
                .map(|vertex| graph.original[vertex])
                .collect(),
        });
    }
    verify_schedule(problem, options, &lands, &turns)?;
    let objective = lands[problem.path_count];
    let checked_objective = i32::try_from(objective)
        .map_err(|_| Error::Reconstruction("objective exceeds vertex range"))?;
    #[expect(
        clippy::float_cmp,
        reason = "an i32-bounded integer objective is represented exactly by f64"
    )]
    if response.objective_value != f64::from(checked_objective) {
        return Err(Error::Reconstruction(
            "response objective differs from visited union",
        ));
    }
    Ok(Solution { paths, objective })
}

fn verify_schedule(
    problem: &Problem,
    options: &SolveOptions,
    lands: &[i64],
    turns: &[i64],
) -> Result<(), Error> {
    let max_length = problem.horizon / (problem.generation_turns + 1);
    let max_lands = (problem.horizon / problem.generation_turns - 1).max(0);
    if lands[0] != 0
        || turns[0] != lands[1] * problem.generation_turns
        || turns[problem.path_count] != problem.horizon
        || lands[problem.path_count] < max_length
    {
        return Err(Error::Reconstruction(
            "schedule endpoints violate formulation",
        ));
    }
    let mut previous = None;
    for index in 0..problem.path_count {
        let land = lands[index + 1] - lands[index];
        let duration = turns[index + 1] - turns[index];
        if lands[index + 1] < problem.minimum_lands
            || lands[index + 1] > max_lands
            || land < 0
            || land > duration
            || land > max_length
            || duration > max_length
            || !(0..=problem.horizon).contains(&turns[index])
        {
            return Err(Error::Reconstruction("schedule domain violation"));
        }
        if index > 0 {
            let available = turns[index] - lands[index + 1] * problem.generation_turns;
            if available < 0
                || (turns[index + 1] < problem.horizon && available >= problem.generation_turns)
            {
                return Err(Error::Reconstruction("army generation timing violation"));
            }
        }
        if options.heuristics
            && previous
                .is_some_and(|(old_land, old_duration)| old_land < land || old_duration < duration)
        {
            return Err(Error::Reconstruction("heuristic order violation"));
        }
        previous = Some((land, duration));
    }
    Ok(())
}

fn link(
    vertex_count: usize,
    selected: &[(usize, usize)],
    sink: usize,
) -> Result<Vec<usize>, Error> {
    let mut successor = vec![None; vertex_count];
    let mut indegree = vec![0; vertex_count];
    for &(first, second) in selected {
        if first >= vertex_count
            || second >= vertex_count
            || first == second
            || successor[first].replace(second).is_some()
        {
            return Err(Error::Reconstruction("invalid selected successor"));
        }
        indegree[second] += 1;
        if indegree[second] > 1 {
            return Err(Error::Reconstruction("multiple selected predecessors"));
        }
    }
    let mut path = vec![0];
    let mut visited = vec![false; vertex_count];
    visited[0] = true;
    let mut vertex = 0;
    while vertex != sink {
        let next = successor[vertex].ok_or(Error::Reconstruction("path ends before sink"))?;
        if visited[next] {
            return Err(Error::Reconstruction("selected cycle"));
        }
        visited[next] = true;
        path.push(next);
        vertex = next;
    }
    if successor[sink].is_some() || path.len() - 1 != selected.len() {
        return Err(Error::Reconstruction("disconnected selected edges"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_disconnected_cycles_and_ambiguous_successors() {
        for selected in [
            vec![(0, 1), (2, 3)],
            vec![(0, 1), (1, 0)],
            vec![(0, 1), (0, 2)],
            vec![(0, 1), (2, 1)],
        ] {
            link(4, &selected, 3).unwrap_err();
        }
        assert_eq!(link(4, &[(0, 2), (2, 3)], 3).unwrap(), [0, 2, 3]);
        assert_eq!(link(1, &[], 0).unwrap(), [0]);
    }
}
