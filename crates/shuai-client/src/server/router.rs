//! Server-to-client event router.

use crate::prelude::*;
use bytestring::ByteString;
use miette::{Diagnostic, Result};
use sioc::error::EventError;
use sioc::prelude::*;
use thiserror::Error;

/// Generates the router and the name table that separates unknown events from
/// malformed known events, from a single variant list.
macro_rules! define_event_router {
    ($($variant:ident($payload:ty),)*) => {
        /// Dispatches a server-to-client event by name.
        #[derive(Debug, EventRouter)]
        pub enum GeneralsIoEvent {
            $($variant(Event<$payload>),)*
        }

        const KNOWN_EVENT_NAMES: &[&str] = &[$(<$payload as EventType>::NAME,)*];
    };
}

define_event_router! {
    // Game state
    PreGameStart(PreGameStart),
    GameStart(GameStart),
    GameUpdate(GameUpdate),
    GameOver(GameOver),
    GameLost(GameLost),
    GameWon(GameWon),

    // Game mechanics
    DisableRematch(DisableRematch),
    RematchUpdate(RematchUpdate),
    AfkWarning(AfkWarning),
    WarnPopup(WarnPopup),
    PingTile(PingTileOn),

    // Queue and matchmaking
    QueueUpdate(QueueUpdate),
    TeamUpdate(TeamUpdate),
    BigTeamUpdate(BigTeamUpdate),
    TeamJoinedQueue(TeamJoinedQueue),
    RemovedFromQueue(RemovedFromQueue),
    QueueLeft(QueueLeft),
    QueueWaiting(QueueWaiting),
    QueueWaitingOver(QueueWaitingOver),
    PartyJoinedQueue(PartyJoinedQueue),

    // Chat
    RecvChatMessage(RecvChatMessage),
    ChatRedaction(ChatRedaction),
    ChatColorChange(ChatColorChange),
    QueueChatHistory(QueueChatHistory),
    PartyChatHistory(PartyChatHistory),
    MainMenuChatUserLeft(MainMenuChatUserLeft),

    // Connectivity
    PongServer(PongServer),
    PongWorker(PongWorker),

    // Party
    JoinedParty(JoinedParty),
    LeftParty(LeftParty),
    PartyUpdate(PartyUpdate),
    PartyUserLeft(PartyUserLeft),
    PartyInvite(PartyInvite),
    PartyInviteRequest(PartyInviteRequest),
    StayPartyUpdate(StayPartyUpdate),

    // Friends
    FriendOnline(FriendOnline),
    FriendOffline(FriendOffline),
    FriendAdded(FriendAdded),
    FriendRemoved(FriendRemoved),
    FriendRequestReceived(FriendRequestReceived),
    FriendPresence(FriendPresence),

    // Stats and leaderboards
    Stars(Stars),
    Rank(Rank),
    Teammates2v2(Teammates2v2),

    // Public customs
    PublicCustomsUpdate(PublicCustomsUpdate),

    // Server status and notifications
    Notify(Notify),
    ServerRestart(ServerRestart),
    ServerDown(ServerDown),

    // Errors
    ErrorSetUsername(ErrorSetUsername),
    ErrorUserId(ErrorUserId),
    ErrorQueueFull(ErrorQueueFull),
    ErrorBanned(ErrorBanned),
    ErrorKicked(ErrorKicked),
    GioError(GioError),
    ErrorJoinQueue(ErrorJoinQueue),
    ModerationWarning(ModerationWarning),
    PartyJoinError(PartyJoinError),
    DisableStayParty(DisableStayParty),
}

impl GeneralsIoEvent {
    /// Converts error events into a [`miette::Report`] and passes other events through as `Ok`.
    ///
    /// # Errors
    ///
    /// Returns the server's report for an error event, moderation warning, or disabled party stay.
    pub fn into_result(self) -> Result<Self> {
        match self {
            GeneralsIoEvent::ErrorSetUsername(e) => Err(e.payload.into()),
            GeneralsIoEvent::ErrorUserId(e) => Err(e.payload.into()),
            GeneralsIoEvent::ErrorQueueFull(e) => Err(e.payload.into()),
            GeneralsIoEvent::ErrorBanned(e) => Err(e.payload.into()),
            GeneralsIoEvent::ErrorKicked(e) => Err(e.payload.into()),
            GeneralsIoEvent::GioError(e) => Err(e.payload.into()),
            GeneralsIoEvent::ErrorJoinQueue(e) => Err(e.payload.into()),
            GeneralsIoEvent::ModerationWarning(e) => Err(e.payload.into()),
            GeneralsIoEvent::PartyJoinError(e) => Err(e.payload.into()),
            GeneralsIoEvent::DisableStayParty(e) => Err(e.payload.into()),
            other => Ok(other),
        }
    }
}

/// A known event whose payload does not match its Rust schema.
#[derive(Debug, Error, Diagnostic)]
#[error("malformed `{name}` event")]
pub struct MalformedEvent {
    /// The wire event name.
    pub name: String,
    /// The decoding failure.
    #[source]
    pub source: EventError,
}

impl MalformedEvent {
    /// Reports whether skipping this event would leave match state untrustworthy or
    /// hide a server rejection, whatever its payload said.
    #[must_use]
    pub fn is_critical(&self) -> bool {
        [
            GameStart::NAME,
            GameUpdate::NAME,
            GameOver::NAME,
            GameLost::NAME,
            GameWon::NAME,
            ErrorSetUsername::NAME,
            ErrorUserId::NAME,
            ErrorQueueFull::NAME,
            ErrorBanned::NAME,
            ErrorKicked::NAME,
            GioError::NAME,
            ErrorJoinQueue::NAME,
        ]
        .contains(&self.name.as_str())
    }
}

/// One inbound signal with connection and decoding outcomes kept distinct.
///
/// [`SocketReceiver::listen`] drops connection signals and reports unknown names
/// as decoding errors, so lifecycle code reads this type instead.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "a short-lived return value, matched at once, so boxing every event gains nothing"
)]
pub enum GeneralsIoSignal {
    /// The server confirmed the namespace connection.
    Connected,
    /// A known event with a valid payload.
    Event(GeneralsIoEvent),
    /// An event name absent from the audited inventory, kept as raw JSON.
    UnknownEvent {
        /// The wire event name.
        name: String,
        /// The complete JSON event array.
        payload: ByteString,
    },
    /// A known event, or a nameless packet, that failed to decode.
    MalformedEvent(MalformedEvent),
    /// The server rejected the namespace connection.
    ConnectRejected {
        /// The server's rejection message.
        message: String,
    },
    /// The namespace disconnected or the connection task stopped.
    Disconnected,
}

impl From<DynEvent> for GeneralsIoSignal {
    fn from(event: DynEvent) -> Self {
        let payload = event.payload.clone();
        match GeneralsIoEvent::try_from(event) {
            Ok(event) => Self::Event(event),
            Err(source) => match wire_event_name(&payload) {
                Some(name) if !KNOWN_EVENT_NAMES.contains(&name.as_str()) => {
                    Self::UnknownEvent { name, payload }
                }
                name => Self::MalformedEvent(MalformedEvent {
                    name: name.unwrap_or_default(),
                    source,
                }),
            },
        }
    }
}

fn wire_event_name(payload: &str) -> Option<String> {
    let arguments: Vec<serde_json::Value> = serde_json::from_str(payload).ok()?;
    Some(arguments.first()?.as_str()?.to_owned())
}

/// Receives the next signal from the connection.
///
/// Cancel safe: the only suspend point is the channel receive. A closed channel
/// is reported as [`GeneralsIoSignal::Disconnected`], which is terminal for the
/// connection.
pub async fn next_signal(rx: &mut SocketReceiver) -> GeneralsIoSignal {
    rx.recv()
        .await
        .map_or(GeneralsIoSignal::Disconnected, GeneralsIoSignal::from)
}

impl From<Signal> for GeneralsIoSignal {
    fn from(signal: Signal) -> Self {
        match signal {
            Signal::Connect(_) => Self::Connected,
            Signal::Event(event) => event.into(),
            Signal::ConnectError(error) => Self::ConnectRejected {
                message: error.message.to_string(),
            },
            Signal::Disconnect => Self::Disconnected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(payload: &str) -> GeneralsIoSignal {
        DynEvent::new(payload.to_owned(), None).into()
    }

    #[test]
    fn known_event_with_valid_payload_decodes() {
        assert!(matches!(
            decode(r#"["game_over"]"#),
            GeneralsIoSignal::Event(GeneralsIoEvent::GameOver(_))
        ));
    }

    #[test]
    fn unknown_name_keeps_its_raw_payload() {
        let GeneralsIoSignal::UnknownEvent { name, payload } = decode(r#"["brand_new",1]"#) else {
            panic!("expected an unknown event");
        };
        assert_eq!(name, "brand_new");
        assert_eq!(&*payload, r#"["brand_new",1]"#);
    }

    #[test]
    fn known_name_with_bad_payload_is_malformed_not_unknown() {
        let GeneralsIoSignal::MalformedEvent(error) = decode(r#"["game_update",{"turn":"x"}]"#)
        else {
            panic!("expected a malformed event");
        };
        assert_eq!(error.name, "game_update");
        assert!(error.is_critical());
    }

    #[test]
    fn malformed_queue_update_is_not_critical() {
        let GeneralsIoSignal::MalformedEvent(error) = decode(r#"["queue_update",7]"#) else {
            panic!("expected a malformed event");
        };
        assert!(!error.is_critical());
    }

    #[test]
    fn nameless_packet_is_malformed() {
        assert!(matches!(
            decode("[]"),
            GeneralsIoSignal::MalformedEvent(MalformedEvent { name, .. }) if name.is_empty()
        ));
    }
}
