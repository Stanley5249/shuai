//! Validated flat maps and atomic event processing.
//!
//! The wire and diff assumptions are specified by the fixtures in this module's
//! tests. Army and terrain views borrow the one owned raw map buffer.
//!
//! [`Map::view`] exposes army and raw terrain grids with linear and `(row, col)`
//! indexing. Each grid also provides checked getters, row slices, and iterators.
//! Columns are contiguous in memory. [`Map::terrain`] decodes a [`Terrain`] by
//! value, while controlled setters validate updates to the same raw buffer.
//! Raw slices are read-only, so callers preserve map invariants.
//!
//! [`GameState::update`] rebuilds the prospective map, cities, and deserts as
//! locals, validates the complete update, then commits everything together. A
//! failed update leaves the previous state unchanged.

mod diff;
#[cfg(test)]
mod generation;
mod map;
mod state;

pub use state::{GameState, Observation};

pub use map::{Error, GridView, MAX_SAFE_INTEGER, Map, MapView, PlayerId, Terrain};
#[cfg(test)]
mod tests;
