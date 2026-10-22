//! Reusable game data processing, independent of the bot session.

pub mod game;

/// Convenience exports for game building blocks.
pub mod prelude {
    pub use crate::game::{
        GameState, GridView, MAX_SAFE_INTEGER, Map, MapView, Observation, PlayerId, Terrain,
        random_connected_map,
    };
}
