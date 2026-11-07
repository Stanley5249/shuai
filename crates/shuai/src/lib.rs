//! Game data processing, the connection lifecycle, and concrete gameplay.

pub mod game;
pub mod opening;
pub mod session;

#[cfg(test)]
mod test_support;

/// Convenience exports for game building blocks and the session lifecycle.
pub mod prelude {
    pub use crate::game::{
        GameState, GridView, MAX_SAFE_INTEGER, Map, MapView, Observation, PlayerId, Terrain,
        random_connected_map,
    };
    pub use crate::opening::{OpeningConfig, play_opening, play_opening_with};
    pub use crate::session::{
        GameMode, LifecycleError, MatchError, Session, SignalError, cancel_on_ctrl_c, next_event,
    };
}
