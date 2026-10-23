use fastrand::Rng;

use super::{Error, Map};

/// Generates a weighted grid MST and turns its leaves into mountains.
///
/// Assigns random weights to four-neighbor grid edges, then selects a minimum
/// spanning tree with Kruskal's algorithm. Endpoint order breaks equal-weight
/// ties. Tree vertices with degree at most one become mountains, and armies
/// start at zero. The remaining terrain is empty.
///
/// The surviving tiles keep every original grid adjacency. A one-cell grid
/// remains empty. A two-cell grid keeps cell zero empty and marks cell one as
/// a mountain, so every generated map has a nonempty connected passable region.
/// Seeds are deterministic for the resolved fastrand implementation.
///
/// # Errors
/// Returns an error for zero dimensions or dimensions exceeding map bounds.
pub fn random_connected_map(width: usize, height: usize, seed: u64) -> Result<Map, Error> {
    let mut raw = vec![0; super::map_length(width, height)?];
    let size = width * height;
    let mut rng = Rng::with_seed(seed);
    let mut edges = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let vertex = row * width + column;
            if column + 1 < width {
                edges.push((rng.u64(..), vertex, vertex + 1));
            }
            if row + 1 < height {
                edges.push((rng.u64(..), vertex, vertex + width));
            }
        }
    }
    let degree = tree_degrees(size, &mut edges);
    raw[0] = i64::try_from(width).map_err(|_| Error("width overflow"))?;
    raw[1] = i64::try_from(height).map_err(|_| Error("height overflow"))?;
    for (vertex, &degree) in degree.iter().enumerate() {
        raw[2 + size + vertex] = if (size > 2 && degree <= 1) || (size == 2 && vertex == 1) {
            -2
        } else {
            -1
        };
    }
    Map::try_from(raw)
}

fn tree_degrees(size: usize, edges: &mut [(u64, usize, usize)]) -> Vec<usize> {
    edges.sort_unstable();
    let mut parent: Vec<_> = (0..size).collect();
    let mut degree = vec![0; size];
    for &(_, first, second) in edges.iter() {
        let first_root = root(&mut parent, first);
        let second_root = root(&mut parent, second);
        if first_root != second_root {
            parent[first_root] = second_root;
            degree[first] += 1;
            degree[second] += 1;
        }
    }
    degree
}

fn root(parent: &mut [usize], mut vertex: usize) -> usize {
    while parent[vertex] != vertex {
        parent[vertex] = parent[parent[vertex]];
        vertex = parent[vertex];
    }
    vertex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_weights_select_mst_leaves() {
        // The first three edges form 0-1-3-2, whose leaves are 0 and 2.
        let mut edges = [(1, 0, 1), (4, 0, 2), (3, 2, 3), (2, 1, 3)];
        assert_eq!(tree_degrees(4, &mut edges), [1, 2, 1, 2]);
    }
}
