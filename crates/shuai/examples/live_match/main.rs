//! Plays one room of matches on the production server, between the real bot and an idle
//! opponent, and prints the replay URLs.
//!
//! The accounts must already have usernames. Their IDs are never printed.
//!
//! ```sh
//! GENERALS_IO_LIVE_USER_IDS=<id1>,<id2> RUST_LOG=shuai=info \
//!     cargo run --example live_match -- [matches]
//! ```

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
/// A match takes under a minute, including the solve and the queue.
const TIME_LIMIT_PER_MATCH: Duration = Duration::from_secs(90);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let matches = std::env::args()
        .nth(1)
        .map_or(Ok(NonZeroU32::MIN), |value| value.parse())
        .into_diagnostic()
        .wrap_err("the match count must be a positive number")?;
    let endpoint = Url::parse(ENDPOINT).into_diagnostic()?;
    let ids = std::env::var("GENERALS_IO_LIVE_USER_IDS")
        .into_diagnostic()
        .wrap_err("GENERALS_IO_LIVE_USER_IDS not set")?;
    let (first, second) = ids
        .split_once(',')
        .ok_or_else(|| miette!("GENERALS_IO_LIVE_USER_IDS needs two IDs"))?;
    let bot = Account {
        endpoint: endpoint.clone(),
        user_id: first.to_owned(),
    };
    let opponent = Account {
        endpoint,
        user_id: second.to_owned(),
    };

    let ctrl_c = CancellationToken::new();
    tokio::spawn(cancel_on_ctrl_c(ctrl_c.clone()));
    let room = LiveRoom::new(matches, &ctrl_c);
    let (bot_run, opponent_run) = room
        .with_time_limit(TIME_LIMIT_PER_MATCH * matches.get(), async {
            tokio::join!(
                room.run_bot(&bot, play_opening),
                room.run_bot(&opponent, play_idle),
            )
        })
        .await;

    for replay_id in verified_replays(vec![bot_run, opponent_run], matches.get())? {
        println!("{}", replay_url(&replay_id));
    }
    Ok(())
}
