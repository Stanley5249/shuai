use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio_util::sync::CancellationToken;

use super::surrender::SurrenderOnce;

/// What Session lends a gameplay function for one match.
///
/// Gameplay must observe `shutdown`, because Session never drops a running gameplay
/// future, and must surrender only through `surrender`.
pub struct MatchContext<'a> {
    /// The event that started this match.
    pub start: Box<GameStartData>,
    /// The connection's only receiver.
    pub rx: &'a mut SocketReceiver,
    /// The connection's sender.
    pub tx: &'a GeneralsIoSender,
    /// This match's one allowed surrender.
    pub surrender: &'a SurrenderOnce,
    /// Cancelled when the session should stop.
    pub shutdown: &'a CancellationToken,
}
