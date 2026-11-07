# Attack queue and timing

This note answers [issue #2](https://github.com/Stanley5249/shuai/issues/2) for
the Opening bot. **No production game has been captured**, so production server
behavior stays inferred. Section "How to finish verification" states what is
missing and how to get it.

## Evidence

| Level | Meaning                                                                                                                                    |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| **B** | Browser code, read directly: what the client emits and consumes.                                                                           |
| **L** | The game simulation bundled for local games, read directly. It is a real implementation of the rules, but it is not the production server. |
| **O** | Observed live from the production server.                                                                                                  |
| **U** | Unknown.                                                                                                                                   |

Source: `https://generals.io/generals-main-prod-v31.4.6-df40179d.js`, SHA-256
`558140f5d7b4f7475933c48bec1978ab174909b020caf33d91260dd4ee99251e`, fetched
2026-11-07. Bracketed numbers are zero-based code-unit offsets into that file.
This release is newer than 31.4.4, which the
[protocol ledger](../../.agents/skills/generals-protocol/references/ground-truth.md)
audited, and only the code cited here was re-read. The bundle holds two copies
of the simulation; offsets refer to the prototype-style copy unless noted.

## Findings

### Queue ownership

| Finding                                                                                                                                                 | Level | Source                               |
| ------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ------------------------------------ |
| The queue is held by the simulation, one FIFO `inputBuffer` per player.                                                                                 | L     | `bufferAttack` [1327380]             |
| The browser also keeps a display-only list, and drops entries with `attackIndex <= inbound attackIndex`. It does not resend them.                       | B     | `UPDATE_ATTACK_INDEX` [475851]       |
| Each tick takes entries from the front until one is valid, so one valid move runs per tick and invalid moves are discarded in that same tick.           | L     | `getPlayerMovesPriorityV2` [1213175] |
| A move is valid only if the player is alive, owns the source, the source has more than 1 army (with a teammate exception), and the map allows the move. | L     | `checkAttackValid` [1328802]         |
| No capacity limit or expiry was found in the code read.                                                                                                 | L     | absence in the code read, not proof  |
| Production queue location, capacity, expiry and reconnect behavior.                                                                                     | U     |                                      |

### Indices

| Finding                                                                                                                                                   | Level | Source                                                         |
| --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | -------------------------------------------------------------- |
| The client assigns the outbound index. The browser counter starts at 1 and increments per submitted attack.                                               | B     | `useState(1)` and `ADD_ATTACK` [480800]                        |
| The inbound `attackIndex` starts at 0 and becomes `max(previous, index)` whenever an entry is taken from the queue, **whether or not the move is valid**. | L     | `checkAttackValidAndIncrementAttackIndex` [1329160], [1273081] |
| So the inbound index means "taken from the queue up to here". It does not prove the move succeeded.                                                       | L     | deduction from the two above                                   |
| Every update for a player carries that player's index.                                                                                                    | L     | `sendGameUpdate` [1321111]                                     |
| The Rust type also allows absent and `-1`. Whether production sends them is unknown.                                                                      | U     | see `GameUpdateData`                                           |

### Queue mutation

| Finding                                                                                                                      | Level | Source                          |
| ---------------------------------------------------------------------------------------------------------------------------- | ----- | ------------------------------- |
| `undo_move` removes the newest queued entry and does not change the index.                                                   | L     | `undoMove` [1336735]            |
| `clear_moves` empties the queue and does not change the index.                                                               | L     | `clearMoves` [1337141]          |
| A cleared or undone index is therefore never acknowledged, so waiting for it would hang.                                     | L     | deduction                       |
| Neither command is atomic with a new `attack`. An urgent move can only follow a clear as a separate packet, racing the tick. | L     | deduction                       |
| `start_holding_clear_moves` affects chess-clock turn advancement only.                                                       | L     | [ChessClock paths near 1361786] |

### Ticks and turns

| Finding                                                                                                                                | Level | Source                                    |
| -------------------------------------------------------------------------------------------------------------------------------------- | ----- | ----------------------------------------- |
| A tick resolves moves at `turn = t`, then increments `turn`, then applies generation. The update then reports `t + 1`.                 | L     | `update` [1216176]                        |
| So an update reporting `t` means the next tick resolves at `t`, and its effects appear in the update reporting `t + 1`.                | L     | deduction                                 |
| Generals and cities gain 1 army when the new turn is divisible by `RECRUIT_RATE = 2`, and all land when divisible by `FARM_RATE = 50`. | L     | [1195886], [1195901]                      |
| The tick period is `500 ms / game_speed`, and `turn` counts ticks at any speed.                                                        | L     | [1271257]                                 |
| The first update follows a 1500 ms pre-game delay.                                                                                     | L     | `PRE_GAME_START_DELAY` [1251031]          |
| Chess-clock games send same-turn heartbeat updates and advance by different rules.                                                     | L     | `sendChessClockHeartbeatUpdate` [1361786] |

The solver's `Path.start_turn` is a server `turn` value.
`turns[0] = lands[1] * generation_turns` matches the generation rule: at turn
`2k` the general holds `1 + k` armies, enough to capture `k` empty tiles (L,
deduction).

### Ending a match

| Finding                                                                                                                                                                                                                                                                                                                                                                                                                            | Level | Source                      |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | --------------------------- |
| `surrender` kills the player, emits `game_lost {surrender: true}` to that player, then pushes state.                                                                                                                                                                                                                                                                                                                               | L     | `handleSurrender` [1340479] |
| `game_over` is emitted only when the whole game ends, so with other players still alive a surrendered player receives `game_lost` but no `game_over`.                                                                                                                                                                                                                                                                              | L     | [1353707]                   |
| At `turn == 0` the surrender path does nothing.                                                                                                                                                                                                                                                                                                                                                                                    | L     | `_goAFK` [1339500]          |
| `surrender` from an already dead player is ignored.                                                                                                                                                                                                                                                                                                                                                                                | L     | [1340479]                   |
| Leaving or quitting broadcasts `disable_rematch`, which can reach us **before** `game_over`.                                                                                                                                                                                                                                                                                                                                       | L     | `handleLeave` [1342495]     |
| The browser emits `leave_game` when it exits a finished game.                                                                                                                                                                                                                                                                                                                                                                      | B     | [113659]                    |
| A `game_start` payload does not name its queue. So before joining the next queue, Session sends `leave_game` followed by acknowledged `get_username`, and discards everything before the acknowledgement. This relies on server packets staying in order and on the server handling requests in order. The acknowledgement was seen working live (O). Whether a rematch can still start after `leave_game` was handled is unknown. | U     | `rematch` scenario          |
| A surrender while other players remain has not been observed in production.                                                                                                                                                                                                                                                                                                                                                        | U     |                             |

### Connection

| Finding                                                                                                               | Level | Source                                                              |
| --------------------------------------------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------- |
| A user ID without a username gets `gio_error "You must choose a username to continue playing!"` after `join_private`. | **O** | probe, 2026-11-07. The only live observation so far.                |
| The browser wraps attack emits in a depth counter that does not change the arguments.                                 | B     | `iA` [82092], [115354]                                              |
| Whether a reconnect can resume a game and its queue.                                                                  | U     | Session treats every disconnect as fatal, so this is not relied on. |

### Latency

These are separate quantities, and only the first two are in our control: local
reaction (event receipt to `attack().await` returning), `sioc` channel
backpressure, one-way network delay, and the server tick phase. The reported
~250 ms domain ping has not been measured in a game (U). A move emitted right
after an update has about one tick minus the one-way delays to arrive. Nothing
here proves it arrives in time, so a missed tick must be survivable.

## Contract for Opening

The goal is that a wrong guess about the queue costs a match, never a false
success.

1. **One move in flight.** Submit the next move only after the previous one is
   observed. Opening never depends on the server buffering moves, so queue
   capacity, expiry and preemption do not matter.
2. **When to submit.** On an update reporting `turn >= start_turn`, once the
   source is owned and holds enough armies. A path's first move waits until the
   general holds 1 army plus 1 per tile the path still has to capture, because a
   late departure leaves fewer armies for the next path than the schedule
   assumed. On time, this never delays a path. A later move in a path needs a
   source with at least 2 armies. Without them, the plan stops if the path still
   has tiles to capture. Otherwise the rest of the path only crosses owned land,
   so it is skipped, because it could not run and gains nothing.
3. **Indices.** Each planned move's position, from 1, is its index in that
   match, so indices only increase and are never reused. A skipped move leaves a
   gap, which the simulation's maximum rule allows (L). `attackIndex` values
   that are absent, `-1`, repeated or smaller than the in-flight index count as
   no progress. A value above it matches nothing submitted, so the plan stops.
4. **What proves a move.** All in one update: `attackIndex` equals the in-flight
   index, the destination is owned in the reconstructed map, and its army grew
   since submission. A valid move always adds armies to its destination, and a
   discarded one does not, so this also covers a move onto land already owned,
   whose ownership proves nothing.
5. **Deviation.** Stop the plan, **never replay**, and return
   `MatchError::Abandon` when: the index matches but the destination gained
   nothing; the index does not advance within a bounded number of ticks; the
   source is lost; or a path runs short of armies mid-way.
6. **Opening completion.** Every planned move is proved. A locally accepted send
   never counts. Only then submit one normal surrender.
7. **After surrender.** Keep reading until `game_over`. If `game_lost` arrived
   but `game_over` does not within a bound, return `MatchError::Eliminated`. If
   neither arrives, return `MatchError::Abandon`.
8. **Unsupported games.** Chess-clock games advance by different rules, and
   spectators cannot move. Return `MatchError::Abandon` instead of guessing.
9. **Urgent moves.** Opening has none. With one move in flight there is nothing
   queued to preempt, so `clear_moves` and `undo_move` are unused.
10. **Disconnect.** Fatal, through `next_event`. Nothing is resubmitted.

### Residual risks

- If production `attackIndex` means "received" rather than "taken from the
  queue", army gain still keeps a move from counting early, but a slow move
  could be seen as failed and abandon the plan.
- Army gain can be faked by land generation on the exact tick a move onto owned
  land is discarded, every `FARM_RATE` turns. That needs an enemy to have
  emptied the source in the same tick, so it is accepted.
- If production never sends a usable `attackIndex`, every plan stops at its
  first move and abandons. That is a lost opening rather than a false success,
  and the first live run would show it at once.
- A surrendered player in a game with other players may see the `Eliminated`
  path instead of `game_over` (L).

## How to finish verification

[`crates/shuai/examples/attack_probe.rs`](../../crates/shuai/examples/attack_probe.rs)
plays nine short scripted games in new private rooms and records every packet.
It needs existing accounts with usernames, because the server requires them, and
it has **not yet completed a game**, so it may need fixes on its first run:

```sh
GENERALS_IO_PROBE_USER_IDS=<id1>,<id2>,<id3> cargo run --example attack_probe -- <out-dir>
```

| Scenario        | Question it settles                                                  |
| --------------- | -------------------------------------------------------------------- |
| `fifo`          | Order, one move per tick, and the index progression                  |
| `invalid`       | Index advance on a discarded move, and a valid move in the same tick |
| `clear`, `undo` | What each command cancels, and index behavior afterwards             |
| `gaps`          | Whether the index is a maximum or the latest value                   |
| `early`         | Discarding when the general has too few armies                       |
| `rematch`       | Rematch handoff and `disable_rematch` ordering                       |
| `three`         | Surrender without `game_over`                                        |
| `speed`         | Update cadence at double speed                                       |

## Fixtures

[`crates/shuai/src/opening/fixtures/`](../../crates/shuai/src/opening/fixtures)
holds replayable traces labelled by evidence level. All are derived from the L
findings above. **None is a production capture.** Add captures from the probe
alongside them with evidence level O.
