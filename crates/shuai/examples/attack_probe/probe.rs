//! One recorded client connection, and starting a game with several of them.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use bytestring::ByteString;
use miette::{IntoDiagnostic, Result, miette};
use serde_json::{Value, json};
use shuai::game::{GameState, Terrain};
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio::task::JoinSet;
use url::Url;

use crate::{ENDPOINT, WAIT};

pub(crate) async fn account_user_id(account: usize) -> Result<String> {
    crate::accounts::registered_user_ids()
        .await?
        .into_iter()
        .nth(account)
        .ok_or_else(|| miette!("user/accounts.db needs {} registered accounts", account + 1))
}

/// One connection's receiver, sender, reconstructed state, and trace.
pub(crate) struct Probe {
    pub(crate) label: &'static str,
    pub(crate) user_id: String,
    pub(crate) client: Option<Client>,
    pub(crate) tx: GeneralsIoSender,
    pub(crate) rx: SocketReceiver,
    pub(crate) state: GameState,
    pub(crate) started: Instant,
    pub(crate) trace: BufWriter<File>,
    pub(crate) next_attack_index: i32,
}

impl Probe {
    pub(crate) async fn connect(
        label: &'static str,
        account: usize,
        path: &Path,
        started: Instant,
    ) -> Result<Self> {
        let client = ClientBuilder::new(Url::parse(ENDPOINT).into_diagnostic()?)
            .transport(TransportStrategy::WebSocket)
            .open()
            .into_diagnostic()?;
        let (socket_tx, rx) = client
            .connect(ByteString::from_static("/"))
            .await
            .into_diagnostic()?;
        Ok(Self {
            label,
            user_id: account_user_id(account).await?,
            client: Some(client),
            tx: GeneralsIoSender::new(socket_tx),
            rx,
            state: GameState::default(),
            started,
            trace: BufWriter::new(
                File::options()
                    .create(true)
                    .append(true)
                    .open(path)
                    .into_diagnostic()?,
            ),
            next_attack_index: 1,
        })
    }

    pub(crate) fn record(&mut self, entry: &Value) -> Result<()> {
        let elapsed = u64::try_from(self.started.elapsed().as_millis()).into_diagnostic()?;
        let line = json!({"t_ms": elapsed, "client": self.label, "entry": entry});
        writeln!(self.trace, "{line}").into_diagnostic()?;
        self.trace.flush().into_diagnostic()
    }

    pub(crate) fn record_command(&mut self, command: &str, arguments: &Value) -> Result<()> {
        self.record(&json!({"out": command, "args": arguments}))
    }

    /// Receives one signal, records it, and applies game events to the state.
    pub(crate) async fn receive(&mut self) -> Result<Option<GeneralsIoEvent>> {
        let signal = self
            .rx
            .recv()
            .await
            .ok_or_else(|| miette!("{} connection closed", self.label))?;
        let Signal::Event(event) = signal else {
            self.record(&json!({"signal": signal.to_string()}))?;
            return Ok(None);
        };
        let payload = event.payload.clone();
        let raw: Value = serde_json::from_str(&payload).into_diagnostic()?;
        self.record(&json!({"in": raw}))?;
        match GeneralsIoSignal::from(event) {
            GeneralsIoSignal::Event(GeneralsIoEvent::GameStart(Event { payload, .. })) => {
                self.state = GameState::default();
                self.state.start(payload.data).map_err(|e| miette!("{e}"))?;
                self.next_attack_index = 1;
                Ok(None)
            }
            GeneralsIoSignal::Event(GeneralsIoEvent::GameUpdate(Event { payload, .. })) => {
                self.state
                    .update(payload.data)
                    .map_err(|e| miette!("{e}"))?;
                Ok(None)
            }
            GeneralsIoSignal::Event(event) => Ok(Some(event)),
            GeneralsIoSignal::MalformedEvent(error) => {
                self.record(&json!({"malformed": error.name, "error": error.source.to_string()}))?;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub(crate) async fn join(&mut self, room: &str) -> Result<()> {
        self.record_command("join_private", &json!([room]))?;
        self.tx
            .join_private(room.to_owned(), self.user_id.clone())
            .await
            .into_diagnostic()?;
        self.set_force_start(room).await
    }

    pub(crate) async fn set_force_start(&mut self, room: &str) -> Result<()> {
        self.record_command("set_force_start", &json!([room, true]))?;
        self.tx
            .set_force_start(room.to_owned(), true)
            .await
            .into_diagnostic()
    }

    pub(crate) fn turn(&self) -> i64 {
        self.state.observation().map_or(-1, |o| o.turn)
    }

    pub(crate) async fn until_game_start(&mut self) -> Result<()> {
        tokio::time::timeout(WAIT, async {
            while self.state.metadata().is_none() {
                self.receive().await?;
            }
            Ok(())
        })
        .await
        .into_diagnostic()?
    }

    pub(crate) async fn until_turn(&mut self, turn: i64) -> Result<()> {
        tokio::time::timeout(WAIT, async {
            while self.turn() < turn {
                self.receive().await?;
            }
            Ok(())
        })
        .await
        .into_diagnostic()?
    }

    /// Reads until `name` arrives or `wait` elapses, and reports which happened.
    pub(crate) async fn wait_for(&mut self, name: &str, wait: Duration) -> Result<bool> {
        let found = tokio::time::timeout(wait, async {
            loop {
                if let Some(event) = self.receive().await?
                    && event.name() == name
                {
                    return Ok::<_, miette::Report>(());
                }
            }
        })
        .await;
        if let Ok(result) = found {
            result.map(|()| true)
        } else {
            self.record(&json!({"note": format!("no {name} within {wait:?}")}))?;
            Ok(false)
        }
    }

    pub(crate) async fn attack_with_index(
        &mut self,
        from: usize,
        to: usize,
        index: i32,
    ) -> Result<()> {
        let from = i32::try_from(from).into_diagnostic()?;
        let to = i32::try_from(to).into_diagnostic()?;
        self.record_command("attack", &json!([from, to, false, index]))?;
        self.tx
            .attack(from, to, false, index)
            .await
            .into_diagnostic()
    }

    pub(crate) async fn attack(&mut self, from: usize, to: usize) -> Result<()> {
        let index = self.next_attack_index;
        self.next_attack_index += 1;
        self.attack_with_index(from, to, index).await
    }

    pub(crate) async fn attack_path(&mut self, path: &[usize]) -> Result<()> {
        for pair in path.windows(2) {
            self.attack(pair[0], pair[1]).await?;
        }
        Ok(())
    }

    pub(crate) async fn surrender(&mut self) -> Result<()> {
        self.record_command("surrender", &json!([]))?;
        self.tx.surrender().await.into_diagnostic()
    }

    pub(crate) async fn leave_game(&mut self) -> Result<()> {
        self.record_command("leave_game", &json!([]))?;
        self.tx.leave_game().await.into_diagnostic()
    }

    /// Finds a simple path of `length` moves from the general through passable tiles.
    pub(crate) fn path_from_general(&self, length: usize) -> Result<Vec<usize>> {
        let start = self
            .state
            .metadata()
            .ok_or_else(|| miette!("no game metadata"))?;
        let me = usize::try_from(start.player_index).into_diagnostic()?;
        let observation = self
            .state
            .observation()
            .ok_or_else(|| miette!("no observation"))?;
        let general = usize::try_from(observation.generals[me]).into_diagnostic()?;
        let map = self.state.map().ok_or_else(|| miette!("no map"))?;
        let width = map.view().armies.width();
        let height = map.view().armies.height();
        let passable = |tile: usize| {
            !self.state.cities().contains(&i64::try_from(tile).unwrap())
                && matches!(
                    map.terrain(tile / width, tile % width),
                    Some(Terrain::Empty | Terrain::Fog)
                )
        };
        let neighbors = |tile: usize| {
            let (row, column) = (tile / width, tile % width);
            [
                (row > 0).then(|| tile - width),
                (row + 1 < height).then(|| tile + width),
                (column > 0).then(|| tile - 1),
                (column + 1 < width).then(|| tile + 1),
            ]
            .into_iter()
            .flatten()
        };
        let mut path = vec![general];
        let mut branches = vec![
            neighbors(general)
                .filter(|&t| passable(t))
                .collect::<Vec<_>>(),
        ];
        while path.len() <= length {
            let Some(next) = branches.last_mut().and_then(Vec::pop) else {
                path.pop();
                branches.pop();
                if path.is_empty() {
                    return Err(miette!("no path of {length} moves"));
                }
                continue;
            };
            if path.contains(&next) {
                continue;
            }
            path.push(next);
            branches.push(neighbors(next).filter(|&t| passable(t)).collect());
        }
        Ok(path)
    }

    pub(crate) async fn close(mut self) -> Result<()> {
        self.trace.flush().into_diagnostic()?;
        let client = self.client.take().expect("a probe closes once");
        let Self { tx, mut rx, .. } = self;
        let join = async {
            tx.disconnect().await;
            drop(tx);
            client.join().await
        };
        let drain = async { while rx.recv().await.is_some() {} };
        let (joined, ()) =
            tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(join, drain) })
                .await
                .into_diagnostic()?;
        joined.into_diagnostic()
    }
}

/// Connects clients, starts one game in a new room, and returns them.
pub(crate) async fn start_game(path: &Path, labels: &'static [&'static str]) -> Result<Vec<Probe>> {
    let started = Instant::now();
    let room = format!("probe{:08x}", fastrand::u32(..));
    let mut probes = Vec::new();
    for (account, &label) in labels.iter().enumerate() {
        let mut probe = Probe::connect(label, account, path, started).await?;
        probe.join(&room).await?;
        probes.push(probe);
    }
    await_game_start(probes).await
}

/// Reads every client concurrently, because a client's errors only show when it is read.
pub(crate) async fn await_game_start(probes: Vec<Probe>) -> Result<Vec<Probe>> {
    let mut starting = JoinSet::new();
    for (position, mut probe) in probes.into_iter().enumerate() {
        starting.spawn(async move {
            probe.until_game_start().await?;
            Ok::<_, miette::Report>((position, probe))
        });
    }
    let mut started = Vec::new();
    while let Some(result) = starting.join_next().await {
        started.push(result.into_diagnostic()??);
    }
    started.sort_by_key(|(position, _)| *position);
    Ok(started.into_iter().map(|(_, probe)| probe).collect())
}

/// Ends the game by surrendering from the first client, and closes every client.
pub(crate) async fn finish(mut probes: Vec<Probe>) -> Result<()> {
    probes[0].surrender().await?;
    for probe in &mut probes {
        probe.wait_for("game_over", Duration::from_secs(5)).await?;
    }
    for probe in probes {
        probe.close().await?;
    }
    Ok(())
}
