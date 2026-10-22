# Game data contract

`shuai::game` follows the existing shuai-client field names. Its offline JSON
fixtures in `crates/shuai/src/game/tests.rs` define the accepted assumptions.
Live protocol verification remains an integration task.

## Numeric and terrain values

JSON arrays use integer tokens. Map dimensions are positive and army counts are
nonnegative JavaScript safe integers. Counts, positions, turns, score totals,
score tiles, and neutralization delays use `i64` with the same safe range.
Player identifiers retain the client's nonnegative `i32` range. Teams, colors,
and unrelated client events retain their existing `i32` declarations because
their fields never hold armies or map/diff offsets. The existing bot continues
to surrender at turn ten.

Outgoing `Attack` and `PingTile`, and incoming `PingTileOn`, retain their existing
`i32` tile-index API. Future state-to-network adapters must use checked
`i32::try_from` conversions or widen that API before emitting large positions.
Current state processing is independent of the sender.

| Terrain code       | Semantic value   |
| ------------------ | ---------------- |
| -1                 | Empty            |
| -2                 | Mountain         |
| -3                 | Fog              |
| -4                 | FogObstacle      |
| 0 through i32::MAX | Player(PlayerId) |

General positions preserve `-1` for undiscovered generals. Attack acknowledgements
preserve `-1`. City and special-tile positions identify actual map cells. Stars,
uncertainties, and raw visibility retain signed safe integers because the client
schema leaves their semantics unspecified. Alternate stars retain JSON values.
Special terrain lists remain separate from terrain codes.

## Diff and state lifecycle

A diff alternates unchanged counts and replacement runs. Each replacement run
contains a count followed by exactly that many values. A terminal unchanged count
and a terminal replacement run are both accepted. Zero counts are accepted. An
empty diff constructs an empty sequence. Output length is the sum of explicit
runs, so a prefix can shrink an old city list and replacement runs can grow it.
An unchanged run copies from the same output offset in the old sequence and must
fit entirely within that old sequence.

Every update reconstructs into reusable disjoint scratch vectors. Validation
covers the complete prospective map, coordinates, scores, and other update
fields. Commit copies scratch into the existing map buffer, reusing its capacity
when sufficient. This makes overlapping growth and shrinkage safe and preserves
all observable state on a failed update. Typed JSON deserialization allocates its
own event vectors independently of reconstruction.

`GameState::start` validates metadata and resets the current map and observation.
The first update reconstructs from empty map/city sequences. An omitted desert
diff preserves the previous desert list, initially taken from start metadata.
Other optional update fields describe the current observation, so an omitted
field becomes `None`. Unknown JSON object fields follow Serde's default behavior
and are ignored. Required fields remain required. Position validation uses map
bounds without inferring relationships between usernames, scores, or generals.

## Generated maps

`random_connected_map(width, height, seed)` uses weighted-grid Kruskal MST
construction and marks its leaves as mountains. Surviving cells retain all
four-neighbor grid adjacencies. Equal weights use endpoint order as a stable
tie-break. Seed reproducibility follows the resolved fastrand implementation.

A one-cell grid stays neutral. A two-cell grid keeps linear cell zero neutral and
marks cell one as a mountain. These deterministic tiny-grid policies correct the
original algorithm's all-blocked result and preserve a nonempty connected
passable region. Ordinary grids follow the original leaf-removal rule.
