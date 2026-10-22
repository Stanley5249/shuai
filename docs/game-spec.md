# Game data specification

`shuai::game` follows the existing shuai-client field names. Its offline JSON
fixtures in `crates/shuai/src/game/tests.rs` define the accepted assumptions.
Live protocol verification remains an integration task.

## Map layout

The snapshot layout is `[width, height, armies..., terrain...]`. Each data
segment has `width * height` elements, and the header defines the dimensions.
The row-major cell index is `row * width + col`. `Map` deserializes and
serializes this flat array using validated `i64` values.

Raw terrain codes and semantic terrain values describe the same cells. The
`shuai::game` API documentation covers borrowed views, indexing, and controlled
mutation over this representation.

## Numeric and terrain values

JSON arrays use integer tokens. Map dimensions are positive and army counts are
nonnegative JavaScript safe integers. Counts, positions, turns, score totals,
score tiles, and neutralization delays use `i64` with the same safe range.
Player identifiers retain the client's nonnegative `i32` range. Teams, colors,
and unrelated client events retain their existing `i32` declarations because
their fields never hold armies or map/diff offsets.

Outgoing `Attack` and `PingTile`, and incoming `PingTileOn`, retain their
existing `i32` tile-index API. Future state-to-network adapters must use checked
`i32::try_from` conversions or widen that API before emitting large positions.
Current state processing is independent of the sender.

| Terrain code       | Semantic value   |
| ------------------ | ---------------- |
| -1                 | Empty            |
| -2                 | Mountain         |
| -3                 | Fog              |
| -4                 | FogObstacle      |
| 0 through i32::MAX | Player(PlayerId) |

General positions preserve `-1` for undiscovered generals. Attack
acknowledgements preserve `-1`. City and special-tile positions identify actual
map cells. Stars, uncertainties, and raw visibility retain signed safe integers
because the client schema leaves their semantics unspecified. Alternate stars
retain JSON values. Special terrain lists remain separate from terrain codes.

## Diff and state lifecycle

A diff alternates unchanged counts and replacement runs. Each replacement run
contains a count followed by exactly that many values. A terminal unchanged
count and a terminal replacement run are both accepted. Zero counts are
accepted. An empty diff constructs an empty sequence. Output length is the sum
of explicit runs, so a prefix can shrink an old city list and replacement runs
can grow it. An unchanged run copies from the same output offset in the old
sequence and must fit entirely within that old sequence.

Validation covers the complete prospective map, coordinates, scores, and other
update fields. A valid update commits related observation fields together. A
failed update preserves all observable state. The `shuai::game` API
documentation describes the buffer-reuse and copy behavior.

`GameState::start` validates metadata and resets the current map and
observation. The first update reconstructs from empty map/city sequences. An
omitted desert diff preserves the previous desert list, initially taken from
start metadata. Other optional update fields describe the current observation,
so an omitted field becomes `None`. Unknown JSON object fields follow Serde's
default behavior and are ignored. Required fields remain required. Position
validation uses map bounds without inferring relationships between usernames,
scores, or generals.

## Tests and allocation example

`crates/shuai/src/game/tests.rs` covers the specification above. From the
workspace root, run the focused library tests and the offline allocation
example:

```sh
cargo test --locked -p shuai --lib
cargo run --locked -p shuai --example game_updates
```

The example counts allocation/reallocation calls for warmed, predeserialized
stable-size updates. Initial event parsing and scratch allocation are outside
the measured interval. Allocation reuse and scratch-to-map copying are separate
costs, and timings depend on the build and host environment.
