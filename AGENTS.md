## Project

A generals.io bot, unaffiliated with generals.io.

- `shuai-client` - typed Socket.IO client for the generals.io protocol
- `shuai` - game state, the Session lifecycle, the Opening bot, and the binary
- `shuai-cpsat` - synchronous opening formulations

## Build & Test

Use the root `justfile` for recurring commands. Run `just fmt` before
committing, `just check` for the fast gate, and `just ci` for complete
validation, including all workspace targets, doctests, and API documentation.
Tests run through `cargo-nextest`. Use `just test-targets --stress-count 10` to
look for flakes, and do not enable retries, because they would hide them.

## Running

```bash
GENERALS_IO_USER_ID=<id> RUST_LOG=info cargo run --bin shuai
```

`shuai=trace` shows bot decisions, `sioc=trace` shows wire packets.

`just live-match [matches]` plays the real bot against an idle bot on the
production server, and prints the replay URLs. It needs
`GENERALS_IO_LIVE_USER_IDS=<id1>,<id2>`, with two accounts that have usernames.
It is not part of `just ci`. A bot is a gameplay function, so another bot is
another argument to `LiveRoom::run_bot` in `examples/live_match/room.rs`.

See `docs/architecture.md` for ownership and `docs/protocol/attack-queue.md` for
the evidence behind attack handling.

## Dependencies

Use `cargo add -p <member> <crate>`. All deps go through
`[workspace.dependencies]`.

## Code Style

Re-export public surface through `prelude`, excluding `Result` and `Error`
aliases. Import those by explicit path.

Descriptive names over abbreviations: `packet` and `event`, not `pkt` and `evt`.
Single letters are fine for short-lived bindings.

## Writing Style

Minimal and concise. Plain English transitions over punctuation. ASCII
punctuation only.

## Docstrings

Third-person declarative. Doctests must compile and run.

## Comments

Write code that does not need them. When one is warranted, explain why, not
what.
