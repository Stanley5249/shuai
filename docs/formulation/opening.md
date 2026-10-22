# Opening formulation

`shuai-cpsat::opening` accepts graph vertices and scheduling parameters
directly. Callers supply passable adjacency and apply city/obstacle policy
before calling the solver. Each vertex ID is its original input index. Duplicate
undirected edges and self-loops are input errors. Neighborhood extraction
traverses both directions, includes vertices within `radius`, and uses explicit
BFS mappings.

## Model and scheduling

The circuit expands each undirected input edge into both directed arcs. The
reported optimum is relative to the chosen scheduling and heuristic constraints.
Each path adds self-loops for skipped vertices and one artificial sink-to-start
closure. A start self-loop forces an empty route. The returned path omits the
artificial closure. Paths may revisit tiles visited by earlier paths, while each
individual path is simple. Bidirectional arcs make routes independent of stored
edge orientation. The empty-route condition prevents a disconnected cycle from
collecting land while the start is skipped.

Let `L[i]` count the visited union after path `i`, and `T[i]` be the turn when
path `i` starts. Cumulative generation, duration, horizon, and visited-union
constraints define the schedule. The objective maximizes the final union,
excluding the start, subject to total-land and maximum-path-length bounds.
Consequently, a start-only graph with a positive horizon is a valid problem with
an infeasible outcome.

`minimum_lands` is an explicit lower bound on every cumulative land count,
independent of search budget. An excessive lower bound produces an infeasible
outcome through a constraint on a valid variable domain. The seconds-based
generation period remains validated input for callers that choose a
latency-derived lower bound.

Hints contain one entry per path. Land entries are cumulative-count increments.
Turn entries are increments of cumulative end turns measured from zero, so the
first includes initial generation wait. Entries and cumulative sums must fit
their variable domains. Hints guide search and can describe a partially
inconsistent schedule that CP-SAT repairs. Optional nonincreasing land and turn
constraints change the feasible region and are controlled by `heuristics`.

Scheduling values, products, aggregate domain estimates, and model sizes are
checked conservatively before construction. Worker counts are positive `i32`
values. Zero budget returns `BudgetExhausted` after input validation. A positive
budget covers preprocessing and model construction, and the solver receives only
the positive remaining time. Shutdown and reconstruction can overshoot because
the budget is cooperative.

## Results and validation

Optimal and feasible outcomes carry independently validated paths, start turns,
durations, integer objectives, and phase timings. Infeasible outcomes represent
a proof. `NoIncumbent` preserves the unknown solver status and its uncertain
stop reason. Budget exhaustion before search has its own outcome. Input
failures, model rejection, bridge failures, and inconsistent solutions are
errors.

Reconstruction follows selected successors with bounded vertex visits and checks
graph edges, timing, visited unions, and objectives. Explicit BFS mappings
return original input vertex identifiers. Hint lengths and values follow the
current problem rather than a fixed path count.

## Tests and examples

`crates/shuai-cpsat/src/opening/tests.rs` contains independent tiny-case
enumeration and regression tests for reverse-edge traversal, disconnected
cycles, vertex mapping, parameter validation, hints, and heuristic modes. Run
these offline checks and examples from the workspace root:

```sh
cargo test --locked -p shuai-cpsat --lib
cargo run --locked -p shuai-cpsat --example opening
cargo run --locked -p shuai-cpsat --example opening_grid -- 1
```

The `opening` example solves a small branch problem. The grid example runs the
same fixture three times; its argument selects the budget in seconds. Reported
incumbents prove feasibility, while only an optimal outcome proves optimality.
Larger bidirectional models can need more search time. Performance depends on
the solver build, configuration, and host; the examples provide repeatable
inputs rather than guaranteed timings.

The reference formulation is `example/algorithm.py` in the
`Stanley5249/generals-io-glhf` repository. This document and Rust tests define
this crate's supported behavior.
