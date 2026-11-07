use miette::{Diagnostic, Report, Result};
use shuai_client::prelude::*;
use sioc::prelude::*;
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;

/// A connection failure observed while waiting for an event.
#[derive(Debug, Error, Diagnostic)]
pub enum SignalError {
    /// The namespace disconnected or the connection task stopped.
    #[error("connection closed")]
    #[diagnostic(code(shuai::session::disconnected))]
    Disconnected,
    /// The server rejected the namespace connection.
    #[error("server rejected the connection: {message}")]
    #[diagnostic(code(shuai::session::connect_rejected))]
    ConnectRejected {
        /// The server's rejection message.
        message: String,
    },
}

/// Returns the next known event, applying the shared signal policy.
///
/// Cancel safe: the only suspend point is the channel receive.
///
/// # Errors
/// Returns an error when the connection or the match state can no longer be trusted.
pub async fn next_event(rx: &mut SocketReceiver) -> Result<GeneralsIoEvent> {
    loop {
        if let Some(event) = admit_signal(next_signal(rx).await)? {
            tracing::trace!(event.name = event.name(), "received event");
            return Ok(event);
        }
    }
}

/// Discards every event already received, applying the shared signal policy.
pub(super) fn discard_received_events(rx: &mut SocketReceiver) -> Result<()> {
    loop {
        match rx.try_recv() {
            Ok(signal) => log_discarded(admit_signal(signal.into())?),
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Err(SignalError::Disconnected.into()),
        }
    }
}

/// Records an event that arrived from a match Session has already left.
pub(super) fn log_discarded(event: Option<GeneralsIoEvent>) {
    match event {
        Some(GeneralsIoEvent::GameStart(_)) => {
            tracing::warn!("discarded a game start from the match that was left");
        }
        Some(event) => tracing::debug!(event.name = event.name(), "discarded event"),
        None => {}
    }
}

/// Decides what one signal means for every phase.
///
/// Unknown events and malformed events without critical meaning are logged and
/// skipped. So are advisory events such as `moderation_warning` and `warn_popup`,
/// because none changes the connection or the match. Disconnects, server error events,
/// and malformed critical events are errors.
fn admit_signal(signal: GeneralsIoSignal) -> Result<Option<GeneralsIoEvent>> {
    match signal {
        GeneralsIoSignal::Connected => {
            tracing::debug!("namespace connected");
            Ok(None)
        }
        GeneralsIoSignal::Event(
            event @ (GeneralsIoEvent::ModerationWarning(_)
            | GeneralsIoEvent::DisableStayParty(_)
            | GeneralsIoEvent::AfkWarning(_)),
        ) => {
            tracing::warn!(event.name = event.name(), "received advisory event");
            Ok(None)
        }
        GeneralsIoSignal::Event(GeneralsIoEvent::WarnPopup(Event { payload, .. })) => {
            tracing::warn!(message = payload.warn_message, "received server warning");
            Ok(None)
        }
        GeneralsIoSignal::Event(event) => event.into_result().map(Some),
        GeneralsIoSignal::UnknownEvent { name, payload } => {
            tracing::warn!(event.name = %name, %payload, "received unknown event");
            Ok(None)
        }
        GeneralsIoSignal::MalformedEvent(error) if is_critical_event(&error.name) => {
            Err(Report::new(error))
        }
        GeneralsIoSignal::MalformedEvent(error) => {
            tracing::warn!(event.name = %error.name, error = %error.source, "skipped malformed event");
            Ok(None)
        }
        GeneralsIoSignal::ConnectRejected { message } => {
            Err(SignalError::ConnectRejected { message }.into())
        }
        GeneralsIoSignal::Disconnected => Err(SignalError::Disconnected.into()),
    }
}

/// Reports whether skipping a malformed event would leave match state untrustworthy, or
/// hide a server rejection, whatever its payload said.
fn is_critical_event(name: &str) -> bool {
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
    .contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admit(payload: &str) -> Result<Option<GeneralsIoEvent>> {
        admit_signal(DynEvent::new(payload.to_owned(), None).into())
    }

    #[test]
    fn known_event_is_delivered() {
        assert!(matches!(
            admit(r#"["game_over"]"#),
            Ok(Some(GeneralsIoEvent::GameOver(_)))
        ));
    }

    #[test]
    fn unknown_advisory_and_unrelated_malformed_events_are_skipped() {
        for payload in [
            r#"["brand_new_event",1]"#,
            r#"["queue_update",7]"#,
            r#"["moderation_warning"]"#,
            r#"["disable_stay_party"]"#,
        ] {
            assert!(matches!(admit(payload), Ok(None)), "{payload}");
        }
    }

    #[test]
    fn malformed_match_events_and_server_errors_are_errors() {
        for payload in [
            r#"["game_start",{"playerIndex":"x"}]"#,
            r#"["game_update",{"turn":"x"}]"#,
            r#"["error_user_id"]"#,
            r#"["error_banned","banned"]"#,
            r#"["error_banned",{}]"#,
        ] {
            assert!(admit(payload).is_err(), "{payload}");
        }
    }

    #[test]
    fn connection_outcomes_keep_their_type() {
        assert!(matches!(
            admit_signal(GeneralsIoSignal::Connected),
            Ok(None)
        ));
        let disconnected = admit_signal(GeneralsIoSignal::Disconnected).unwrap_err();
        assert!(matches!(
            disconnected.downcast_ref(),
            Some(SignalError::Disconnected)
        ));
        let rejected = admit_signal(GeneralsIoSignal::ConnectRejected {
            message: "no".to_owned(),
        })
        .unwrap_err();
        assert!(matches!(
            rejected.downcast_ref(),
            Some(SignalError::ConnectRejected { .. })
        ));
    }
}
