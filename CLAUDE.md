## Project

A generals.io bot, unaffiliated with generals.io.

- `shuai-client` - typed Socket.IO client for the generals.io protocol
- `shuai` - the bot binary

## Build & Test

Always use `--workspace --all-targets`. Run `cargo fmt --all` after every code
edit before committing.

```bash
cargo {test,check,clippy} --workspace --all-targets
cargo fmt --all
cargo doc --no-deps --workspace
```

## Running

```bash
GENERALS_IO_USER_ID=<id> RUST_LOG=info cargo run --bin shuai
```

`shuai=trace` shows bot decisions, `sioc=trace` shows wire packets.

## Dependencies

Use `cargo add -p <member> <crate>`. All deps go through `[workspace.dependencies]`.

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
