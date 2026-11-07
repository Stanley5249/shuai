//! Plays one room of matches on the production server, between the real bot and an idle
//! opponent, and prints the replay URLs.
//!
//! It uses the first two registered accounts in `user/accounts.db`. Their IDs are never
//! printed.
//!
//! The bots rejoin one room between matches, unless `--new-room` gives each match its
//! own.
//!
//! ```sh
//! RUST_LOG=shuai=info cargo run --example live_match -- [matches] [--new-room]
//! ```

#[path = "../support/accounts.rs"]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "this example only reads the accounts")
)]
mod accounts;
mod room;
#[cfg(test)]
#[path = "../../src/test_support.rs"]
#[expect(
    dead_code,
    reason = "these tests use only part of the library's test support"
)]
mod test_support;

use std::num::NonZeroU32;
use std::time::Duration;

use miette::{IntoDiagnostic, Result, WrapErr, miette};
use shuai::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::room::{Account, LiveRoom, play_idle, replay_url, verified_replays};

const ENDPOINT: &str = "https://ws.generals.io";
const NEW_ROOM_FLAG: &str = "--new-room";
/// A match takes under a minute, including the solve and the queue.
const TIME_LIMIT_PER_MATCH: Duration = Duration::from_secs(90);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let (flags, values): (Vec<String>, Vec<String>) = std::env::args()
        .skip(1)
        .partition(|arg| arg.starts_with("--"));
    let new_room_per_match = flags.iter().any(|flag| flag == NEW_ROOM_FLAG);
    if let Some(unknown) = flags.iter().find(|flag| *flag != NEW_ROOM_FLAG) {
        return Err(miette!("unknown flag {unknown}"));
    }
    let matches = values
        .first()
        .map_or(Ok(NonZeroU32::MIN), |value| value.parse())
        .into_diagnostic()
        .wrap_err("the match count must be a positive number")?;
    let endpoint = Url::parse(ENDPOINT).into_diagnostic()?;
    let mut user_ids = accounts::registered_user_ids().await?.into_iter();
    let (Some(first), Some(second)) = (user_ids.next(), user_ids.next()) else {
        return Err(miette!("user/accounts.db needs two registered accounts"));
    };
    let bot = Account {
        endpoint: endpoint.clone(),
        user_id: first,
    };
    let opponent = Account {
        endpoint,
        user_id: second,
    };

    let ctrl_c = CancellationToken::new();
    tokio::spawn(cancel_on_ctrl_c(ctrl_c.clone()));
    let replays = if new_room_per_match {
        let mut replays = Vec::new();
        for _ in 0..matches.get() {
            replays.extend(play_room(NonZeroU32::MIN, &bot, &opponent, &ctrl_c).await?);
        }
        replays
    } else {
        play_room(matches, &bot, &opponent, &ctrl_c).await?
    };
    for replay_id in replays {
        println!("{}", replay_url(&replay_id));
    }
    Ok(())
}

/// Plays `matches` in one new room, and returns the verified replay IDs.
async fn play_room(
    matches: NonZeroU32,
    bot: &Account,
    opponent: &Account,
    ctrl_c: &CancellationToken,
) -> Result<Vec<String>> {
    let room = LiveRoom::new(matches, ctrl_c);
    let (bot_run, opponent_run) = room
        .with_time_limit(TIME_LIMIT_PER_MATCH * matches.get(), async {
            tokio::join!(
                room.run_bot(bot, play_opening),
                room.run_bot(opponent, play_idle),
            )
        })
        .await;
    verified_replays(vec![bot_run, opponent_run], matches.get())
}
