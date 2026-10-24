# Shuai architecture

This document records the agreed design direction for [Session lifecycle (#1)](https://github.com/Stanley5249/shuai/issues/1) and the [concrete Opening implementation (#5)](https://github.com/Stanley5249/shuai/issues/5). It is a **composition and ownership contract**, not a mandate to build a framework.

## Decision: no universal Bot abstraction

**Do not introduce** a project-wide `Bot` trait, `BotGame` context, mandatory `GameCore`, `GameRunner` / `GameDriver` / `GameActor`, generic `GameInput` / `GameEffect`, universal attack coordinator, general CPU-job scheduler, or mandatory Sans-I/O implementation in anticipation of hypothetical bots.

The concrete Opening bot is ordinary async Rust code. Another NN, search, RL, data-collection, or training implementation may compose the same building blocks differently. It may implement its *own* state machine, Sans-I/O core, coordinator or traits if those genuinely simplify **that implementation**.

Do not silently reintroduce a universal Bot framework in unrelated work. Reconsider only in a new, explicit design review supported by multiple real implementations sharing a stable contract and a demonstrated reduction in complexity.

## Existing building blocks

- **`shuai-client` protocol:** `GeneralsIoEvent` for typed server events; `GameStartData`, `GameUpdateData`, `GameOver`, `GameWon`, `GameLost`, queue/rematch/chat/error payloads; `GeneralsIoSender` for async sends and supported application acknowledgements. Reuse these types rather than duplicating packet schemas.
- **`shuai::game::GameState`:** optional reusable validated reconstruction of game start metadata, map diffs and latest `Observation` via `start(Box<GameStartData>)` and `update(GameUpdateData)`. Each concrete gameplay implementation may own a fresh state instance. A small state-update helper is acceptable if it removes real repetition; do not mandate one for every bot.
- **Session lifecycle:** connection, room/matchmaking policy, rematch/requeue and Ctrl+C/cleanup. Room policy can remain plain functions and `GameMode`, not a trait.
- **Opening implementation:** CP-SAT solving, deadlines, multi-tick plans, attack-progress reconciliation and its normal surrender condition belong solely to #5.

## Ownership, event delivery and phase handoff

```text
sioc Client/Manager
      |
Session -- owns one SocketReceiver and the long-lived GeneralsIoSender
  |
  +-- queue(...)         -- borrows &mut SocketReceiver; returns GameStartData
  +-- play_opening(...)  -- borrows &mut SocketReceiver; handles the active match
  +-- rematch(...)       -- borrows &mut SocketReceiver; returns new GameStartData or requeue
```

These names illustrate **ordinary async operations**, not required public function signatures or a common phase trait.

1. Only **Session owns the receiver**; one active operation borrows it at a time. Do not add a permanent event-forwarding channel or a second receiver consumer by default.
2. Pass an **already received `GameStartData`** directly to gameplay. A new `GameStart` received during rematch must be carried into the next gameplay call, not consumed twice or lost.
3. The active gameplay operation consumes the **complete relevant event stream**, not just `GameUpdate`. Apply each map update in receive order, including same-turn heartbeats; record/handle `GameWon`, `GameLost`, chat and warnings when relevant. `GameOver` is the normal terminal handoff event for gameplay. A normal return does **not** mean a planned opening finished; it means the match ended.
4. Keep unknown event names observable and distinguish them from malformed known payloads; the current typed router rejects unknown names. `SocketReceiver::listen()` also skips raw connect/disconnect signals: use raw signals where necessary to protect lifecycle correctness.
5. A phase borrows `GeneralsIoSender` for outgoing requests. `sioc` may apply bounded-channel backpressure. Success from fire-and-forget `attack().await` indicates local submission, **not** server execution; an actual application acknowledgement exists only for events whose API supplies one.
6. Release match-specific tasks, senders and state before starting another match. Never reuse a stale plan after GameOver, reconnect or loss of trustworthy incremental state.

## Gameplay exit and shutdown contract

The concrete Opening integration can use a narrow signature such as:

```rust,ignore
async fn play_opening(
    start: GameStartData,
    rx: &mut SocketReceiver,
    tx: &GeneralsIoSender,
    shutdown: CancellationToken,
) -> Result<()>; // sketch; CancellationToken requires a cancellation facility
```

The signal type and error type may be chosen during implementation; **do not introduce a universal gameplay interface** to match the sketch.

| Condition | Opening responsibility | Session responsibility |
| --- | --- | --- |
| Observed `GameOver` | Stop work, cancel/join solver if active, return normally | Attempt rematch; on timeout or `DisableRematch` requeue |
| Opening plan complete | Confirm server-observed progress, issue normal surrender when safe, **continue consuming events until `GameOver`** | None yet |
| Recoverable match-local error | Recover in place if state is trustworthy; otherwise clean up and report context | Requeue **only after a safe server-supported exit**; if unable to establish safety, terminate instead |
| Protocol/invariant error, lost authoritative diff continuity | Report fatal failure; stop unsafe commands/jobs | Terminate session, attempt cleanup, report error |
| Ctrl+C | Stop new attacks/computations; request cooperative solver cancellation; reap/join work; return control (or error) | Stop queue/requeue; attempt best-effort surrender if still in game; disconnect, release sender clones and join client |

**Priority:** once shutdown is requested, Session takes the shutdown path even if the gameplay future returns `Ok(())`. An ordinary `Ok(())` without shutdown is reserved for an observed `GameOver`. This avoids inventing a new exit enum prematurely. If the implementation truly needs richer *normal* exit information, introduce only a small local enum with a concrete consumer.

Do **not** implement Ctrl+C by simply dropping `play_opening` in an outer `select!`: aborting an async wrapper does not necessarily stop native CP-SAT running in a blocking worker. Deliver a cooperative shutdown signal, request the solver's native StopHandle, and observe worker termination. Use a bounded cleanup/grace policy and report an uncooperative worker; do not claim hard native-thread cancellation guarantees. The session is responsible for completing cleanup even on early errors (not just the happy `Client::join()` path).

`GameWon`/`GameLost` describe a result and are not substitutes for `GameOver` unless verified protocol evidence establishes another terminal contract.

## Room lifecycle

- **Fixed private room:** preserve the configured ID when requeueing.
- **Random private room:** generate a new room ID **on each requeue** (including after a failed rematch), **not** on an accepted rematch.
- **Rematch:** wait for rematch; on timeout or `DisableRematch`, requeue. Carry an early `GameStart` directly forward.
- Preserve currently supported 1v1 matchmaking behavior. Do not create a `MatchmakingPolicy` trait until multiple concrete implementations show that it is useful.

## Opening-specific constraints (#5)

- Start `GameState` with `GameStartData`, then wait for enough valid `GameUpdateData` to construct a solver problem. Never solve an absent map.
- Keep the existing CP-SAT opening formulation/reconstruction unchanged; accept `FEASIBLE` and `OPTIMAL`, and handle no-solution outcomes explicitly.
- Run expensive computation off the event-receive path, with bounded worker count, cooperative cancellation and correct match lifetime. A long-running solve must not block urgent event reception.
- Use a **bot-local** deadline-driven loop as needed (incoming events, solver completion, pending I/O, deadline and shutdown); there is no required shared Game loop. A typical ~500 ms server update is not an outbound timer. Urgent local reaction is a measured **≤300 ms target**, not a hard network/server guarantee.
- Server attack queues, `attack_index`, half-turn/time mapping, clear/undo ordering and completion evidence remain an explicit [protocol research dependency (#2)](https://github.com/Stanley5249/shuai/issues/2). Never treat `emit` success as move execution or assume queue preemption without evidence.
- Do not silently retry a command after cancellation when it is uncertain whether its local submission completed.

## Naming and scope

Names should reflect existing responsibilities, not speculative technologies. Prefer precise public names (`Session`, `GameState`, `GeneralsIoSender`, `GameStartData`, `Opening`) and narrow-module short names where meaning is obvious. Do not export vague top-level `Event`, `Input`, `Action`, `Effect`, `Job`, or `Worker` just to coordinate abstract frameworks. Distinguish **matchmaking queue** from **attack queue**, **local send acceptance** from **server-observed execution**, and **opening completion** from **match termination**.

See [#1](https://github.com/Stanley5249/shuai/issues/1) for reusable lifecycle implementation, [#5](https://github.com/Stanley5249/shuai/issues/5) for the concrete Opening integration and [#2](https://github.com/Stanley5249/shuai/issues/2) for protocol evidence. This document does not imply code has been implemented.
