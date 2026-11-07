# Shuai architecture

This document records the agreed design direction for
[Session lifecycle (#1)](https://github.com/Stanley5249/shuai/issues/1) and the
[concrete Opening implementation (#5)](https://github.com/Stanley5249/shuai/issues/5).
It is a **composition and ownership contract**, not a mandate to build a
framework.

## Project identity

Shuai is a concrete generals.io bot application with reusable libraries, not a
framework. The one extension point is the gameplay function passed to
`Session::run`, and nothing else needs one.

| Crate          | Role                                                                            | Must not know about                  |
| -------------- | ------------------------------------------------------------------------------- | ------------------------------------ |
| `shuai-client` | Protocol: typed events, the sender, decoding, and what the server's events mean | What the application does about them |
| `shuai-cpsat`  | Solver: graphs and schedules                                                    | Game state, networking               |
| `shuai`        | Application: game state, Session policy, Opening, the binary                    |                                      |

`shuai` has a library target so that its binary, examples and tests can share
code, not to offer a public API. Its default `opening` feature holds the Opening
bot, and the only dependency on `shuai-cpsat` and the OR-Tools native libraries,
so that a bot or tool without a solver, such as `attack_probe`, links none of
them. `just lint-without-solver` keeps that build working. For example,
`shuai-client` reports that a known event failed to decode, and
`session::next_event` decides whether that stops the session.

## Decision: no universal Bot abstraction

**Do not introduce** a project-wide `Bot` trait, `BotGame` context, mandatory
`GameCore`, `GameRunner` / `GameDriver` / `GameActor`, generic `GameInput` /
`GameEffect`, universal attack coordinator, general CPU-job scheduler, or
mandatory Sans-I/O implementation in anticipation of hypothetical bots.

The concrete Opening bot is ordinary async Rust code. Another NN, search, RL,
data-collection, or training implementation may compose the same building blocks
differently. It may implement its _own_ state machine, Sans-I/O core,
coordinator or traits if those genuinely simplify **that implementation**.

Do not silently reintroduce a universal Bot framework in unrelated work.
Reconsider only in a new, explicit design review supported by multiple real
implementations sharing a stable contract and a demonstrated reduction in
complexity.

## Existing building blocks

- **`shuai-client` protocol:** `GeneralsIoEvent` for typed server events;
  `GameStartData`, `GameUpdateData`, `GameOver`, `GameWon`, `GameLost`,
  queue/rematch/chat/error payloads; `GeneralsIoSender` for async sends and
  supported application acknowledgements. Reuse these types rather than
  duplicating packet schemas.
- **`shuai::game::GameState`:** optional reusable validated reconstruction of
  game start metadata, map diffs and latest `Observation` via
  `start(Box<GameStartData>)` and `update(GameUpdateData)`. Each concrete
  gameplay implementation may own a fresh state instance. A small state-update
  helper is acceptable if it removes real repetition; do not mandate one for
  every bot.
- **Session lifecycle:** connection, room/matchmaking policy, rematch/requeue
  and Ctrl+C/cleanup. Room policy can remain plain functions and `QueueMode`,
  not a trait.
- **Opening implementation:** CP-SAT solving, deadlines, multi-tick plans,
  attack-progress reconciliation and its normal surrender condition belong
  solely to #5.

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

These names illustrate **ordinary async operations**, not required public
function signatures or a common phase trait.

1. Only **Session owns the receiver**; one active operation borrows it at a
   time. Do not add a permanent event-forwarding channel or a second receiver
   consumer by default.
2. Pass an **already received `GameStartData`** directly to gameplay. A new
   `GameStart` received during rematch must be carried into the next gameplay
   call, not consumed twice or lost.
3. The active gameplay operation consumes the **complete relevant event
   stream**, not just `GameUpdate`. Apply each map update in receive order,
   including same-turn heartbeats; record/handle `GameWon`, `GameLost`, chat and
   warnings when relevant. `GameOver` is the normal terminal handoff event for
   gameplay. A normal return does **not** mean a planned opening finished; it
   means the match ended.
4. Keep unknown event names observable and distinguish them from malformed known
   payloads; the current typed router rejects unknown names.
   `SocketReceiver::listen()` also skips raw connect/disconnect signals: use raw
   signals where necessary to protect lifecycle correctness.
5. A phase borrows `GeneralsIoSender` for outgoing requests. `sioc` may apply
   bounded-channel backpressure. Success from fire-and-forget `attack().await`
   indicates local submission, **not** server execution; an actual application
   acknowledgement exists only for events whose API supplies one.
6. Release match-specific tasks, senders and state before starting another
   match. Never reuse a stale plan after GameOver, reconnect or loss of
   trustworthy incremental state.

## Gameplay exit and shutdown contract

A gameplay function has one narrow signature, and `MatchContext` holds what
Session lends it for one match: the `GameStartData`, the receiver, the sender,
the match's `SurrenderOnce` and the shutdown token.

```rust,ignore
async fn play_opening(context: MatchContext<'_>) -> Result<(), MatchError>;
```

`Session::run` takes this as a plain async function argument, so tests can pass
a stub without CP-SAT. `MatchError` tells Session what the server may still
believe: `Abandon` means the match may be active but can be left, so Session
surrenders and requeues only after the server confirms the exit. `Eliminated`
means `GameLost` arrived without `GameOver`, so Session leaves and requeues.
`Fatal` stops the session, and any `?` on a `miette::Report` becomes `Fatal`, so
an unclassified failure can never requeue. `SurrenderOnce` allows one surrender
per match, because gameplay, the `Abandon` exit and cleanup can each want one.
Every phase reads events through `session::next_event`, which holds the single
policy for unknown events, malformed events and disconnects.

**Do not introduce a universal gameplay interface** around this signature.

| Condition                                                    | Opening responsibility                                                                                             | Session responsibility                                                                                                |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| Observed `GameOver`                                          | Stop work, cancel/join solver if active, return normally                                                           | In `OneVsOne`, attempt rematch, then requeue on timeout or `DisableRematch`. In a private room, leave and rejoin      |
| Opening plan complete                                        | Confirm server-observed progress, issue normal surrender when safe, **continue consuming events until `GameOver`** | None yet                                                                                                              |
| Recoverable match-local error                                | Recover in place if state is trustworthy; otherwise clean up and report context                                    | Requeue **only after a safe server-supported exit**; if unable to establish safety, terminate instead                 |
| Protocol/invariant error, lost authoritative diff continuity | Report fatal failure; stop unsafe commands/jobs                                                                    | Terminate session, attempt cleanup, report error                                                                      |
| Ctrl+C                                                       | Stop new attacks/computations; request cooperative solver cancellation; reap/join work; return control (or error)  | Stop queue/requeue; attempt best-effort surrender if still in game; disconnect, release sender clones and join client |

**Priority:** once shutdown is requested, Session takes the shutdown path even
if the gameplay future returns `Ok(())`. An ordinary `Ok(())` without shutdown
is reserved for an observed `GameOver`. This avoids inventing a new exit enum
prematurely. If the implementation truly needs richer _normal_ exit information,
introduce only a small local enum with a concrete consumer.

Do **not** implement Ctrl+C by simply dropping `play_opening` in an outer
`select!`: aborting an async wrapper does not necessarily stop native CP-SAT
running in a blocking worker. Deliver a cooperative shutdown signal, request the
solver's native StopHandle, and observe worker termination. Use a bounded
cleanup/grace policy, and treat an uncooperative worker as fatal so that no
later match overlaps it; do not claim hard native-thread cancellation
guarantees. The session is responsible for completing cleanup even on early
errors (not just the happy `Client::join()` path).

`GameOver` ends the match, and `GameWon`/`GameLost` are notifications before it.
`GameLost` does confirm the local player's elimination, so with other players
still alive it allows leaving and requeueing after a deadline, but never a
rematch.

## Room lifecycle

- **Fixed private room (`QueueMode::FixedPrivate`):** rejoin the same ID after
  every match. This is what the browser's Play Again does, and it is how players
  in a custom room play again.
- **Random private room (`QueueMode::RandomPrivate`):** a new ID for every
  match.
- **Rematch:** only the 1v1 queue votes. It waits for the rematch, and on
  timeout or `DisableRematch` requeues, carrying an early `GameStart` forward. A
  custom game's browser UI offers no rematch, and a private room did not answer
  votes in live tests, so private rooms never vote.
- **Leaving:** before joining the next queue, Session sends `leave_game` and
  then an acknowledged request, and discards everything received until that
  acknowledgement, so a late `GameStart` from the match it left can never reach
  the next gameplay call.
- Preserve currently supported 1v1 matchmaking behavior. Do not create a
  `MatchmakingPolicy` trait until multiple concrete implementations show that it
  is useful.

## Opening-specific constraints (#5)

- Start `GameState` with `GameStartData`, then wait for enough valid
  `GameUpdateData` to construct a solver problem. Never solve an absent map.
- Keep the existing CP-SAT opening formulation/reconstruction unchanged; accept
  `FEASIBLE` and `OPTIMAL`, and handle no-solution outcomes explicitly.
- Run expensive computation off the event-receive path, with bounded worker
  count, cooperative cancellation and correct match lifetime. A long-running
  solve must not block urgent event reception.
- Use a **bot-local** deadline-driven loop as needed (incoming events, solver
  completion, pending I/O, deadline and shutdown); there is no required shared
  Game loop. A typical ~500 ms server update is not an outbound timer. Urgent
  local reaction is a measured **≤300 ms target**, not a hard network/server
  guarantee.
- Server attack queues, `attack_index`, tick/time mapping, clear/undo ordering
  and completion evidence are recorded in the
  [attack queue note](protocol/attack-queue.md), which separates observed from
  inferred behavior ([#2](https://github.com/Stanley5249/shuai/issues/2)). Never
  treat `emit` success as move execution or assume queue preemption without
  evidence.
- `opening::Execution` holds those attack rules as a private, I/O-free core
  replayed from fixtures. `play_opening` is the async loop around it, driving
  receive, solver completion, the one pending send, the `GameOver` deadline and
  shutdown.
- Do not silently retry a command after cancellation when it is uncertain
  whether its local submission completed.

## Live testing

A bot is a gameplay function, so live tests compose the same `Session`.
`examples/live_match/room.rs` runs several in one private room, concurrently in
one task, with different functions as different bots, and rejects a run unless
every bot completed its matches and they agree on the replay IDs. It is example
support with one caller, not public API.

## Vocabulary

One concept has one term, and one term has one meaning.

| Term              | Meaning                                                                                       |
| ----------------- | --------------------------------------------------------------------------------------------- |
| match             | One played game, from `game_start` to `game_over`. Session and gameplay use this              |
| game              | The server's word for a match, kept in its events and in `GameState`                          |
| room              | A private game lobby, named by its `room_id`. It outlives a match                             |
| matchmaking queue | Where a session waits for a match. `QueueMode` says which                                     |
| attack queue      | The server's list of submitted moves for a player                                             |
| turn              | The server's tick counter, the `turn` of an update, at every game speed                       |
| tick              | One step of that counter, used when counting a duration                                       |
| attack index      | How far the server has taken moves from the attack queue                                      |
| acknowledgement   | A Socket.IO reply to a request. An attack index is not one                                    |
| plan              | The solver's paths for one opening                                                            |
| path              | One army's route in a plan, with a start turn                                                 |
| move              | One step of a path, from a tile to a neighbor                                                 |
| attack            | The command that submits a move                                                               |
| opening           | The Opening bot's work at a match's start: solve a plan, execute it, then surrender           |
| phase             | A part of Session's lifecycle: queue, match, rematch                                          |
| stage             | A part of one opening: `Stage`                                                                |
| surrender         | Giving up the match on the server, at most once, through `SurrenderOnce`                      |
| leave             | Sending `leave_game` and discarding what the match still sends, up to a later acknowledgement |
| abandon           | Gameplay giving up on a match it cannot continue: `MatchError::Abandon`                       |

## Naming and scope

Names should reflect existing responsibilities, not speculative technologies.
Prefer precise public names (`Session`, `GameState`, `GeneralsIoSender`,
`GameStartData`, `play_opening`) and narrow-module short names where meaning is
obvious. Do not export vague top-level `Event`, `Input`, `Action`, `Effect`,
`Job`, or `Worker` just to coordinate abstract frameworks. Distinguish
**matchmaking queue** from **attack queue**, **local send acceptance** from
**server-observed execution**, and **opening completion** from **match
termination**.

See [#1](https://github.com/Stanley5249/shuai/issues/1) for reusable lifecycle
implementation, [#5](https://github.com/Stanley5249/shuai/issues/5) for the
concrete Opening integration and
[#2](https://github.com/Stanley5249/shuai/issues/2) for protocol evidence.
