//! Game data processing, the connection lifecycle, and concrete gameplay.

pub mod game;
#[cfg(feature = "opening")]
pub mod opening;
pub mod session;

#[cfg(test)]
mod test_support;

/// Convenience exports for game building blocks and the session lifecycle.
pub mod prelude {
    pub use crate::game::{
        GameState, GridView, MAX_SAFE_INTEGER, Map, MapView, Observation, PlayerId, Terrain,
    };
    #[cfg(feature = "opening")]
    pub use crate::opening::{OpeningConfig, play_opening, play_opening_with};
    pub use crate::session::{
        LifecycleError, MatchContext, MatchError, QueueMode, Session, SignalError, SurrenderOnce,
        cancel_on_ctrl_c, next_event,
    };
}
