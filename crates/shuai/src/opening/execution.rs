//! Decides attacks from server observations, without any I/O.
//!
//! The rules and their evidence are in `docs/protocol/attack-queue.md`.

use shuai_cpsat::opening::Path;

/// How many ticks a submitted move may stay unacknowledged. One tick is normal, and
/// the rest tolerates missed ticks.
const STALL_TICKS: i64 = 4;

/// What the latest update shows about one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Whether the local player owns the tile.
    pub owned: bool,
    /// The visible army count, which is zero for an unseen tile.
    pub army: i64,
}

/// The server-observed values one decision needs.
pub struct Progress<T> {
    /// The turn the update reports.
    pub turn: i64,
    /// The inbound attack index, which may be absent or `-1`.
    pub attack_index: Option<i64>,
    /// Reads a tile from the reconstructed map.
    pub tile: T,
}

/// One attack to submit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedAttack {
    /// The source tile.
    pub from: usize,
    /// The destination tile.
    pub to: usize,
    /// The outbound index, unique within the match.
    pub attack_index: i32,
}

/// Why a plan can no longer be proved complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deviation {
    /// The server took the move from its queue, but the destination gained nothing.
    MoveFailed,
    /// The server has not acknowledged the move within the stall bound.
    Unacknowledged,
    /// The inbound index is beyond anything submitted, so its meaning is unknown.
    UnexplainedIndex,
    /// The next move's source is not owned.
    SourceLost,
    /// A path ran out of armies with captures still ahead, and only a general
    /// generates more before the opening ends.
    TooFewArmies,
}

/// What to do after one observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing can be decided yet.
    Wait,
    /// Submit this attack. It will not be returned again.
    Attack(PlannedAttack),
    /// Every planned move is proved by server observations.
    Complete,
    /// Stop the plan. A deviated move is never replayed.
    Deviated(Deviation),
}

#[derive(Debug)]
struct PlannedMove {
    from: usize,
    to: usize,
    earliest_turn: i64,
    starts_path: bool,
    /// The path's tiles after this move's source.
    ahead: Vec<usize>,
}

#[derive(Debug)]
struct InFlight {
    submitted_turn: i64,
    destination_army: i64,
}

/// Tracks one match's plan, with at most one move in flight.
#[derive(Debug)]
pub struct Execution {
    moves: Vec<PlannedMove>,
    next_move: usize,
    in_flight: Option<InFlight>,
}

impl Execution {
    /// Flattens solver paths into moves, each gated by its path's start turn.
    #[must_use]
    pub fn new(paths: &[Path]) -> Self {
        let mut moves = Vec::new();
        for path in paths {
            for (position, pair) in path.vertices.windows(2).enumerate() {
                moves.push(PlannedMove {
                    from: pair[0],
                    to: pair[1],
                    earliest_turn: path.start_turn,
                    starts_path: position == 0,
                    ahead: path.vertices[position + 1..].to_vec(),
                });
            }
        }
        Self {
            moves,
            next_move: 0,
            in_flight: None,
        }
    }

    /// Applies one observation. Repeated turns are safe, so heartbeats need no filtering.
    pub fn observe(&mut self, progress: &Progress<impl Fn(usize) -> Tile>) -> Step {
        if let Some(in_flight) = &self.in_flight {
            let attack_index = i64::try_from(self.next_move + 1).expect("move count fits i64");
            let observed = progress.attack_index.unwrap_or(-1);
            if observed > attack_index {
                return Step::Deviated(Deviation::UnexplainedIndex);
            }
            if observed < attack_index {
                return if progress.turn - in_flight.submitted_turn > STALL_TICKS {
                    Step::Deviated(Deviation::Unacknowledged)
                } else {
                    Step::Wait
                };
            }
            let destination = (progress.tile)(self.moves[self.next_move].to);
            if !destination.owned || destination.army <= in_flight.destination_army {
                return Step::Deviated(Deviation::MoveFailed);
            }
            self.next_move += 1;
            self.in_flight = None;
        }
        loop {
            let Some(planned) = self.moves.get(self.next_move) else {
                return Step::Complete;
            };
            if progress.turn < planned.earliest_turn {
                return Step::Wait;
            }
            let source = (progress.tile)(planned.from);
            if !source.owned {
                return Step::Deviated(Deviation::SourceLost);
            }
            let captures_ahead = planned
                .ahead
                .iter()
                .filter(|&&tile| !(progress.tile)(tile).owned)
                .count();
            if planned.starts_path {
                // A late departure leaves fewer armies for the next path, so a path
                // waits for its general. The plan's own schedule never waits here.
                let required = i64::try_from(captures_ahead).expect("path length fits i64") + 1;
                if source.army < required.max(2) {
                    return Step::Wait;
                }
            } else if source.army < 2 {
                if captures_ahead > 0 {
                    return Step::Deviated(Deviation::TooFewArmies);
                }
                // The rest of this path only crosses owned land. The move cannot run
                // and would gain nothing, so nothing is left to prove.
                self.next_move += 1;
                continue;
            }
            self.in_flight = Some(InFlight {
                submitted_turn: progress.turn,
                destination_army: (progress.tile)(planned.to).army,
            });
            return Step::Attack(PlannedAttack {
                from: planned.from,
                to: planned.to,
                attack_index: i32::try_from(self.next_move + 1).expect("move count fits i32"),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::Deserialize;
    use serde_json::Value;

    use super::*;

    #[derive(Deserialize)]
    struct PlannedPath {
        start_turn: i64,
        vertices: Vec<usize>,
    }

    #[derive(Deserialize)]
    struct Header {
        evidence: String,
        plan: Vec<PlannedPath>,
    }

    #[derive(Deserialize)]
    struct Observation {
        turn: i64,
        attack_index: Option<i64>,
        /// Owned tiles and their armies. Every other tile is unowned with no army.
        owned: HashMap<usize, i64>,
        expect: Value,
    }

    fn expected_step(expect: &Value) -> Step {
        match expect {
            Value::String(step) => match step.as_str() {
                "wait" => Step::Wait,
                "complete" => Step::Complete,
                "move_failed" => Step::Deviated(Deviation::MoveFailed),
                "unacknowledged" => Step::Deviated(Deviation::Unacknowledged),
                "unexplained_index" => Step::Deviated(Deviation::UnexplainedIndex),
                "source_lost" => Step::Deviated(Deviation::SourceLost),
                "too_few_armies" => Step::Deviated(Deviation::TooFewArmies),
                other => panic!("unknown expected step {other}"),
            },
            Value::Array(attack) => Step::Attack(PlannedAttack {
                from: usize::try_from(attack[0].as_u64().unwrap()).unwrap(),
                to: usize::try_from(attack[1].as_u64().unwrap()).unwrap(),
                attack_index: i32::try_from(attack[2].as_i64().unwrap()).unwrap(),
            }),
            other => panic!("unknown expected step {other}"),
        }
    }

    fn replay(name: &str, fixture: &str) {
        let mut lines = fixture.lines();
        let header: Header = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert!(
            ["L", "O"].contains(&header.evidence.as_str()),
            "{name} needs an evidence level"
        );
        let paths: Vec<Path> = header
            .plan
            .into_iter()
            .map(|path| Path {
                start_turn: path.start_turn,
                duration: 0,
                vertices: path.vertices,
            })
            .collect();
        let mut execution = Execution::new(&paths);
        for (number, line) in lines.enumerate() {
            let observation: Observation = serde_json::from_str(line).unwrap();
            let progress = Progress {
                turn: observation.turn,
                attack_index: observation.attack_index,
                tile: |tile| Tile {
                    owned: observation.owned.contains_key(&tile),
                    army: observation.owned.get(&tile).copied().unwrap_or(0),
                },
            };
            assert_eq!(
                execution.observe(&progress),
                expected_step(&observation.expect),
                "{name}, observation {number}"
            );
        }
    }

    macro_rules! fixture_tests {
        ($($name:ident,)*) => {
            $(#[test]
            fn $name() {
                replay(
                    stringify!($name),
                    include_str!(concat!("fixtures/", stringify!($name), ".jsonl")),
                );
            })*
        };
    }

    fixture_tests! {
        attack_order_and_index_progression,
        heartbeat_same_turn_updates,
        missing_and_repeated_indices,
        unexplained_index_stops_the_plan,
        discarded_move_advances_the_index,
        discarded_move_onto_owned_land_is_not_proved,
        cleared_move_is_never_acknowledged,
        last_move_unobserved_never_completes,
        missed_tick_is_survivable,
        revisit_of_owned_tile,
        late_departure_waits_for_armies,
        path_short_of_armies_stops_the_plan,
        path_tail_over_owned_land_is_skipped,
        lost_source_stops_the_plan,
    }

    #[test]
    fn a_plan_without_moves_is_complete() {
        let paths = [Path {
            start_turn: 2,
            duration: 0,
            vertices: vec![0],
        }];
        let step = Execution::new(&paths).observe(&Progress {
            turn: 0,
            attack_index: None,
            tile: |_| Tile {
                owned: false,
                army: 0,
            },
        });
        assert_eq!(step, Step::Complete);
    }
}
