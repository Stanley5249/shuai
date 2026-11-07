//! The concrete Opening bot: solve once, execute, surrender, and await `game_over`.
//!
//! [`play_opening`] is a plain async function that Session calls for one match. It
//! owns a fresh [`GameState`](crate::game::GameState), solver job, and plan. The
//! execution rules and their evidence are in `docs/protocol/attack-queue.md`.

mod execution;
mod play;
mod problem;
mod solve;

pub use play::{OpeningConfig, play_opening, play_opening_with};

#[cfg(test)]
mod tests;
