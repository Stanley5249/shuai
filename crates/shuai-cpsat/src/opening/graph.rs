use std::collections::{HashSet, VecDeque};

use super::{Error, Problem, SolveOptions};

pub struct Graph {
    pub original: Vec<usize>,
    pub edges: Vec<(usize, usize)>,
}

pub fn validate(problem: &Problem, options: &SolveOptions) -> Result<(), Error> {
    if problem.vertex_count == 0 || problem.start >= problem.vertex_count {
        return Err(Error::Input("start must identify an existing vertex"));
    }
    if problem.path_count == 0
        || problem.horizon <= 0
        || problem.generation_turns <= 0
        || !problem.generation_seconds.is_finite()
        || problem.generation_seconds <= 0.0
        || problem.minimum_lands < 0
    {
        return Err(Error::Input("invalid schedule or generation period"));
    }
    if options.workers == 0 || i32::try_from(options.workers).is_err() {
        return Err(Error::Input("workers must be positive and fit i32"));
    }
    if i32::try_from(problem.vertex_count).is_err() || i32::try_from(problem.path_count).is_err() {
        return Err(Error::Input("vertex or path count exceeds solver indices"));
    }
    // OR-Tools reserves extremes as infinity and checks summed domain activity.
    let safe = i64::MAX / 16;
    if problem.horizon > safe || problem.generation_turns > safe || problem.minimum_lands > safe {
        return Err(Error::Input("schedule exceeds safe solver arithmetic"));
    }
    problem
        .horizon
        .checked_mul(problem.generation_turns)
        .filter(|&value| value <= safe)
        .ok_or(Error::Input("generation-domain product overflow"))?;
    let count = problem
        .path_count
        .checked_mul(
            problem
                .vertex_count
                .checked_mul(12)
                .ok_or(Error::Input("model size overflow"))?
                .checked_add(
                    problem
                        .edges
                        .len()
                        .checked_mul(2)
                        .ok_or(Error::Input("arc count overflow"))?,
                )
                .and_then(|size| size.checked_add(12))
                .ok_or(Error::Input("model size overflow"))?,
        )
        .ok_or(Error::Input("model size overflow"))?;
    if i32::try_from(count).is_err() {
        return Err(Error::Input("model exceeds solver variable indices"));
    }
    let bound = i64::try_from(count).map_err(|_| Error::Input("model domain size overflow"))?;
    problem
        .horizon
        .checked_mul(bound)
        .filter(|&value| value <= safe)
        .ok_or(Error::Input("aggregate solver domains overflow"))?;
    let mut unique = HashSet::new();
    for &(first, second) in &problem.edges {
        if first >= problem.vertex_count || second >= problem.vertex_count || first == second {
            return Err(Error::Input("edge endpoint invalid or self-loop"));
        }
        if !unique.insert((first.min(second), first.max(second))) {
            return Err(Error::Input("duplicate undirected edge"));
        }
    }
    let max_lands = (problem.horizon / problem.generation_turns - 1).max(0);
    for (hints, maximum, minimum) in [
        (&options.hints.lands, max_lands, problem.minimum_lands),
        (&options.hints.turns, problem.horizon, 0),
    ] {
        if let Some(hints) = hints {
            if hints.len() != problem.path_count {
                return Err(Error::Input("hint length differs from path count"));
            }
            let mut cumulative = 0_i64;
            for &increment in hints {
                if increment < 0 {
                    return Err(Error::Input("hint increment must be nonnegative"));
                }
                cumulative = cumulative
                    .checked_add(increment)
                    .ok_or(Error::Input("hint sum overflow"))?;
                if cumulative < minimum || cumulative > maximum {
                    return Err(Error::Input("cumulative hint outside variable domain"));
                }
            }
        }
    }
    Ok(())
}

pub fn neighborhood(problem: &Problem) -> Graph {
    let mut adjacency = vec![Vec::new(); problem.vertex_count];
    for &(first, second) in &problem.edges {
        adjacency[first].push(second);
        adjacency[second].push(first);
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    let mut original = Vec::new();
    let mut local = vec![None; problem.vertex_count];
    let mut queue = VecDeque::from([(problem.start, 0)]);
    local[problem.start] = Some(0);
    let mut discovered = 1;
    while let Some((vertex, depth)) = queue.pop_front() {
        original.push(vertex);
        if depth == problem.radius {
            continue;
        }
        for &neighbor in &adjacency[vertex] {
            if local[neighbor].is_none() {
                local[neighbor] = Some(discovered);
                discovered += 1;
                queue.push_back((neighbor, depth + 1));
            }
        }
    }
    let edges = problem
        .edges
        .iter()
        .filter_map(|&(first, second)| Some((local[first]?, local[second]?)))
        .flat_map(|(first, second)| [(first, second), (second, first)])
        .collect();
    Graph { original, edges }
}
