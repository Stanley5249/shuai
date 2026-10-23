use serde::{Deserialize, Serialize};
use serde_json::Value;
use shuai_client::server::event::{
    ChessClockState, GameStartData, GameUpdateData, Score, TilePingData,
};

use super::{Error, MAX_SAFE_INTEGER, Map, PlayerId, dimensions, nonnegative, validate_map};

/// The current update snapshot. Absent optional fields remain explicitly absent.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observation {
    /// Current turn.
    pub turn: i64,
    /// Per-player scores.
    pub scores: Vec<Score>,
    /// Server attack acknowledgement, with `-1` preserved.
    pub attack_index: Option<i64>,
    /// General positions, with undiscovered positions represented by `-1`.
    pub generals: Vec<i64>,
    /// Optional rating information.
    pub stars: Option<Vec<Option<f64>>>,
    /// Optional uncertainty information with its original wire representation.
    pub uncertainties: Option<Vec<Option<f64>>>,
    /// Optional untyped alternate rating information.
    pub alt_stars: Option<Vec<Option<Value>>>,
    /// Optional raw visibility values with their original wire representation.
    pub raw_visible: Option<Vec<i64>>,
    /// Optional pings delivered with this update.
    pub pings: Option<Vec<TilePingData>>,
    /// Optional chess-clock snapshot delivered with this update.
    pub chess_clock: Option<ChessClockState>,
}

/// Owns one game's metadata, reconstructed map, and observation.
///
/// Starting a game resets the complete state. Updates require start metadata.
/// Each update validates its prospective fields before committing them, so a
/// failed update leaves the previous state unchanged.
#[derive(Debug, Default)]
pub struct GameState {
    start: Option<Box<GameStartData>>,
    map: Option<Map>,
    cities: Vec<i64>,
    deserts: Option<Vec<i64>>,
    observation: Option<Observation>,
}

impl GameState {
    /// Validates metadata and resets observations for the next game atomically.
    ///
    /// Accepts [`shuai_client::constants::INDEX_SPECTATOR`] as the local viewer.
    ///
    /// # Errors
    /// Returns an error for invalid player indices, special tile positions outside
    /// the supported nonnegative integer range, or nonfinite clock state or
    /// configuration values.
    pub fn start(&mut self, data: Box<GameStartData>) -> Result<(), Error> {
        if data.player_index != shuai_client::constants::INDEX_SPECTATOR {
            PlayerId::try_from(data.player_index)?;
        }
        for positions in [
            &data.swamps,
            &data.deserts,
            &data.lights,
            &data.lookouts,
            &data.observatories,
        ]
        .into_iter()
        .flatten()
        {
            for &position in positions {
                nonnegative(position)?;
            }
        }
        if let Some(clock) = &data.chess_clock {
            validate_clock(clock)?;
        }
        if let Some(config) = &data.chess_clock_config
            && [
                config.clock_increment,
                config.starting_clock,
                config.decay_buffer_ms,
            ]
            .into_iter()
            .any(|value| !value.is_finite())
        {
            return Err(Error("chess-clock configuration values must be finite"));
        }
        self.start = Some(data);
        self.map = None;
        self.cities.clear();
        self.deserts = None;
        self.observation = None;
        Ok(())
    }

    /// Reconstructs and validates all update fields, then commits them together.
    ///
    /// # Errors
    /// Returns an error if the game has not started, a diff is malformed, or a
    /// resulting value violates the map or observation contract.
    pub fn update(&mut self, data: GameUpdateData) -> Result<(), Error> {
        let start = self
            .start
            .as_ref()
            .ok_or(Error("update requires game start"))?;
        let old = self.map.as_ref().map_or(&[][..], Map::as_slice);
        let raw = super::diff::patch(old, &data.map_diff)?;
        validate_map(&raw)?;
        let (width, height) = dimensions(&raw)?;
        let size = width * height;
        let cities = super::diff::patch(&self.cities, &data.cities_diff)?;
        positions(&cities, size)?;
        for positions_list in [
            &start.swamps,
            &start.deserts,
            &start.lights,
            &start.lookouts,
            &start.observatories,
        ]
        .into_iter()
        .flatten()
        {
            positions(positions_list, size)?;
        }
        let deserts = match &data.deserts_diff {
            Some(diff) => {
                let old = self
                    .deserts
                    .as_deref()
                    .or(start.deserts.as_deref())
                    .unwrap_or(&[]);
                Some(super::diff::patch(old, diff)?)
            }
            None => None,
        };
        if let Some(deserts) = deserts.as_ref().or(self.deserts.as_ref()) {
            positions(deserts, size)?;
        }
        validate_observation(&data, size)?;

        self.map = Some(Map { raw });
        self.cities = cities;
        if deserts.is_some() {
            self.deserts = deserts;
        }
        self.observation = Some(Observation {
            turn: data.turn,
            scores: data.scores,
            attack_index: data.attack_index,
            generals: data.generals,
            stars: data.stars,
            uncertainties: data.uncertainties,
            alt_stars: data.alt_stars,
            raw_visible: data.raw_visible,
            pings: data.pings,
            chess_clock: data.chess_clock,
        });
        Ok(())
    }

    /// Borrows the current game's metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&GameStartData> {
        self.start.as_deref()
    }

    /// Borrows the map after the first successful update.
    #[must_use]
    pub fn map(&self) -> Option<&Map> {
        self.map.as_ref()
    }

    /// Borrows the reconstructed city list.
    #[must_use]
    pub fn cities(&self) -> &[i64] {
        &self.cities
    }

    /// Borrows the current desert list, falling back to start metadata.
    #[must_use]
    pub fn deserts(&self) -> Option<&[i64]> {
        self.deserts
            .as_deref()
            .or_else(|| self.start.as_ref()?.deserts.as_deref())
    }

    /// Borrows the latest observation.
    #[must_use]
    pub fn observation(&self) -> Option<&Observation> {
        self.observation.as_ref()
    }
}

fn validate_observation(data: &GameUpdateData, size: usize) -> Result<(), Error> {
    nonnegative(data.turn)?;
    if let Some(index) = data.attack_index {
        sentinel_integer(index)?;
    }
    for &general in &data.generals {
        if general != -1 {
            position(general, size)?;
        }
    }
    for score in &data.scores {
        sentinel_integer(score.total)?;
        sentinel_integer(score.tiles)?;
        PlayerId::try_from(score.i)?;
        if let Some(progress) = score.neutralize_in
            && (!progress.is_finite() || progress < 0.0)
        {
            return Err(Error(
                "neutralization progress must be finite and nonnegative",
            ));
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "2^53 - 1 is exact in f64")]
    let max_rating = MAX_SAFE_INTEGER as f64;
    for values in [&data.stars, &data.uncertainties].into_iter().flatten() {
        if values
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || value.abs() > max_rating)
        {
            return Err(Error("rating values exceed the finite safe numeric range"));
        }
    }
    if let Some(values) = &data.raw_visible {
        for &value in values {
            if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) {
                return Err(Error("optional integer exceeds JavaScript safe range"));
            }
        }
    }

    if let Some(pings) = &data.pings {
        for ping in pings {
            position(i64::from(ping.tile_index), size)?;
            if let Some(player) = ping.player_index {
                PlayerId::try_from(player)?;
            }
        }
    }
    if let Some(clock) = &data.chess_clock {
        validate_clock(clock)?;
    }

    Ok(())
}

fn validate_clock(clock: &ChessClockState) -> Result<(), Error> {
    if clock.player_clocks.iter().any(|value| !value.is_finite())
        || clock
            .decay_starts_at
            .is_some_and(|value| !value.is_finite())
    {
        return Err(Error("chess-clock values must be finite"));
    }
    for &player in &clock.running_players {
        PlayerId::try_from(player)?;
    }
    Ok(())
}

fn sentinel_integer(value: i64) -> Result<(), Error> {
    if value == -1 {
        return Ok(());
    }
    nonnegative(value)
}

fn positions(values: &[i64], size: usize) -> Result<(), Error> {
    values.iter().try_for_each(|&value| position(value, size))
}

fn position(value: i64, size: usize) -> Result<(), Error> {
    nonnegative(value)?;
    let index = usize::try_from(value).map_err(|_| Error("position overflow"))?;
    if index >= size {
        return Err(Error("position outside map"));
    }
    Ok(())
}
