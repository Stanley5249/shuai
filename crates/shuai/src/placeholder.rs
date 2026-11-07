//! A minimal concrete gameplay stub for exercising the session lifecycle.

use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::session::{MatchError, next_event};

const SURRENDER_TURN: i64 = 10;

/// Surrenders at a fixed turn and returns after `GameOver`.
///
/// # Errors
/// Returns a fatal error when the connection or match state cannot be trusted.
#[instrument(skip_all, err(Debug))]
pub async fn play_placeholder(
    _start: Box<GameStartData>,
    rx: &mut SocketReceiver,
    tx: &GeneralsIoSender,
    shutdown: &CancellationToken,
) -> Result<(), MatchError> {
    let mut surrendered = false;
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return Ok(()),
            event = next_event(rx) => match event? {
                GeneralsIoEvent::GameUpdate(Event { payload, .. })
                    if !surrendered && payload.data.turn >= SURRENDER_TURN =>
                {
                    tx.surrender().await.map_err(miette::Report::new)?;
                    surrendered = true;
                }
                GeneralsIoEvent::GameOver(_) => return Ok(()),
                _ => {}
            },
        }
    }
}
