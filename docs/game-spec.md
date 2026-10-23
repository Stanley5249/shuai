# Game data specification

`shuai::game` follows the existing shuai-client field names. Its offline JSON
fixtures in `crates/shuai/src/game/tests.rs` define the accepted assumptions.
The protocol skill's evidence ledger records client-bundle verification and
remaining server/runtime uncertainty.

## Map layout

The snapshot layout is `[width, height, armies..., terrain...]`. Each data
segment has `width * height` elements, and the header defines the dimensions.
The row-major cell index is `row * width + col`. `Map` deserializes and
serializes this flat array using validated `i64` values.

Raw terrain codes and semantic terrain values describe the same cells. The
`shuai::game` API documentation covers borrowed views, indexing, and controlled
mutation over this representation.

## Numeric and terrain values

Map and diff arrays use integer tokens. Map dimensions are positive and army
counts are nonnegative JavaScript safe integers. Counts, positions, turns, score
totals, and score tiles use `i64` with the same safe range. Score totals and
tile counts also accept -1 for hidden Silent War values. Neutralization progress
uses a finite nonnegative fraction rather than an integer delay. Player
identifiers retain the client's nonnegative `i32` range. Start metadata also
accepts `INDEX_SPECTATOR` for the local viewer. Teams, colors, and unrelated
client events retain their existing `i32` declarations because their fields
never hold armies or map/diff offsets.

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
map cells. Stars and uncertainties preserve fractional ratings as finite `f64`
values within the JavaScript safe numeric range. Raw visibility retains signed
safe integers. Alternate stars retain JSON values. Special terrain lists remain
separate from terrain codes. Optional ping batches and chess-clock snapshots are
preserved in observations, with coordinates, player identifiers, and clock
finiteness validated before commit.

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
observation. Initial clock snapshots use the same finite-value and player-ID
checks as updates; clock configuration values must also be finite. The first
update reconstructs from empty map/city sequences. An omitted desert diff
preserves the previous desert list, initially taken from start metadata. Other
optional update fields describe the current observation, so an omitted field
becomes `None`. Unmodeled JSON object fields generally follow Serde's default
behavior and are ignored. Custom-option and queue-extension maps explicitly
retain extra keys. Required fields remain required. Position validation checks
map bounds. Array lengths for usernames, scores, and generals are independent.

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
