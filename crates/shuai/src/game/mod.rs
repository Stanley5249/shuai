//! Validated flat maps and atomic event processing.
//!
//! The wire and diff assumptions are specified by the fixtures in this module's
//! tests. Army and terrain views borrow the one owned raw map buffer.

mod diff;
mod generation;
mod state;

pub use generation::random_connected_map;
pub use state::{GameState, Observation};

use std::ops::Index;

use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

/// The largest integer represented exactly by JavaScript numbers.
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// An invalid map, diff, event value, or coordinate.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid game data: {0}")]
pub struct Error(pub &'static str);

/// A nonnegative player identifier within the client schema's `i32` range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct PlayerId(i32);

impl TryFrom<i32> for PlayerId {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Error> {
        if value < 0 {
            return Err(Error("negative player identifier"));
        }
        Ok(Self(value))
    }
}

impl From<PlayerId> for i32 {
    fn from(value: PlayerId) -> Self {
        value.0
    }
}

/// Terrain codes, with special tile lists carried separately by game events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terrain {
    /// An unoccupied visible tile (`-1`).
    Empty,
    /// An impassable visible mountain (`-2`).
    Mountain,
    /// An unseen tile (`-3`).
    Fog,
    /// An unseen obstacle (`-4`).
    FogObstacle,
    /// A tile owned by the identified player.
    Player(PlayerId),
}

impl TryFrom<i64> for Terrain {
    type Error = Error;

    fn try_from(value: i64) -> Result<Self, Error> {
        match value {
            -1 => Ok(Self::Empty),
            -2 => Ok(Self::Mountain),
            -3 => Ok(Self::Fog),
            -4 => Ok(Self::FogObstacle),
            0..=2_147_483_647 => Ok(Self::Player(PlayerId(
                i32::try_from(value).map_err(|_| Error("player identifier overflow"))?,
            ))),
            _ => Err(Error("unknown terrain code")),
        }
    }
}

impl From<Terrain> for i64 {
    fn from(value: Terrain) -> Self {
        match value {
            Terrain::Empty => -1,
            Terrain::Mountain => -2,
            Terrain::Fog => -3,
            Terrain::FogObstacle => -4,
            Terrain::Player(player) => Self::from(player.0),
        }
    }
}

fn nonnegative(value: i64) -> Result<(), Error> {
    if !(0..=MAX_SAFE_INTEGER).contains(&value) {
        return Err(Error("expected a nonnegative JavaScript safe integer"));
    }
    Ok(())
}

fn dimensions(raw: &[i64]) -> Result<(usize, usize), Error> {
    let [width, height, ..] = raw else {
        return Err(Error("missing map header"));
    };
    nonnegative(*width)?;
    nonnegative(*height)?;
    let width = usize::try_from(*width).map_err(|_| Error("width overflow"))?;
    let height = usize::try_from(*height).map_err(|_| Error("height overflow"))?;
    if width == 0 || height == 0 {
        return Err(Error("map dimensions must be positive"));
    }
    let length = width
        .checked_mul(height)
        .and_then(|size| size.checked_mul(2))
        .and_then(|size| size.checked_add(2))
        .ok_or(Error("map size overflow"))?;
    if raw.len() != length {
        return Err(Error("map length differs from its header"));
    }
    Ok((width, height))
}

fn validate_map(raw: &[i64]) -> Result<(), Error> {
    let (width, height) = dimensions(raw)?;
    let size = width * height;
    for &army in &raw[2..2 + size] {
        nonnegative(army)?;
    }
    for &terrain in &raw[2 + size..] {
        Terrain::try_from(terrain)?;
    }
    Ok(())
}

/// A map stored as `[width, height, armies..., terrain...]`.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Vec<i64>")]
pub struct Map {
    raw: Vec<i64>,
}

impl Serialize for Map {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.raw.serialize(serializer)
    }
}

impl TryFrom<Vec<i64>> for Map {
    type Error = Error;

    fn try_from(raw: Vec<i64>) -> Result<Self, Error> {
        validate_map(&raw)?;
        Ok(Self { raw })
    }
}

impl Map {
    /// Borrows the complete flat map, suitable for ML input adapters.
    #[must_use]
    pub fn as_slice(&self) -> &[i64] {
        &self.raw
    }

    /// Returns the allocated raw capacity in integers.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.raw.capacity()
    }

    /// Borrows row-major army and raw terrain views.
    ///
    /// # Panics
    /// Panics if an internal map invariant has been violated.
    #[must_use]
    pub fn view(&self) -> MapView<'_> {
        let (width, height) = dimensions(&self.raw).expect("Map has a validated header");
        let size = width * height;
        MapView {
            armies: GridView {
                raw: &self.raw[2..2 + size],
                width,
                height,
            },
            terrain: GridView {
                raw: &self.raw[2 + size..],
                width,
                height,
            },
        }
    }

    /// Reads a terrain value after checking both coordinate components.
    ///
    /// # Panics
    /// Panics if an internal map invariant has been violated.
    #[must_use]
    pub fn terrain(&self, row: usize, column: usize) -> Option<Terrain> {
        self.view()
            .terrain
            .get(row, column)
            .map(|&code| Terrain::try_from(code).expect("Map has validated terrain"))
    }

    /// Writes a validated army count at a checked coordinate.
    ///
    /// # Errors
    /// Returns an error for an invalid coordinate or army value.
    pub fn set_army(&mut self, row: usize, column: usize, value: i64) -> Result<(), Error> {
        nonnegative(value)?;
        let index = self.coordinate(row, column)?;
        self.raw[2 + index] = value;
        Ok(())
    }

    /// Writes terrain at a checked coordinate.
    ///
    /// # Errors
    /// Returns an error for an invalid coordinate.
    pub fn set_terrain(&mut self, row: usize, column: usize, value: Terrain) -> Result<(), Error> {
        let index = self.coordinate(row, column)?;
        let size = self.view().terrain.as_slice().len();
        self.raw[2 + size + index] = value.into();
        Ok(())
    }

    fn coordinate(&self, row: usize, column: usize) -> Result<usize, Error> {
        let view = self.view().armies;
        if row >= view.height || column >= view.width {
            return Err(Error("coordinate outside map"));
        }
        Ok(row * view.width + column)
    }
}

/// A borrowed rectangular grid with raw integer indexing.
#[derive(Debug, Clone, Copy)]
pub struct GridView<'a> {
    raw: &'a [i64],
    width: usize,
    height: usize,
}

impl<'a> GridView<'a> {
    /// Returns the number of columns.
    #[must_use]
    pub fn width(self) -> usize {
        self.width
    }

    /// Returns the number of rows.
    #[must_use]
    pub fn height(self) -> usize {
        self.height
    }

    /// Borrows the contiguous row-major values.
    #[must_use]
    pub fn as_slice(self) -> &'a [i64] {
        self.raw
    }

    /// Reads a checked coordinate.
    #[must_use]
    pub fn get(self, row: usize, column: usize) -> Option<&'a i64> {
        (row < self.height && column < self.width).then(|| &self.raw[row * self.width + column])
    }

    /// Borrows one complete row.
    #[must_use]
    pub fn row(self, row: usize) -> Option<&'a [i64]> {
        (row < self.height).then(|| &self.raw[row * self.width..(row + 1) * self.width])
    }

    /// Iterates through complete rows with contiguous columns.
    pub fn rows(self) -> impl Iterator<Item = &'a [i64]> {
        self.raw.chunks_exact(self.width)
    }
}

impl Index<usize> for GridView<'_> {
    type Output = i64;
    fn index(&self, index: usize) -> &Self::Output {
        &self.raw[index]
    }
}

impl Index<(usize, usize)> for GridView<'_> {
    type Output = i64;
    fn index(&self, (row, column): (usize, usize)) -> &Self::Output {
        self.get(row, column)
            .expect("grid coordinate must be in bounds")
    }
}

/// Army and raw terrain views over the same owned map.
#[derive(Debug, Clone, Copy)]
pub struct MapView<'a> {
    /// Borrowed army values.
    pub armies: GridView<'a>,
    /// Borrowed terrain codes.
    pub terrain: GridView<'a>,
}

#[cfg(test)]
mod tests;
