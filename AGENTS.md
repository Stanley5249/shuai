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

`GENERALS_IO_ROOM=<id>` keeps the bot in one private room. Without it, every
match gets a new random room.

`just live-match [matches] [--new-room]` plays the real bot against an idle bot
on the production server, and prints the replay URLs. It is not part of
`just ci`. A bot is a gameplay function, so another bot is another argument to
`LiveRoom::run_bot` in `examples/live_match/room.rs`.

Live test accounts are in `user/accounts.db`, a Turso database under the
directory Git ignores. Never commit anything under `user/` or print a `user_id`.
Only one process can open the database at a time, so close other tools first. An
entry with a `user_id` is registered.
`cargo run --example attack_probe -- register <name>` registers a listed name,
saving and backing up its ID before anything remote. Run it again after a
failure: it reuses the saved ID.

### Recovering accounts

Each change writes a JSON snapshot to `user/backups/`. They are local, so they
do not protect against losing the disk. Copy `user/` elsewhere if that matters.

1. Delete nothing. Move `user/accounts.db` and `user/accounts.db-wal` to a new
   `user/damaged-<date>/`.
2. Run `cargo run --example attack_probe -- import-accounts <newest snapshot>`.
3. Run `cargo run --example attack_probe -- check-accounts`.
4. Before registering anything, compare with the other snapshots, because a new
   ID cannot recover a name already registered. If an ID exists only in another
   snapshot, combine them by hand into one JSON file in the same format, keeping
   every ID and resolving any conflict. Move the restored database aside too,
   and import the combined file, because import refuses a store that has
   entries.

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
