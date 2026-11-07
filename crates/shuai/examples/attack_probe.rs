//! Records live attack-queue behavior for the protocol note in `docs/protocol/`.
//!
//! Each scenario plays one short game in a new private room containing only this
//! program's clients, and ends it by surrendering. Every inbound packet and outbound
//! command is written to `<out>/<scenario>.jsonl`. User IDs are never recorded.
//!
//! The server requires a username to play, so this needs existing accounts: three
//! distinct IDs, because the `three` scenario uses them all.
//!
//! ```sh
//! GENERALS_IO_PROBE_USER_IDS=<id1>,<id2>,<id3> \
//!     cargo run --example attack_probe -- <out-dir> [scenario...]
//! ```
//!
//! Passing `check-accounts` instead of an output directory reports, by position,
//! which of those IDs have a username. It makes one read-only request per ID.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bytestring::ByteString;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use serde_json::{Value, json};
use shuai::game::{GameState, Terrain};
use shuai_client::prelude::*;
use sioc::prelude::*;
use tokio::task::JoinSet;
use url::Url;

const ENDPOINT: &str = "https://ws.generals.io";
const START_TURN: i64 = 20;
const WAIT: Duration = Duration::from_secs(60);
const SCENARIOS: &[&str] = &[
    "fifo", "invalid", "clear", "undo", "gaps", "early", "rematch", "three", "speed",
];

fn account_user_id(account: usize) -> Result<String> {
    let ids = std::env::var("GENERALS_IO_PROBE_USER_IDS")
        .into_diagnostic()
        .wrap_err("GENERALS_IO_PROBE_USER_IDS not set")?;
    ids.split(',')
        .nth(account)
        .map(str::to_owned)
        .ok_or_else(|| miette!("GENERALS_IO_PROBE_USER_IDS needs {} IDs", account + 1))
}

/// One connection's receiver, sender, reconstructed state, and trace.
struct Probe {
    label: &'static str,
    user_id: String,
    client: Option<Client>,
    tx: GeneralsIoSender,
    rx: SocketReceiver,
    state: GameState,
    started: Instant,
    trace: BufWriter<File>,
    next_attack_index: i32,
}

impl Probe {
    async fn connect(
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
            user_id: account_user_id(account)?,
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

    fn record(&mut self, entry: &Value) -> Result<()> {
        let elapsed = u64::try_from(self.started.elapsed().as_millis()).into_diagnostic()?;
        let line = json!({"t_ms": elapsed, "client": self.label, "entry": entry});
        writeln!(self.trace, "{line}").into_diagnostic()?;
        self.trace.flush().into_diagnostic()
    }

    fn record_command(&mut self, command: &str, arguments: &Value) -> Result<()> {
        self.record(&json!({"out": command, "args": arguments}))
    }

    /// Receives one signal, records it, and applies game events to the state.
    async fn receive(&mut self) -> Result<Option<GeneralsIoEvent>> {
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

    async fn join(&mut self, room: &str) -> Result<()> {
        self.record_command("join_private", &json!([room]))?;
        self.tx
            .join_private(room.to_owned(), self.user_id.clone())
            .await
            .into_diagnostic()?;
        self.tx
            .set_force_start(room.to_owned(), true)
            .await
            .into_diagnostic()
    }

    fn turn(&self) -> i64 {
        self.state.observation().map_or(-1, |o| o.turn)
    }

    async fn until_game_start(&mut self) -> Result<()> {
        tokio::time::timeout(WAIT, async {
            while self.state.metadata().is_none() {
                self.receive().await?;
            }
            Ok(())
        })
        .await
        .into_diagnostic()?
    }

    async fn until_turn(&mut self, turn: i64) -> Result<()> {
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
    async fn wait_for(&mut self, name: &str, wait: Duration) -> Result<bool> {
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

    async fn attack_with_index(&mut self, from: usize, to: usize, index: i32) -> Result<()> {
        let from = i32::try_from(from).into_diagnostic()?;
        let to = i32::try_from(to).into_diagnostic()?;
        self.record_command("attack", &json!([from, to, false, index]))?;
        self.tx
            .attack(from, to, false, index)
            .await
            .into_diagnostic()
    }

    async fn attack(&mut self, from: usize, to: usize) -> Result<()> {
        let index = self.next_attack_index;
        self.next_attack_index += 1;
        self.attack_with_index(from, to, index).await
    }

    async fn attack_path(&mut self, path: &[usize]) -> Result<()> {
        for pair in path.windows(2) {
            self.attack(pair[0], pair[1]).await?;
        }
        Ok(())
    }

    async fn surrender(&mut self) -> Result<()> {
        self.record_command("surrender", &json!([]))?;
        self.tx.surrender().await.into_diagnostic()
    }

    async fn leave_game(&mut self) -> Result<()> {
        self.record_command("leave_game", &json!([]))?;
        self.tx.leave_game().await.into_diagnostic()
    }

    /// Finds a simple path of `length` moves from the general through passable tiles.
    fn path_from_general(&self, length: usize) -> Result<Vec<usize>> {
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

    async fn close(mut self) -> Result<()> {
        self.trace.flush().into_diagnostic()?;
        let client = self.client.take().expect("a probe closes once");
        self.tx.disconnect().await;
        let Self { tx, mut rx, .. } = self;
        drop(tx);
        let join = client.join();
        let drain = async { while rx.recv().await.is_some() {} };
        let (joined, ()) =
            tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(join, drain) })
                .await
                .into_diagnostic()?;
        joined.into_diagnostic()
    }
}

/// Connects clients, starts one game in a new room, and returns them.
async fn start_game(path: &Path, labels: &'static [&'static str]) -> Result<Vec<Probe>> {
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
async fn await_game_start(probes: Vec<Probe>) -> Result<Vec<Probe>> {
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
async fn finish(mut probes: Vec<Probe>) -> Result<()> {
    probes[0].surrender().await?;
    for probe in &mut probes {
        probe.wait_for("game_over", Duration::from_secs(5)).await?;
    }
    for probe in probes {
        probe.close().await?;
    }
    Ok(())
}

async fn scenario(name: &str, path: &Path) -> Result<()> {
    if name == "three" {
        return three_players(path).await;
    }
    if name == "rematch" {
        return rematch(path).await;
    }
    if name == "speed" {
        return speed(path).await;
    }
    let mut probes = start_game(path, &["player", "idle"]).await?;
    let (player, idle) = probes.split_at_mut(1);
    let player = &mut player[0];
    let idle = &mut idle[0];
    // The idle client only reads, so its own channel cannot fill during the script.
    let idle_read = async {
        loop {
            idle.receive().await?;
        }
        #[expect(unreachable_code, reason = "this read loop ends by being dropped")]
        Ok::<(), miette::Report>(())
    };
    let script = async {
        match name {
            "fifo" => {
                // One burst, then a single attack emitted as soon as an update arrives.
                player.until_turn(START_TURN).await?;
                let path = player.path_from_general(6)?;
                player.attack_path(&path[..6]).await?;
                player.until_turn(START_TURN + 8).await?;
                player.attack(path[5], path[6]).await?;
                player.until_turn(START_TURN + 12).await
            }
            "invalid" => {
                // A valid move, an unowned source, and a valid move, all buffered together.
                player.until_turn(START_TURN).await?;
                let path = player.path_from_general(5)?;
                player.attack(path[0], path[1]).await?;
                player.attack(path[4], path[5]).await?;
                player.attack(path[1], path[2]).await?;
                player.until_turn(START_TURN + 6).await
            }
            "clear" => {
                // Clear after two moves executed, then check whether indices resume.
                player.until_turn(START_TURN).await?;
                let path = player.path_from_general(7)?;
                player.attack_path(&path[..6]).await?;
                player.until_turn(START_TURN + 2).await?;
                player.record_command("clear_moves", &json!([]))?;
                player.tx.clear_moves().await.into_diagnostic()?;
                player.until_turn(START_TURN + 7).await?;
                player.attack(path[2], path[3]).await?;
                player.until_turn(START_TURN + 10).await
            }
            "undo" => {
                // Undo the newest buffered move, before any of them can have run.
                player.until_turn(START_TURN).await?;
                let path = player.path_from_general(4)?;
                player.attack_path(&path[..5]).await?;
                player.record_command("undo_move", &json!([]))?;
                player.tx.undo_move().await.into_diagnostic()?;
                player.until_turn(START_TURN + 7).await
            }
            "gaps" => {
                // Indices that skip and go backwards, to separate max from latest.
                player.until_turn(START_TURN).await?;
                let path = player.path_from_general(4)?;
                player.attack_with_index(path[0], path[1], 10).await?;
                player.attack_with_index(path[1], path[2], 30).await?;
                player.attack_with_index(path[2], path[3], 20).await?;
                player.attack_with_index(path[3], path[4], 0).await?;
                player.until_turn(START_TURN + 7).await
            }
            "early" => {
                // Moves buffered while the general holds too few armies to move.
                player.until_turn(1).await?;
                let path = player.path_from_general(3)?;
                player.attack_path(&path).await?;
                player.until_turn(8).await
            }
            other => Err(miette!("unknown scenario {other}")),
        }
    };
    tokio::select! {
        result = script => result?,
        result = idle_read => result?,
    }
    finish(probes).await
}

/// Surrender, a rematch vote from both clients, then a leave that disables rematch.
async fn rematch(path: &Path) -> Result<()> {
    let mut probes = start_game(path, &["player", "other"]).await?;
    probes[0].until_turn(6).await?;
    probes[0].surrender().await?;
    for probe in &mut probes {
        probe.wait_for("game_over", Duration::from_secs(10)).await?;
    }
    for probe in &mut probes {
        probe.record_command("rematch", &json!([true]))?;
        probe.tx.rematch(true).await.into_diagnostic()?;
        probe.state = GameState::default();
    }
    let mut probes = await_game_start(probes).await?;
    probes[0].until_turn(6).await?;
    probes[0].surrender().await?;
    for probe in &mut probes {
        probe.wait_for("game_over", Duration::from_secs(10)).await?;
    }
    probes[1].leave_game().await?;
    probes[0]
        .wait_for("disable_rematch", Duration::from_secs(5))
        .await?;
    probes[0].leave_game().await?;
    for probe in probes {
        probe.close().await?;
    }
    Ok(())
}

/// A surrender while two other players remain, which may not produce `game_over`.
async fn three_players(path: &Path) -> Result<()> {
    let mut probes = start_game(path, &["player", "idle1", "idle2"]).await?;
    probes[0].until_turn(6).await?;
    probes[0].surrender().await?;
    probes[0]
        .wait_for("game_over", Duration::from_secs(8))
        .await?;
    probes[0].leave_game().await?;
    probes[1].surrender().await?;
    for probe in &mut probes[1..] {
        probe.wait_for("game_over", Duration::from_secs(10)).await?;
    }
    for probe in probes {
        probe.close().await?;
    }
    Ok(())
}

/// Update cadence at double speed, set by the room host before the game starts.
async fn speed(path: &Path) -> Result<()> {
    let started = Instant::now();
    let room = format!("probe{:08x}", fastrand::u32(..));
    let mut host = Probe::connect("player", 0, path, started).await?;
    let mut other = Probe::connect("other", 1, path, started).await?;
    host.record_command("join_private", &json!([room]))?;
    host.tx
        .join_private(room.clone(), host.user_id.clone())
        .await
        .into_diagnostic()?;
    let options: CustomGameOptions =
        serde_json::from_value(json!({"game_speed": 2.0})).into_diagnostic()?;
    host.record_command("set_custom_options", &json!([room, {"game_speed": 2.0}]))?;
    let ack = host.tx.set_custom_options(room.clone(), options).await;
    host.record(&json!({"note": format!("set_custom_options: {ack:?}")}))?;
    other.join(&room).await?;
    host.tx
        .set_force_start(room, true)
        .await
        .into_diagnostic()?;
    let mut started = await_game_start(vec![host, other]).await?;
    let mut other = started.pop().expect("two clients started");
    let mut host = started.pop().expect("two clients started");
    let read_other = async {
        loop {
            other.receive().await?;
        }
        #[expect(unreachable_code, reason = "this read loop ends by being dropped")]
        Ok::<(), miette::Report>(())
    };
    tokio::select! {
        result = host.until_turn(12) => result?,
        result = read_other => result?,
    }
    finish(vec![host, other]).await
}

async fn check_accounts() -> Result<()> {
    let ids = std::env::var("GENERALS_IO_PROBE_USER_IDS")
        .into_diagnostic()
        .wrap_err("GENERALS_IO_PROBE_USER_IDS not set")?;
    let client = ClientBuilder::new(Url::parse(ENDPOINT).into_diagnostic()?)
        .transport(TransportStrategy::WebSocket)
        .open()
        .into_diagnostic()?;
    let (socket_tx, _rx) = client
        .connect(ByteString::from_static("/"))
        .await
        .into_diagnostic()?;
    let tx = GeneralsIoSender::new(socket_tx);
    for (position, id) in ids.split(',').enumerate() {
        let username = tx.get_username(id.to_owned()).await.into_diagnostic()?;
        println!("{position}: has username: {}", username.is_some());
    }
    tx.disconnect().await;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let first = arguments.next().ok_or_else(|| miette!("missing out dir"))?;
    if first == "check-accounts" {
        return check_accounts().await;
    }
    let out = PathBuf::from(first);
    std::fs::create_dir_all(&out).into_diagnostic()?;
    let selected: Vec<String> = arguments.collect();
    for name in SCENARIOS {
        if !selected.is_empty() && !selected.iter().any(|s| s == name) {
            continue;
        }
        println!("running {name}");
        if let Err(error) = scenario(name, &out.join(format!("{name}.jsonl"))).await {
            println!("{name} failed: {error:?}");
        }
    }
    Ok(())
}
