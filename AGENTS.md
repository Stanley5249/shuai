## Project

A generals.io bot, unaffiliated with generals.io.

- `shuai-client` - typed Socket.IO client for the generals.io protocol
- `shuai` - game state building blocks and the bot binary
- `shuai-cpsat` - synchronous opening formulations

## Build & Test

Use the root `justfile` for recurring commands. Run `just fmt` before
committing, `just check` for the fast gate, and `just ci` for complete
validation, including all workspace targets, doctests, and API documentation.

## Running

```bash
GENERALS_IO_USER_ID=<id> RUST_LOG=info cargo run --bin shuai
```

`shuai=trace` shows bot decisions, `sioc=trace` shows wire packets.

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
