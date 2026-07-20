# ♟️ Shuai

A [generals.io](https://generals.io) bot, named after 帥, the general in Chinese Chess (象棋).

- `shuai-client` - typed Socket.IO client for the generals.io protocol
- `shuai` - the bot

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
