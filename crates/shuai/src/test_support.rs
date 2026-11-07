//! A scripted local Socket.IO server, so tests use the real sioc client.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

const EXPECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Recorded in the transcript when the client sends a namespace disconnect.
pub const CLIENT_DISCONNECT: &str = "client disconnect";

/// Controls one accepted connection from the test body.
pub struct FakeServer {
    pub url: Url,
    transcript: mpsc::UnboundedReceiver<Command>,
    outgoing: mpsc::UnboundedSender<String>,
    task: JoinHandle<()>,
}

/// A client command as a JSON array, and the ID that would acknowledge it.
type Command = (Vec<Value>, Option<String>);

impl FakeServer {
    pub async fn start() -> Self {
        Self::start_with_username(Some("bot")).await
    }

    pub async fn start_with_username(username: Option<&str>) -> Self {
        Self::start_with_ack(Some(json!([username]).to_string())).await
    }

    /// Starts a server where the test acknowledges each username request itself, using
    /// the ID from [`FakeServer::expect_username_request`].
    pub async fn start_without_username_ack() -> Self {
        Self::start_with_ack(None).await
    }

    async fn start_with_ack(username_ack: Option<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let (transcript_tx, transcript) = mpsc::unbounded_channel();
        let (outgoing, outgoing_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(serve(listener, username_ack, transcript_tx, outgoing_rx));
        Self {
            url,
            transcript,
            outgoing,
            task,
        }
    }

    /// Sends a Socket.IO event, given its complete JSON array.
    pub fn send(&self, event: &Value) {
        self.outgoing.send(format!("42{event}")).unwrap();
    }

    /// Acknowledges a username request.
    pub fn acknowledge_username(&self, id: &str, username: Option<&str>) {
        let answer = json!([username]);
        self.outgoing.send(format!("43{id}{answer}")).unwrap();
    }

    /// Sends a server-initiated namespace disconnect.
    pub fn disconnect_client(&self) {
        self.outgoing.send("41".to_owned()).unwrap();
    }

    /// Requires `name` to be the very next client command and returns its arguments.
    pub async fn expect(&mut self, name: &str) -> Vec<Value> {
        let (event, _) = self.expect_command(name).await;
        event[1..].to_vec()
    }

    /// Requires a username request as the very next client command, and returns the ID
    /// that acknowledges it.
    pub async fn expect_username_request(&mut self) -> String {
        let (_, ack_id) = self.expect_command("get_username").await;
        ack_id.expect("a username request needs an acknowledgement")
    }

    async fn expect_command(&mut self, name: &str) -> Command {
        let command = tokio::time::timeout(EXPECT_TIMEOUT, self.transcript.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out while expecting `{name}`"))
            .unwrap_or_else(|| panic!("connection ended while expecting `{name}`"));
        assert_eq!(command.0[0], name, "unexpected client command order");
        command
    }

    /// Requires leaving a match, followed by the request that bounds its late events.
    pub async fn expect_leave(&mut self) {
        self.expect("leave_game").await;
        self.expect("get_username").await;
    }

    /// Returns the next client command if one arrives within `wait`.
    #[cfg_attr(
        not(feature = "opening"),
        expect(dead_code, reason = "only Opening's tests watch for silence")
    )]
    pub async fn next_within(&mut self, wait: Duration) -> Option<Vec<Value>> {
        tokio::time::timeout(wait, self.transcript.recv())
            .await
            .ok()
            .flatten()
            .map(|(event, _)| event)
    }

    /// Returns the next client command's name.
    #[cfg_attr(
        not(feature = "opening"),
        expect(dead_code, reason = "only Opening's tests read commands by name")
    )]
    pub async fn expect_next_name(&mut self) -> String {
        let (event, _) = tokio::time::timeout(EXPECT_TIMEOUT, self.transcript.recv())
            .await
            .expect("a client command should arrive")
            .expect("the connection should still be open");
        event[0].as_str().unwrap().to_owned()
    }

    /// Requires a private room join with force start, and returns the room ID.
    pub async fn expect_private_join(&mut self) -> Value {
        let room_id = self.expect("join_private").await[0].clone();
        assert_eq!(self.expect("set_force_start").await[0], room_id);
        room_id
    }

    /// Waits for the connection to end and requires exactly `names` to remain.
    pub async fn expect_only(self, names: &[&str]) {
        assert_eq!(self.remaining_commands().await, names);
    }

    /// Waits for the connection to end and returns the commands that remain.
    pub async fn remaining_commands(mut self) -> Vec<String> {
        let mut remaining = Vec::new();
        loop {
            let event = tokio::time::timeout(EXPECT_TIMEOUT, self.transcript.recv())
                .await
                .expect("the client should close the connection");
            let Some((event, _)) = event else { break };
            remaining.push(event[0].as_str().unwrap().to_owned());
        }
        self.task.await.expect("the fake server should not panic");
        remaining
    }
}

async fn serve(
    listener: TcpListener,
    username_ack: Option<String>,
    transcript: mpsc::UnboundedSender<Command>,
    mut outgoing: mpsc::UnboundedReceiver<String>,
) {
    let (stream, _) = listener.accept().await.unwrap();
    let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
    let open = json!({
        "sid": "engine", "upgrades": [], "pingInterval": 25000,
        "pingTimeout": 20000, "maxPayload": 1_000_000,
    });
    socket
        .send(Message::text(format!("0{open}")))
        .await
        .unwrap();
    let mut writable = true;
    // A real server sends namespace events only after confirming the namespace.
    let mut connected = false;
    loop {
        tokio::select! {
            frame = outgoing.recv(), if writable && connected => {
                let Some(frame) = frame else {
                    writable = false;
                    continue;
                };
                // A failed write only means the client stopped reading. Commands it
                // already sent are still buffered, so keep reading the transcript.
                writable = socket.send(Message::text(frame)).await.is_ok();
            }
            message = socket.next() => {
                let Some(Ok(Message::Text(text))) = message else { return };
                let text = text.as_str();
                if text == "40" {
                    connected = true;
                    writable &= socket
                        .send(Message::text(r#"40{"sid":"socket"}"#))
                        .await
                        .is_ok();
                } else if text == "41" {
                    let _ = transcript.send((vec![json!(CLIENT_DISCONNECT)], None));
                } else if let Some(packet) = text.strip_prefix("42") {
                    let digits = packet.find('[').unwrap();
                    let (id, body) = packet.split_at(digits);
                    let event: Vec<Value> = serde_json::from_str(body).unwrap();
                    if !id.is_empty()
                        && let Some(username_ack) = &username_ack
                    {
                        assert_eq!(event[0], "get_username", "unexpected acknowledged request");
                        writable &= socket
                            .send(Message::text(format!("43{id}{username_ack}")))
                            .await
                            .is_ok();
                    }
                    let ack_id = (!id.is_empty()).then(|| id.to_owned());
                    let _ = transcript.send((event, ack_id));
                }
            }
        }
    }
}
