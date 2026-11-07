//! Runs the real bot against the production server in a new private room.
//!
//! The first account runs [`Session`] with `play_opening` for a few matches. The second
//! only reads, and rejoins the room after each game, so it never plays a move.
//!
//! ```sh
//! GENERALS_IO_PROBE_USER_IDS=<id1>,<id2> RUST_LOG=shuai=debug \
//!     cargo run --example live_opening -- [matches]
//! ```

use std::pin::pin;
use std::time::Duration;

use bytestring::ByteString;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use shuai::prelude::*;
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use url::Url;

const ENDPOINT: &str = "https://ws.generals.io";
const DEFAULT_MATCHES: u32 = 2;
/// Private rooms do not answer rematch votes, so waiting longer only slows the run.
const REMATCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Keeps the opponent in the room, without moving, until it is stopped.
async fn idle_opponent(user_id: String, room: String) -> Result<()> {
    let client = ClientBuilder::new(Url::parse(ENDPOINT).into_diagnostic()?)
        .transport(TransportStrategy::WebSocket)
        .open()
        .into_diagnostic()?;
    let (socket_tx, mut rx) = client
        .connect(ByteString::from_static("/"))
        .await
        .into_diagnostic()?;
    let tx = GeneralsIoSender::new(socket_tx);
    tx.join_private(room.clone(), user_id.clone())
        .await
        .into_diagnostic()?;
    tx.set_force_start(room.clone(), true)
        .await
        .into_diagnostic()?;
    loop {
        match next_event(&mut rx).await? {
            GeneralsIoEvent::GameOver(_) => {
                tx.leave_game().await.into_diagnostic()?;
                tx.join_private(room.clone(), user_id.clone())
                    .await
                    .into_diagnostic()?;
            }
            // Any player joining clears every force-start vote.
            GeneralsIoEvent::QueueUpdate(Event { payload, .. })
                if payload.data.is_forcing == Some(false) =>
            {
                tx.set_force_start(room.clone(), true)
                    .await
                    .into_diagnostic()?;
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let matches = std::env::args()
        .nth(1)
        .map_or(Ok(DEFAULT_MATCHES), |value| value.parse())
        .into_diagnostic()?;
    let ids = std::env::var("GENERALS_IO_PROBE_USER_IDS")
        .into_diagnostic()
        .wrap_err("GENERALS_IO_PROBE_USER_IDS not set")?;
    let (bot_id, opponent_id) = ids
        .split_once(',')
        .ok_or_else(|| miette!("GENERALS_IO_PROBE_USER_IDS needs two IDs"))?;
    let room = format!("live{:08x}", fastrand::u32(..));

    let shutdown = CancellationToken::new();
    tokio::spawn(cancel_on_ctrl_c(shutdown.clone()));
    let mut opponent = tokio::spawn(idle_opponent(opponent_id.to_owned(), room.clone()));

    let mut completed = 0;
    let mut failure = None;
    let finished = shutdown.clone();
    let session_result = {
        let session = Session::connect(
            Url::parse(ENDPOINT).into_diagnostic()?,
            bot_id.to_owned(),
            shutdown.clone(),
        )
        .await?
        .with_mode(GameMode::FixedPrivate(room))
        .with_rematch_timeout(REMATCH_TIMEOUT)
        .run(
            async |start: Box<GameStartData>,
                   rx: &mut SocketReceiver,
                   tx: &GeneralsIoSender,
                   surrender: &SurrenderOnce,
                   shutdown: &CancellationToken| {
                let result = play_opening(start, rx, tx, surrender, shutdown).await;
                println!("match {} finished: {result:?}", completed + 1);
                match &result {
                    Ok(()) => completed += 1,
                    // Session turns a non-fatal failure during shutdown into success, so
                    // this run keeps its own record of it.
                    Err(error) => failure = Some(format!("{error:?}")),
                }
                if completed >= matches || failure.is_some() {
                    finished.cancel();
                }
                result
            },
        );
        let mut session = pin!(session);
        tokio::select! {
            result = &mut session => result,
            joined = &mut opponent => {
                // The bot would otherwise wait alone in the room forever.
                shutdown.cancel();
                session.await?;
                return match joined.into_diagnostic()? {
                    Ok(()) => Err(miette!("the opponent stopped")),
                    Err(error) => Err(error.wrap_err("the opponent failed")),
                };
            }
        }
    };
    opponent.abort();
    session_result?;
    if let Some(failure) = failure {
        return Err(miette!("a match failed: {failure}"));
    }
    if completed < matches {
        return Err(miette!("only {completed} of {matches} matches completed"));
    }
    println!("completed {completed} matches");
    Ok(())
}
