use std::collections::{HashSet, VecDeque};

use serde_json::{Value, json};
use shuai_client::server::event::{GameStartData, GameUpdateData};

use super::*;

fn start() -> Box<GameStartData> {
    Box::new(
        serde_json::from_value(json!({
            "playerIndex": 0, "chat_room": "offline", "usernames": ["a", "b"],
            "game_type": "custom", "options": {}
        }))
        .unwrap(),
    )
}

fn update(map_diff: Vec<i64>, cities_diff: Vec<i64>) -> GameUpdateData {
    serde_json::from_value(json!({
        "turn": 1, "scores": [{"total": 3, "tiles": 1, "i": 0, "color": 0, "has_kill": false}],
        "attackIndex": -1, "generals": [-1, 0], "map_diff": Value::from(map_diff),
        "cities_diff": Value::from(cities_diff)
    }))
    .unwrap()
}

fn snapshot() -> Vec<i64> {
    vec![3, 2, 0, 1, 2, 3, 4, 5, -1, -2, -3, -4, 0, 1]
}

fn replace(raw: &[i64]) -> Vec<i64> {
    let mut result = vec![0, i64::try_from(raw.len()).unwrap()];
    result.extend_from_slice(raw);
    result
}

#[test]
fn rectangular_views_borrow_one_buffer_and_round_trip() {
    let map = Map::try_from(snapshot()).unwrap();
    let view = map.view();
    assert_eq!(view.armies[4], 4);
    assert_eq!(view.armies[(1, 1)], 4);
    assert_eq!(view.armies.row(1), Some(&[3, 4, 5][..]));
    assert_eq!(
        view.armies.rows().collect::<Vec<_>>(),
        [&[0, 1, 2][..], &[3, 4, 5][..]]
    );
    assert_eq!(
        view.armies.as_slice().as_ptr(),
        map.as_slice()[2..].as_ptr()
    );
    assert_eq!(view.terrain[(1, 0)], -4);
    assert_eq!(
        map.terrain(1, 1),
        Some(Terrain::Player(PlayerId::try_from(0).unwrap()))
    );
    assert_eq!(view.armies.get(0, 3), None);
    assert_eq!(view.armies.get(2, 0), None);
    assert_eq!(view.armies.row(2), None);
    let json = serde_json::to_string(&map).unwrap();
    assert_eq!(serde_json::from_str::<Map>(&json).unwrap(), map);
}

#[test]
fn controlled_mutation_and_numeric_contract() {
    let mut map = Map::try_from(snapshot()).unwrap();
    map.set_army(0, 1, MAX_SAFE_INTEGER).unwrap();
    map.set_army(0, 1, MAX_SAFE_INTEGER + 1).unwrap_err();
    map.set_army(0, 1, -1).unwrap_err();
    map.set_terrain(0, 3, Terrain::Empty).unwrap_err();
    map.set_terrain(1, 2, Terrain::Fog).unwrap();
    assert_eq!(map.terrain(1, 2), Some(Terrain::Fog));
    PlayerId::try_from(-1).unwrap_err();
    serde_json::from_str::<PlayerId>("-1").unwrap_err();
    for code in -4..=2 {
        assert_eq!(i64::from(Terrain::try_from(code).unwrap()), code);
    }
    Terrain::try_from(-5).unwrap_err();
    Terrain::try_from(i64::from(i32::MAX) + 1).unwrap_err();
    for invalid in [
        vec![],
        vec![0, 1],
        vec![1, 0],
        vec![1, 1, 0],
        vec![1, 1, -1, -1],
        vec![i64::MAX, 2],
    ] {
        Map::try_from(invalid).unwrap_err();
    }
    for raw in [
        "[1.0,1,0,-1]",
        "[1,1,0.5,-1]",
        "[1,1,\"0\",-1]",
        "[1,1,9223372036854775808,-1]",
    ] {
        serde_json::from_str::<Map>(raw).unwrap_err();
    }
}

#[test]
fn checked_diff_grammar_handles_growth_shrink_and_terminal_runs() {
    let mut output = Vec::new();
    for (old, diff, expected) in [
        (vec![], vec![], vec![]),
        (vec![1, 2, 3], vec![3], vec![1, 2, 3]),
        (vec![1, 2, 3], vec![1], vec![1]),
        (vec![1, 2, 3], vec![1, 1, 9, 1], vec![1, 9, 3]),
        (vec![1], vec![1, 2, 8, 9], vec![1, 8, 9]),
        (vec![1, 2], vec![0, 0, 2, 0], vec![1, 2]),
    ] {
        diff::patch(&old, &diff, &mut output).unwrap();
        assert_eq!(output, expected);
    }
    for malformed in [
        vec![-1],
        vec![4],
        vec![0, -1],
        vec![0, 2, 1],
        vec![MAX_SAFE_INTEGER + 1],
        vec![0, MAX_SAFE_INTEGER],
    ] {
        diff::patch(&[1, 2, 3], &malformed, &mut output).unwrap_err();
    }
}

#[test]
fn state_is_atomic_reuses_capacity_and_resets() {
    let mut state = GameState::default();
    state
        .update(update(replace(&snapshot()), vec![]))
        .unwrap_err();
    state.start(start()).unwrap();
    assert!(state.map().is_none());
    state
        .update(update(replace(&snapshot()), vec![0, 2, 0, 5]))
        .unwrap();
    let pointer = state.map().unwrap().as_slice().as_ptr();
    let capacity = state.map().unwrap().capacity();
    let mut changed = update(vec![2, 1, 8, 11], vec![1]);
    changed.turn = 2;
    state.update(changed).unwrap();
    assert_eq!(state.map().unwrap().as_slice().as_ptr(), pointer);
    assert_eq!(state.map().unwrap().capacity(), capacity);
    assert_eq!(state.map().unwrap().view().armies[0], 8);
    assert_eq!(state.cities(), [0]);
    let previous = state.map().unwrap().as_slice().to_vec();
    let mut invalid = update(vec![2, 1, 9, 11], vec![0, 1, 6]);
    invalid.turn = 3;
    state.update(invalid).unwrap_err();
    assert_eq!(state.map().unwrap().as_slice(), previous);
    assert_eq!(state.cities(), [0]);
    assert_eq!(state.observation().unwrap().turn, 2);
    let mut invalid = update(vec![14], vec![1]);
    invalid.scores[0].total = -1;
    state.update(invalid).unwrap_err();
    assert_eq!(state.map().unwrap().as_slice(), previous);
    state.start(start()).unwrap();
    assert!(state.map().is_none());
    assert!(state.observation().is_none());
    assert_eq!(state.cities(), &[] as &[i64]);
    state
        .update(update(replace(&[1, 1, 0, -1]), vec![]))
        .unwrap();
    assert_eq!(state.map().unwrap().as_slice(), [1, 1, 0, -1]);
}

#[test]
fn header_changes_optional_fields_and_desert_patches() {
    let mut state = GameState::default();
    let mut metadata = start();
    metadata.deserts = Some(vec![0]);
    state.start(metadata).unwrap();
    let mut first = update(replace(&snapshot()), vec![]);
    first.deserts_diff = Some(vec![1, 1, 5]);
    first.stars = Some(vec![MAX_SAFE_INTEGER]);
    first.alt_stars = Some(vec![None, Some(Value::Bool(true))]);
    state.update(first).unwrap();
    assert_eq!(state.deserts(), Some(&[0, 5][..]));
    assert!(state.observation().unwrap().stars.is_some());
    state.update(update(vec![14], vec![])).unwrap();
    assert!(state.observation().unwrap().stars.is_none());
    assert_eq!(state.deserts(), Some(&[0, 5][..]));
    let mut shrink = update(replace(&[1, 1, 0, -1]), vec![]);
    shrink.deserts_diff = Some(vec![1]);
    state.update(shrink).unwrap();
    assert_eq!(state.deserts(), Some(&[0][..]));
    assert_eq!(state.map().unwrap().view().armies.width(), 1);
}

#[test]
fn every_rejected_update_preserves_all_committed_fields() {
    let mut state = GameState::default();
    state.start(start()).unwrap();
    let mut initial = update(replace(&snapshot()), vec![0, 1, 2]);
    initial.stars = Some(vec![-MAX_SAFE_INTEGER]);
    initial.deserts_diff = Some(vec![0, 1, 4]);
    state.update(initial).unwrap();
    for case in 0..10 {
        let mut event = update(vec![2, 1, 9, 11], vec![1]);
        event.turn = 2;
        match case {
            0 => event.turn = MAX_SAFE_INTEGER + 1,
            1 => event.generals = vec![-2],
            2 => event.generals = vec![6],
            3 => event.attack_index = -2,
            4 => event.raw_visible = Some(vec![MAX_SAFE_INTEGER + 1]),
            5 => event.scores[0].i = -1,
            6 => event.scores[0].neutralize_in = Some(-1),
            7 => event.deserts_diff = Some(vec![0, 1, 6]),
            8 => event.map_diff = vec![0, 2, 1, 1],
            _ => event.map_diff = vec![0, 14, 3, 2, 0, 1, 2, 3, 4, 5, -1, -2, -3, -4, 0, -5],
        }
        state.update(event).unwrap_err();
        assert_eq!(state.map().unwrap().as_slice(), snapshot());
        assert_eq!(state.cities(), [2]);
        assert_eq!(state.deserts(), Some(&[4][..]));
        assert_eq!(state.observation().unwrap().turn, 1);
        assert_eq!(
            state.observation().unwrap().stars,
            Some(vec![-MAX_SAFE_INTEGER])
        );
    }
    let mut invalid_metadata = start();
    invalid_metadata.player_index = -1;
    state.start(invalid_metadata).unwrap_err();
    assert_eq!(state.map().unwrap().as_slice(), snapshot());
}

#[test]
fn seeded_generation_preserves_connected_nonempty_grid() {
    random_connected_map(0, 1, 0).unwrap_err();
    random_connected_map(usize::MAX, 2, 0).unwrap_err();
    assert_eq!(
        random_connected_map(1, 1, 0).unwrap().as_slice(),
        [1, 1, 0, -1]
    );
    assert_eq!(
        random_connected_map(2, 1, 0)
            .unwrap()
            .view()
            .terrain
            .as_slice(),
        [-1, -2]
    );
    for (width, height) in [(1, 8), (8, 1), (2, 3), (9, 5)] {
        for seed in 0..20 {
            let map = random_connected_map(width, height, seed).unwrap();
            assert_eq!(map, random_connected_map(width, height, seed).unwrap());
            let terrain = map.view().terrain;
            let passable: HashSet<_> = terrain
                .as_slice()
                .iter()
                .enumerate()
                .filter_map(|(i, &code)| (code == -1).then_some(i))
                .collect();
            assert!(!passable.is_empty());
            let mut queue = VecDeque::from([*passable.iter().next().unwrap()]);
            let mut visited = HashSet::new();
            while let Some(vertex) = queue.pop_front() {
                if !visited.insert(vertex) {
                    continue;
                }
                for next in [
                    vertex.checked_sub(width),
                    (vertex + width < width * height).then_some(vertex + width),
                    (vertex % width > 0).then(|| vertex - 1),
                    (vertex % width + 1 < width).then_some(vertex + 1),
                ]
                .into_iter()
                .flatten()
                {
                    if passable.contains(&next) {
                        queue.push_back(next);
                    }
                }
            }
            assert_eq!(visited, passable);
        }
    }
}
