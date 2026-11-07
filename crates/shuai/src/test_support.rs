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
    transcript: mpsc::UnboundedReceiver<Vec<Value>>,
    outgoing: mpsc::UnboundedSender<String>,
    task: JoinHandle<()>,
}

impl FakeServer {
    pub async fn start() -> Self {
        Self::start_with_username(Some("bot")).await
    }

    pub async fn start_with_username(username: Option<&str>) -> Self {
        Self::start_with_ack(Some(json!([username]).to_string())).await
    }

    /// Starts a server where the test acknowledges each username request itself. The
    /// acknowledgement ID becomes the request's last argument in the transcript.
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

    /// Acknowledges a username request, given the ID from its transcript entry.
    pub fn acknowledge_username(&self, id: &Value) {
        let id = id.as_str().unwrap();
        self.outgoing.send(format!(r#"43{id}["bot"]"#)).unwrap();
    }

    /// Sends a server-initiated namespace disconnect.
    pub fn disconnect_client(&self) {
        self.outgoing.send("41".to_owned()).unwrap();
    }

    /// Requires `name` to be the very next client command and returns its arguments.
    pub async fn expect(&mut self, name: &str) -> Vec<Value> {
        let event = tokio::time::timeout(EXPECT_TIMEOUT, self.transcript.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out while expecting `{name}`"))
            .unwrap_or_else(|| panic!("connection ended while expecting `{name}`"));
        assert_eq!(event[0], name, "unexpected client command order");
        event[1..].to_vec()
    }

    /// Requires leaving a match, followed by the request that bounds its late events.
    pub async fn expect_leave(&mut self) {
        self.expect("leave_game").await;
        self.expect("get_username").await;
    }

    /// Returns the next client command if one arrives within `wait`.
    pub async fn next_within(&mut self, wait: Duration) -> Option<Vec<Value>> {
        tokio::time::timeout(wait, self.transcript.recv())
            .await
            .ok()
            .flatten()
    }

    /// Requires a private room join with force start, and returns the room ID.
    pub async fn expect_private_join(&mut self) -> Value {
        let room_id = self.expect("join_private").await[0].clone();
        assert_eq!(self.expect("set_force_start").await[0], room_id);
        room_id
    }

    /// Waits for the connection to end and requires exactly `names` to remain.
    pub async fn expect_only(mut self, names: &[&str]) {
        let mut remaining = Vec::new();
        loop {
            let event = tokio::time::timeout(EXPECT_TIMEOUT, self.transcript.recv())
                .await
                .expect("the client should close the connection");
            let Some(event) = event else { break };
            remaining.push(event[0].as_str().unwrap().to_owned());
        }
        assert_eq!(remaining, names);
        self.task.await.expect("the fake server should not panic");
    }
}

async fn serve(
    listener: TcpListener,
    username_ack: Option<String>,
    transcript: mpsc::UnboundedSender<Vec<Value>>,
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
                    let _ = transcript.send(vec![json!(CLIENT_DISCONNECT)]);
                } else if let Some(packet) = text.strip_prefix("42") {
                    let digits = packet.find('[').unwrap();
                    let (id, body) = packet.split_at(digits);
                    let event: Vec<Value> = serde_json::from_str(body).unwrap();
                    let mut event = event;
                    if !id.is_empty() && username_ack.is_none() {
                        event.push(json!(id));
                    } else if !id.is_empty()
                        && let Some(username_ack) = &username_ack
                    {
                        assert_eq!(event[0], "get_username", "unexpected acknowledged request");
                        writable &= socket
                            .send(Message::text(format!("43{id}{username_ack}")))
                            .await
                            .is_ok();
                    }
                    let _ = transcript.send(event);
                }
            }
        }
    }
}
