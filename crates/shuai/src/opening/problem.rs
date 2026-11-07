//! Builds the solver's input from authoritative game state.

use std::time::Duration;

use shuai_client::constants::INDEX_SPECTATOR;
use shuai_client::server::event::GameStartData;
use shuai_cpsat::opening::{Hints, Problem, SolveOptions};
use thiserror::Error;

use crate::game::{GameState, Map, PlayerId, Terrain};

// The opening schedule, unchanged from the reference formulation.
const RADIUS: usize = 16;
const PATH_COUNT: usize = 5;
const HORIZON: i64 = 50;
const GENERATION_TURNS: i64 = 2;
const SECONDS_PER_TURN_AT_NORMAL_SPEED: f64 = 0.5;
const LAND_HINTS: [i64; PATH_COUNT] = [12, 6, 4, 2, 0];
const TURN_HINTS: [i64; PATH_COUNT] = [12, 8, 4, 2, 0];
/// Added to the budget so the first path cannot be scheduled before the solver returns.
const MINIMUM_LANDS_MARGIN_SECONDS: f64 = 0.1;
const MAX_SOLVER_WORKERS: usize = 8;

/// Why this match cannot use the opening solver.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum UnsupportedGame {
    /// Chess-clock turns advance by different rules.
    #[error("chess-clock games advance turns by different rules")]
    ChessClock,
    /// A spectator cannot move.
    #[error("spectators cannot move")]
    Spectator,
    /// The configured game speed is not a positive finite number.
    #[error("game speed is not a positive finite number")]
    InvalidSpeed,
    /// The update does not show the local player's general.
    #[error("the update does not show the local general")]
    GeneralNotVisible,
}

/// Rejects games whose rules the opening schedule does not cover.
///
/// # Errors
/// Returns the unsupported property of the game.
pub fn check_supported(start: &GameStartData) -> Result<(), UnsupportedGame> {
    if start.player_index == INDEX_SPECTATOR {
        return Err(UnsupportedGame::Spectator);
    }
    if start.chess_clock.is_some() || start.chess_clock_config.is_some() {
        return Err(UnsupportedGame::ChessClock);
    }
    Ok(())
}

/// The solver's search settings for one match.
#[must_use]
pub fn solve_options(budget: Duration) -> SolveOptions {
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    SolveOptions {
        budget,
        workers: available.min(MAX_SOLVER_WORKERS),
        heuristics: true,
        hints: Hints {
            lands: Some(LAND_HINTS.to_vec()),
            turns: Some(TURN_HINTS.to_vec()),
        },
    }
}

/// Builds the opening problem from the first observed map.
///
/// Passable tiles are the unobstructed ones the local player may walk through:
/// cities, swamps, mountains, fog obstacles and other players' tiles are excluded. So
/// are tunnels, because they cap incoming armies, which the schedule does not model.
///
/// # Errors
/// Returns an error if the game's speed or the general's position cannot be used.
pub fn build_problem(state: &GameState, budget: Duration) -> Result<Problem, UnsupportedGame> {
    let start = state.metadata().ok_or(UnsupportedGame::GeneralNotVisible)?;
    let map = state.map().ok_or(UnsupportedGame::GeneralNotVisible)?;
    let observation = state
        .observation()
        .ok_or(UnsupportedGame::GeneralNotVisible)?;
    let general = usize::try_from(start.player_index)
        .ok()
        .and_then(|player| observation.generals.get(player))
        .and_then(|&general| usize::try_from(general).ok())
        .ok_or(UnsupportedGame::GeneralNotVisible)?;
    let speed = start.options.game_speed.unwrap_or(1.0);
    if !speed.is_finite() || speed <= 0.0 {
        return Err(UnsupportedGame::InvalidSpeed);
    }
    let generation_seconds =
        f64::from(u8::try_from(GENERATION_TURNS).expect("the generation period is small"))
            * SECONDS_PER_TURN_AT_NORMAL_SPEED
            / speed;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the budget is seconds and the generation period is positive"
    )]
    let minimum_lands =
        ((budget.as_secs_f64() + MINIMUM_LANDS_MARGIN_SECONDS) / generation_seconds).ceil() as i64;

    let view = map.view();
    let (width, height) = (view.armies.width(), view.armies.height());
    let blocked = blocked_tiles(state, start, map);
    let passable = |tile: usize| {
        tile == general
            || (!blocked.contains(&tile)
                && matches!(
                    map.terrain(tile / width, tile % width),
                    Some(Terrain::Empty | Terrain::Fog)
                ))
    };
    Ok(Problem {
        vertex_count: width * height,
        edges: passable_edges(width, height, passable),
        start: general,
        radius: RADIUS,
        horizon: HORIZON,
        path_count: PATH_COUNT,
        generation_turns: GENERATION_TURNS,
        generation_seconds,
        minimum_lands,
    })
}

fn blocked_tiles(state: &GameState, start: &GameStartData, map: &Map) -> Vec<usize> {
    let tunnels = map
        .view()
        .tunnel_limits
        .into_iter()
        .flat_map(|limits| limits.as_slice().iter().enumerate())
        .filter(|&(_, &limit)| limit != 0)
        .map(|(tile, _)| tile);
    state
        .cities()
        .iter()
        .chain(start.swamps.iter().flatten())
        .filter_map(|&tile| usize::try_from(tile).ok())
        .chain(tunnels)
        .collect()
}

fn passable_edges(
    width: usize,
    height: usize,
    passable: impl Fn(usize) -> bool,
) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let tile = row * width + column;
            if !passable(tile) {
                continue;
            }
            if column + 1 < width && passable(tile + 1) {
                edges.push((tile, tile + 1));
            }
            if row + 1 < height && passable(tile + width) {
                edges.push((tile, tile + width));
            }
        }
    }
    edges
}

/// Reports whether every tile of a solved plan can still be walked on.
#[must_use]
pub fn plan_is_applicable<'a>(
    mut vertices: impl Iterator<Item = &'a usize>,
    map: &Map,
    player: PlayerId,
) -> bool {
    let width = map.view().armies.width();
    vertices.all(|&tile| {
        matches!(
            map.terrain(tile / width, tile % width),
            Some(Terrain::Empty | Terrain::Fog)
        ) || map.terrain(tile / width, tile % width) == Some(Terrain::Player(player))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const BUDGET: Duration = Duration::from_millis(1900);

    fn state(terrain: &[i64], cities: &[i64], start: serde_json::Value) -> GameState {
        let size = terrain.len();
        let mut raw = vec![3, 2];
        raw.extend(std::iter::repeat_n(0, size));
        raw.extend_from_slice(terrain);
        let mut map_diff = vec![0, i64::try_from(raw.len()).unwrap()];
        map_diff.extend(raw);
        let mut cities_diff = vec![0, i64::try_from(cities.len()).unwrap()];
        cities_diff.extend_from_slice(cities);
        let mut game = GameState::default();
        game.start(Box::new(serde_json::from_value(start).unwrap()))
            .unwrap();
        game.update(
            serde_json::from_value(json!({
                "turn": 1, "scores": [], "attackIndex": 0, "generals": [0, -1],
                "map_diff": map_diff, "cities_diff": cities_diff,
                "pings": null, "chessClock": null,
            }))
            .unwrap(),
        )
        .unwrap();
        game
    }

    fn metadata() -> serde_json::Value {
        json!({
            "playerIndex": 0, "chat_room": "room", "usernames": ["a", "b"],
            "game_type": "custom", "options": {}, "replay_id": null, "team_chat_room": null,
            "team_2v2_play_again": null, "big_team_play_again": null,
            "chessClock": null, "chessClockConfig": null,
        })
    }

    #[test]
    fn graph_excludes_obstacles_cities_swamps_and_enemy_tiles() {
        // Row 0: general, fog, mountain. Row 1: fog obstacle, empty, enemy.
        let game = state(&[0, -3, -2, -4, -1, 1], &[], metadata());
        let problem = build_problem(&game, BUDGET).unwrap();
        assert_eq!(problem.start, 0);
        assert_eq!(problem.vertex_count, 6);
        assert_eq!(problem.edges, [(0, 1), (1, 4)]);

        let game = state(&[0, -3, -3, -3, -3, -3], &[4], metadata());
        let problem = build_problem(&game, BUDGET).unwrap();
        assert_eq!(problem.edges, [(0, 1), (0, 3), (1, 2), (2, 5)]);

        let mut swamps = metadata();
        swamps["swamps"] = json!([1]);
        let game = state(&[0, -3, -3, -3, -3, -3], &[], swamps);
        let problem = build_problem(&game, BUDGET).unwrap();
        assert_eq!(problem.edges, [(0, 3), (2, 5), (3, 4), (4, 5)]);
    }

    #[test]
    fn graph_excludes_tunnels_seen_or_under_fog() {
        let mut game = GameState::default();
        game.start(Box::new(serde_json::from_value(metadata()).unwrap()))
            .unwrap();
        let mut map_diff = vec![0, 20, 3, 2];
        map_diff.extend([1, 0, 0, 0, 0, 0]);
        map_diff.extend([0, -3, -3, -3, -3, -3]);
        map_diff.extend([0, 2, 0, 0, -1, 0]);
        game.update(
            serde_json::from_value(json!({
                "turn": 1, "scores": [], "attackIndex": 0, "generals": [0, -1],
                "map_diff": map_diff, "cities_diff": [0, 0],
                "pings": null, "chessClock": null,
            }))
            .unwrap(),
        )
        .unwrap();
        let problem = build_problem(&game, BUDGET).unwrap();
        assert_eq!(problem.edges, [(0, 3), (2, 5)]);
    }

    #[test]
    fn schedule_matches_the_reference_formulation() {
        let game = state(&[0, -3, -3, -3, -3, -3], &[], metadata());
        let problem = build_problem(&game, BUDGET).unwrap();
        assert_eq!(
            (problem.radius, problem.horizon, problem.path_count),
            (16, 50, 5)
        );
        assert_eq!(problem.generation_turns, 2);
        assert!((problem.generation_seconds - 1.0).abs() < f64::EPSILON);
        assert_eq!(problem.minimum_lands, 2);
    }

    #[test]
    fn game_speed_shortens_the_generation_period() {
        let mut fast = metadata();
        fast["options"] = json!({"game_speed": 2.0});
        let game = state(&[0, -3, -3, -3, -3, -3], &[], fast);
        let problem = build_problem(&game, BUDGET).unwrap();
        assert!((problem.generation_seconds - 0.5).abs() < f64::EPSILON);
        assert_eq!(problem.minimum_lands, 4);
    }

    #[test]
    fn unusable_speed_is_rejected() {
        let mut stopped = metadata();
        stopped["options"] = json!({"game_speed": 0.0});
        let game = state(&[0, -3, -3, -3, -3, -3], &[], stopped);
        assert_eq!(
            build_problem(&game, BUDGET).unwrap_err(),
            UnsupportedGame::InvalidSpeed
        );
    }

    #[test]
    fn chess_clock_and_spectator_games_are_unsupported() {
        let mut clock = metadata();
        clock["chessClockConfig"] = json!({
            "clockIncrement": 1.0, "startingClock": 1.0, "decayBufferMs": 1.0,
            "dontLimitMoveSpeed": false,
        });
        let clock: GameStartData = serde_json::from_value(clock).unwrap();
        assert_eq!(check_supported(&clock), Err(UnsupportedGame::ChessClock));

        let mut spectator = metadata();
        spectator["playerIndex"] = json!(-1);
        let spectator: GameStartData = serde_json::from_value(spectator).unwrap();
        assert_eq!(check_supported(&spectator), Err(UnsupportedGame::Spectator));

        let normal: GameStartData = serde_json::from_value(metadata()).unwrap();
        assert_eq!(check_supported(&normal), Ok(()));
    }

    #[test]
    fn plan_is_inapplicable_once_a_tile_is_blocked() {
        let game = state(&[0, -3, -2, -3, -1, 1], &[], metadata());
        let map = game.map().unwrap();
        let player = PlayerId::try_from(0).unwrap();
        assert!(plan_is_applicable([0, 1, 4].iter(), map, player));
        assert!(!plan_is_applicable([0, 1, 2].iter(), map, player));
        assert!(!plan_is_applicable([4, 5].iter(), map, player));
    }
}
