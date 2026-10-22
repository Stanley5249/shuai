# Bidirectional opening follow-up

The main migration passed its acceptance gates and was committed as `79f7a66`
with the Python model's stored-edge orientation restriction preserved. This
separate improvement expands each undirected edge into both directed circuit
arcs, making feasible routes independent of the order of input edge endpoints.

## Correctness

The chain with stored edges `(0, 2), (1, 2)` and start zero is infeasible in the
migrated baseline for one path, horizon four, and generation period one. The
bidirectional model proves objective two and returns `[0, 2, 1]`. Reversing the
first edge of the ordinary three-vertex chain also preserves its optimum two.
The independent tiny-case oracle now enumerates both edge directions and checks
graphs containing cycles as well as branches.

Arc expansion also requires an empty-route constraint. A CP-SAT circuit can
skip the general using its self-loop and select a cycle among other vertices.
The acyclic baseline fixtures hid that case. The improved model forces zero
visited vertices whenever the start self-loop is selected. A regression test
forces that self-loop in a two-land chain and proves infeasibility, ruling out
disconnected land collection. Successor-based reconstruction and all independent
schedule/union/objective checks remain active.

## Model size and solve measurements

The branch fixture has five vertices, four undirected edges, and two paths. The
grid fixture has 64 vertices, 112 undirected edges, and five paths. Counts below
come from the source model structure. Tests check the improved branch counts.

| Fixture  | Model              | Variables | Constraints | Circuit arcs across all paths |
| -------- | ------------------ | --------- | ----------- | ----------------------------- |
| Branches | Migration baseline | 42        | 49          | 26                            |
| Branches | Bidirectional      | 50        | 51          | 34                            |
| 8x8 grid | Migration baseline | 1527      | 1012        | 1195                          |
| 8x8 grid | Bidirectional      | 2087      | 1017        | 1755                          |

Debug smoke runs used one worker, heuristics enabled, presolve disabled, and the
same resolved solver build. The grid used horizon 50, generation period two,
radius 16, and explicit minimum lands eight. Each row describes three runs.

| Model              | Budget    | Outcome in all three runs        | Model construction | Search call     |
| ------------------ | --------- | -------------------------------- | ------------------ | --------------- |
| Migration baseline | 1 second  | Feasible incumbent, objective 21 | 2.74-3.32 ms       | 1.0007-1.0015 s |
| Bidirectional      | 1 second  | UNKNOWN, no incumbent            | 3.45-5.70 ms       | 0.9990-1.0027 s |
| Bidirectional      | 5 seconds | Feasible incumbent, objective 22 | 3.58-4.20 ms       | 5.0009-5.0035 s |

These grid incumbents carry a feasibility result and an unproven optimum. The
measurements demonstrate a larger search space and a real short-budget cost on
this fixture. They establish route correctness separately from search quality.
The tiny branch fixture still proves objective four within a few milliseconds.

`cargo run -p shuai-cpsat --example opening_grid -- 1` repeats the current grid
comparison fixture three times. Supplying `5` selects a five-second budget. The
earlier baseline observations were captured before arc expansion. The ordinary
`opening` example remains a small solved demonstration. Presolve tuning, search
hints for large grids, and broader benchmarks remain future optimization work.
