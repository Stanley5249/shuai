# ♟️ Shuai

A [generals.io](https://generals.io) bot, named after 帥, the general in Chinese
Chess (象棋).

- `shuai-client` - typed Socket.IO client for the generals.io protocol
- `shuai` - game state building blocks and the bot
- `shuai-cpsat` - synchronous opening formulations

Game data assumptions are in [game-spec.md](docs/game-spec.md). The opening
model and offline examples are in
[formulation/opening.md](docs/formulation/opening.md). Game state processing and
opening solving are independent building blocks. Bot orchestration uses the
existing client workflow; state-to-solver integration is a separate step.

## Requirements

Install [Rust through rustup](https://rust-lang.org/tools/install/),
[Just](https://just.systems/man/en/installation.html),
[Tombi](https://tombi-toml.github.io/tombi/docs/installation/), and
[Bun](https://bun.sh/docs/installation). Windows recipes use
[PowerShell 7](https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows).
Bun runs oxfmt for document and configuration formatting.

## Commands

The [justfile](justfile) defines recurring tasks. Run `just` to list them,
`just fmt` to format, `just check` for the fast gate, and `just ci` for complete
validation. `just test` includes target tests and doctests.

## Running

`GENERALS_IO_USER_ID` is your generals.io user ID. In browser developer tools,
open the Network tab (Fetch/XHR or Socket section) and find Socket.IO packets
like `42["stars_and_rank","your-user-id"]`.

```bash
GENERALS_IO_USER_ID=your-user-id RUST_LOG=info cargo run --bin shuai
```

```powershell
$env:GENERALS_IO_USER_ID = "your-user-id"; $env:RUST_LOG = "info"; cargo run --bin shuai
```
