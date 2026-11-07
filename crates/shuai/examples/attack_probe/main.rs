//! Records live attack-queue behavior for the protocol note in `docs/protocol/`.
//!
//! Each scenario plays one short game in a new private room containing only this
//! program's clients, and ends it by surrendering. Every inbound packet and outbound
//! command is written to `<out>/<scenario>.jsonl`. User IDs are never recorded.
//!
//! The server requires a username to play, so this uses the registered accounts in
//! `user/accounts.db`. Most scenarios need two, and `three` needs three.
//!
//! ```sh
//! just probe <out-dir> [scenario...]
//! ```
//!
//! Passing `check-accounts` instead of an output directory reports which registered
//! accounts the server knows, with one read-only request each.
//!
//! Passing `register <username>` registers an unregistered name from the store, once. It
//! saves and backs up the new ID first, and a later attempt reuses it, so a lost answer
//! cannot lose a username, which is permanent.
//!
//! Passing `import-accounts <json>` fills an empty store from a JSON file, and
//! `compare-accounts <json>` reports whether the store matches one, printing no IDs.

use std::path::PathBuf;
use std::time::Duration;

use bytestring::ByteString;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use shuai_client::prelude::*;
use sioc::prelude::*;
use url::Url;

#[path = "../support/accounts.rs"]
mod accounts;
mod probe;
mod scenarios;

use crate::accounts::AccountStore;
use crate::scenarios::scenario;

const ENDPOINT: &str = "https://ws.generals.io";
const START_TURN: i64 = 20;
const WAIT: Duration = Duration::from_secs(60);
const REGISTER_WAIT: Duration = Duration::from_secs(5);
const SCENARIOS: &[&str] = &[
    "fifo",
    "invalid",
    "clear",
    "undo",
    "gaps",
    "early",
    "rematch",
    "rematch_late",
    "leave_after_game",
    "disconnect_after_game",
    "leave_during_game",
    "three",
    "speed",
];

async fn check_accounts() -> Result<()> {
    let client = ClientBuilder::new(Url::parse(ENDPOINT).into_diagnostic()?)
        .transport(TransportStrategy::WebSocket)
        .open()
        .into_diagnostic()?;
    let (socket_tx, _rx) = client
        .connect(ByteString::from_static("/"))
        .await
        .into_diagnostic()?;
    let tx = GeneralsIoSender::new(socket_tx);
    let entries = AccountStore::open().await?.entries().await?;
    for entry in entries {
        if let Some(user_id) = entry.user_id {
            let known = tx.get_username(user_id).await.into_diagnostic()?;
            println!("{}: server username: {known:?}", entry.username);
        }
    }
    tx.disconnect().await;
    Ok(())
}

/// Registers a listed name. The ID is saved and backed up before anything remote, and
/// reused by any later attempt, so a lost answer cannot strand the permanent username on
/// a forgotten ID.
async fn register(username: String) -> Result<()> {
    let store = AccountStore::open().await?;
    let user_id = store.reserve_user_id(&username).await?;

    let client = ClientBuilder::new(Url::parse(ENDPOINT).into_diagnostic()?)
        .transport(TransportStrategy::WebSocket)
        .open()
        .into_diagnostic()?;
    let (socket_tx, mut rx) = client
        .connect(ByteString::from_static("/"))
        .await
        .into_diagnostic()?;
    let tx = GeneralsIoSender::new(socket_tx);
    let existing = tx.get_username(user_id.clone()).await.into_diagnostic()?;
    if existing.is_none() {
        tx.set_username(user_id.clone(), username.clone())
            .await
            .into_diagnostic()?;
        // The answer is an `error_set_username` event, whose empty message means success.
        let answer = tokio::time::timeout(REGISTER_WAIT, async {
            loop {
                match next_signal(&mut rx).await {
                    GeneralsIoSignal::Event(event) => {
                        let name = event.name();
                        if name == "error_set_username" || name == "gio_error" {
                            return event.into_result().map(|_| ());
                        }
                    }
                    GeneralsIoSignal::Disconnected => return Err(miette!("connection closed")),
                    _ => {}
                }
            }
        })
        .await;
        if let Ok(answer) = answer {
            answer.wrap_err("registration rejected")?;
        }
    }
    let confirmed = tx.get_username(user_id.clone()).await.into_diagnostic();
    tx.disconnect().await;
    let confirmed = confirmed?;
    if confirmed.as_deref() != Some(username.as_str()) {
        return Err(miette!("server reports the username {confirmed:?}"));
    }
    store.confirm_registration(&username, &user_id).await?;
    println!("registered {username:?}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let first = arguments.next().ok_or_else(|| miette!("missing out dir"))?;
    if first == "check-accounts" {
        return check_accounts().await;
    }
    if first == "import-accounts" {
        let path = PathBuf::from(arguments.next().ok_or_else(|| miette!("missing file"))?);
        let entries = accounts::read_entries_file(&path)?;
        AccountStore::open().await?.import(&entries).await?;
        println!("imported {} entries", entries.len());
        return Ok(());
    }
    if first == "compare-accounts" {
        let path = PathBuf::from(arguments.next().ok_or_else(|| miette!("missing file"))?);
        let expected = accounts::read_entries_file(&path)?;
        let stored = AccountStore::open().await?.entries().await?;
        println!(
            "{} stored, {} in the file, identical: {}",
            stored.len(),
            expected.len(),
            stored == expected
        );
        return Ok(());
    }
    if first == "register" {
        let username = arguments
            .next()
            .ok_or_else(|| miette!("missing username"))?;
        return register(username).await;
    }
    let out = PathBuf::from(first);
    std::fs::create_dir_all(&out).into_diagnostic()?;
    let selected: Vec<String> = arguments.collect();
    if let Some(unknown) = selected.iter().find(|s| !SCENARIOS.contains(&s.as_str())) {
        return Err(miette!("unknown scenario {unknown:?}"));
    }
    let mut failed = 0;
    for name in SCENARIOS {
        if !selected.is_empty() && !selected.iter().any(|s| s == name) {
            continue;
        }
        println!("running {name}");
        if let Err(error) = scenario(name, &out.join(format!("{name}.jsonl"))).await {
            println!("{name} failed: {error:?}");
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(miette!("{failed} scenarios failed"));
    }
    Ok(())
}
