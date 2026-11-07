//! Decides attacks from server observations, without any I/O.
//!
//! The rules and their evidence are in `docs/protocol/attack-queue.md`.

use shuai_cpsat::opening::Path;

/// How many ticks a submitted move may stay unacknowledged. One tick is normal, and
/// the rest tolerates missed ticks.
const STALL_TICKS: i64 = 4;

/// The server-observed values one decision needs.
pub struct Progress<O> {
    /// The turn the update reports.
    pub turn: i64,
    /// The inbound attack index, which may be absent or `-1`.
    pub attack_index: Option<i64>,
    /// Whether the reconstructed map shows a tile as owned by the local player.
    pub owned: O,
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
    /// The server took the move from its queue, but the destination is not ours.
    MoveFailed,
    /// The server has not acknowledged the move within the stall bound.
    Unacknowledged,
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
}

/// Tracks one match's plan, with at most one move in flight.
#[derive(Debug)]
pub struct Execution {
    moves: Vec<PlannedMove>,
    next_move: usize,
    submitted_turn: Option<i64>,
}

impl Execution {
    /// Flattens solver paths into moves, each gated by its path's start turn.
    #[must_use]
    pub fn new(paths: &[Path]) -> Self {
        let moves = paths
            .iter()
            .flat_map(|path| {
                path.vertices.windows(2).map(|pair| PlannedMove {
                    from: pair[0],
                    to: pair[1],
                    earliest_turn: path.start_turn,
                })
            })
            .collect();
        Self {
            moves,
            next_move: 0,
            submitted_turn: None,
        }
    }

    /// Applies one observation. Repeated turns are safe, so heartbeats need no filtering.
    pub fn observe(&mut self, progress: &Progress<impl Fn(usize) -> bool>) -> Step {
        if let Some(submitted_turn) = self.submitted_turn {
            let attack_index = i64::try_from(self.next_move + 1).expect("move count fits i64");
            if progress.attack_index.unwrap_or(-1) < attack_index {
                return if progress.turn - submitted_turn > STALL_TICKS {
                    Step::Deviated(Deviation::Unacknowledged)
                } else {
                    Step::Wait
                };
            }
            if !(progress.owned)(self.moves[self.next_move].to) {
                return Step::Deviated(Deviation::MoveFailed);
            }
            self.next_move += 1;
            self.submitted_turn = None;
        }
        let Some(planned) = self.moves.get(self.next_move) else {
            return Step::Complete;
        };
        if progress.turn < planned.earliest_turn {
            return Step::Wait;
        }
        self.submitted_turn = Some(progress.turn);
        Step::Attack(PlannedAttack {
            from: planned.from,
            to: planned.to,
            attack_index: i32::try_from(self.next_move + 1).expect("move count fits i32"),
        })
    }
}

#[cfg(test)]
mod tests {
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
        owned: Vec<usize>,
        expect: Value,
    }

    fn expected_step(expect: &Value) -> Step {
        match expect {
            Value::String(step) if step == "wait" => Step::Wait,
            Value::String(step) if step == "complete" => Step::Complete,
            Value::String(step) if step == "move_failed" => Step::Deviated(Deviation::MoveFailed),
            Value::String(step) if step == "unacknowledged" => {
                Step::Deviated(Deviation::Unacknowledged)
            }
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
                owned: |tile| observation.owned.contains(&tile),
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
        index_gap_proves_the_in_flight_move,
        discarded_move_advances_the_index,
        cleared_move_is_never_acknowledged,
        last_move_unobserved_never_completes,
        missed_tick_is_survivable,
        revisit_of_owned_tile,
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
            owned: |_| false,
        });
        assert_eq!(step, Step::Complete);
    }
}
