use std::sync::atomic::{AtomicBool, Ordering};

use shuai_client::prelude::*;

/// Allows one surrender per match, whichever phase wants it first.
///
/// Session creates one for each match, and gameplay and Session's own exit paths all
/// send through it. An attempt counts even if the local send fails, because nothing
/// shows whether the packet left, and a surrender is never retried.
#[derive(Debug, Default)]
pub struct SurrenderOnce {
    attempted: AtomicBool,
}

impl SurrenderOnce {
    /// Sends the surrender unless this match already attempted one.
    ///
    /// The attempt is marked when this future is first polled, so a future that was
    /// created but never polled does not use it up.
    ///
    /// # Errors
    /// Returns the local send failure of the attempt this call made.
    pub async fn send(&self, tx: &GeneralsIoSender) -> sioc::error::Result<()> {
        if self.attempted.swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        tx.surrender().await
    }
}
