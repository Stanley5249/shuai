//! The scripted games, each ending in a surrender.

use std::path::Path;
use std::time::{Duration, Instant};

use miette::{IntoDiagnostic, Result, miette};
use serde_json::json;
use shuai::game::GameState;
use shuai_client::prelude::*;

use crate::START_TURN;
use crate::probe::{Probe, await_game_start, finish, start_game};

pub(crate) async fn scenario(name: &str, path: &Path) -> Result<()> {
    if name == "three" {
        return three_players(path).await;
    }
    if name == "rematch" {
        return rematch(path).await;
    }
    if name == "rematch_late" {
        return rematch_late(path).await;
    }
    if name == "leave_after_game" {
        return leave_after_game(path).await;
    }
    if name == "disconnect_after_game" {
        return disconnect_after_game(path).await;
    }
    if name == "leave_during_game" {
        return leave_during_game(path).await;
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

/// Plays until both clients saw `game_over` after the first one surrendered.
async fn finished_game(path: &Path) -> Result<Vec<Probe>> {
    let mut probes = start_game(path, &["player", "other"]).await?;
    probes[0].until_turn(6).await?;
    probes[0].surrender().await?;
    for probe in &mut probes {
        probe.wait_for("game_over", Duration::from_secs(10)).await?;
    }
    Ok(probes)
}

/// Records everything a client receives for `wait`.
async fn observe(probe: &mut Probe, wait: Duration) -> Result<()> {
    probe.wait_for("nothing is named this", wait).await?;
    Ok(())
}

/// Both clients vote well after the game ended, then wait for any answer.
async fn rematch_late(path: &Path) -> Result<()> {
    let mut probes = finished_game(path).await?;
    let mut other = probes.pop().expect("two clients played");
    let mut player = probes.pop().expect("two clients played");
    tokio::try_join!(
        observe(&mut player, Duration::from_secs(2)),
        observe(&mut other, Duration::from_secs(2)),
    )?;
    for probe in [&mut player, &mut other] {
        probe.record_command("rematch", &json!([true]))?;
        probe.tx.rematch(true).await.into_diagnostic()?;
    }
    tokio::try_join!(
        observe(&mut player, Duration::from_secs(15)),
        observe(&mut other, Duration::from_secs(15)),
    )?;
    player.close().await?;
    other.close().await
}

/// What a client sees when its opponent leaves a finished game.
async fn leave_after_game(path: &Path) -> Result<()> {
    let mut probes = finished_game(path).await?;
    let mut other = probes.pop().expect("two clients played");
    let mut player = probes.pop().expect("two clients played");
    other.leave_game().await?;
    observe(&mut player, Duration::from_secs(5)).await?;
    player.close().await?;
    other.close().await
}

/// What a client sees when its opponent disconnects from a finished game.
async fn disconnect_after_game(path: &Path) -> Result<()> {
    let mut probes = finished_game(path).await?;
    let other = probes.pop().expect("two clients played");
    let mut player = probes.pop().expect("two clients played");
    player.record(&json!({"note": "opponent disconnecting"}))?;
    other.close().await?;
    observe(&mut player, Duration::from_secs(8)).await?;
    player.close().await
}

/// What a client sees when its opponent leaves during the game.
async fn leave_during_game(path: &Path) -> Result<()> {
    let mut probes = start_game(path, &["player", "other"]).await?;
    let mut other = probes.pop().expect("two clients played");
    let mut player = probes.pop().expect("two clients played");
    player.until_turn(6).await?;
    other.leave_game().await?;
    player
        .wait_for("game_over", Duration::from_secs(10))
        .await?;
    observe(&mut player, Duration::from_secs(3)).await?;
    player.close().await?;
    other.close().await
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
    host.set_force_start(&room).await?;
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
