# Opening formulation contract

`shuai-cpsat::opening` accepts graph vertices and scheduling parameters directly.
Callers supply passable adjacency and apply city/obstacle policy before calling
the solver. Each vertex ID is its original input index. Duplicate undirected
edges and self-loops are input errors. Neighborhood extraction traverses both
directions, includes vertices within `radius`, and uses explicit BFS mappings.

## Model and scheduling

The migrated circuit uses one stored orientation per undirected input edge.
This is a known Python model limitation and restricts feasible routes. The
reported optimum is relative to that orientation and the chosen heuristic
constraints. Each path adds self-loops for skipped vertices and one artificial
sink-to-start closure. The returned path omits that closure. Paths may revisit
tiles visited by earlier paths, while each individual path is simple.

Let `L[i]` count the visited union after path `i`, and `T[i]` be the turn when
path `i` starts. The implementation retains the Python cumulative generation,
duration, horizon, and visited-union constraints. The objective maximizes the
final union, excluding the start. The Python lower bound on total lands and its
maximum path length are retained. Consequently, a start-only graph with a
positive horizon is a valid problem with an infeasible outcome.

The Python timeout-derived lower bound is an explicit `minimum_lands` input in
Rust. The Python expression is `ceil((timeout + 0.1) / generation_seconds)` and
applies to every cumulative land variable. Increasing a Python time budget can
therefore reduce feasibility and even create an invalid variable domain. Rust
budgets affect available search time. An excessive explicit lower bound yields
proven infeasibility through a regular constraint on a valid variable domain.
The seconds-based generation period remains validated input for callers that
choose their own latency-derived lower bound.

Hints contain one entry per path. Land entries are cumulative-count increments.
Turn entries are increments of cumulative end turns measured from zero, so the
first includes initial generation wait. Entries and cumulative sums must fit
their variable domains. Hints guide search and can describe a partially
inconsistent schedule that CP-SAT repairs. Optional nonincreasing land and turn
constraints change the feasible region and are controlled by `heuristics`.

Scheduling values, products, aggregate domain estimates, and model sizes are
checked conservatively before construction. Worker counts are positive `i32`
values. Zero budget returns `BudgetExhausted` after input validation. A positive
budget covers preprocessing and model construction, and the solver receives
only the positive remaining time. Shutdown and reconstruction can overshoot
because the budget is cooperative.

## Results and corrections

Optimal and feasible outcomes carry independently validated paths, start turns,
durations, integer objectives, and phase timings. Infeasible outcomes represent a
proof. `NoIncumbent` preserves the unknown solver status and its uncertain stop
reason. Budget exhaustion before search has its own outcome. Input failures,
model rejection, bridge failures, and inconsistent solutions are errors.

The migration corrects these Python defects:

- BFS mapping explicitly composes original and local IDs. A component containing
  original vertices `[1, 3, 4]` with start `4` exposes Python's inverse-permutation
  mistake. Its permuted vertex zero becomes original `3`; Rust keeps `4` at zero.
- Hint lengths follow `path_count`, with domain validation replacing the fixed
  five-path hints used by the Python wrapper.
- Reconstruction follows selected successors with bounded vertex visits and
  rejects disconnected edges, cycles, and ambiguous predecessors.
- Empty-domain scheduling bounds become valid models with infeasible outcomes.
- Preprocessing consumes the same monotonic budget as search, and CP-SAT always
  receives a positive remaining limit.

## Offline acceptance evidence

The independently enumerated tiny-case oracle in the Rust tests checks graph
paths, turn allocations, visited unions, and both heuristic modes. A two-edge
chain with one path and horizon four has optimum two. Two disjoint two-edge
branches with two paths and horizon six have optimum four. Both use generation
period one and the general at zero.

The original `scheduling_pcvrp` and `link_edges` functions were loaded from the
Python source through AST extraction. This runs their unchanged bodies while
avoiding unrelated server imports. `uv run --no-project --python 3.12` with
`ortools==9.12.4544` matched the source repository's lockfile. A 0.1-second budget
produced the same objectives and valid paths in both cases. Reversing the chain's
first stored edge produced `INFEASIBLE` in both models. Python's same chain with
a five-second budget produced `MODEL_INVALID` because its lower bound exceeded
its upper bound. Rust's explicit excessive bound produced `Infeasible`.

The first attempt with the latest Python OR-Tools hit a `status_name()` wrapper
compatibility error. The source lockfile's version supplied the comparison
baseline. The Rust dependency and its resolved revision are owned by Cargo.lock.

`cargo run -p shuai-cpsat --example opening` reproduces the branch fixture and
prints timings. One debug run measured about 0.15 ms preprocessing, 0.69 ms model
construction, 7.34 ms search, and 0.05 ms reconstruction. These are local smoke
measurements, with solver/runtime initialization included, rather than a
performance guarantee.

`cargo run -p shuai --example game_updates` measured zero allocation/reallocation
calls across 10,000 warmed, predeserialized stable-size updates, taking about
9.85 ms in a debug build. Event deserialization and initial scratch allocation
occur before the measured interval. Pointer/capacity tests independently verify
map-buffer reuse.
