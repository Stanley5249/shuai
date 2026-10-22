use serde::{Deserialize, Serialize};
use serde_json::Value;
use shuai_client::server::event::{GameStartData, GameUpdateData, Score};

use super::{Error, Map, PlayerId, nonnegative, validate_map};

/// The current update snapshot. Absent optional fields remain explicitly absent.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observation {
    /// Current turn.
    pub turn: i64,
    /// Per-player scores.
    pub scores: Vec<Score>,
    /// Server attack acknowledgement, with `-1` preserved.
    pub attack_index: i64,
    /// General positions, with undiscovered positions represented by `-1`.
    pub generals: Vec<i64>,
    /// Optional rating information.
    pub stars: Option<Vec<i64>>,
    /// Optional uncertainty information with its original wire representation.
    pub uncertainties: Option<Vec<i64>>,
    /// Optional untyped alternate rating information.
    pub alt_stars: Option<Vec<Option<Value>>>,
    /// Optional raw visibility values with their original wire representation.
    pub raw_visible: Option<Vec<i64>>,
}

/// Owns one game's metadata, reconstructed map, and observation.
///
/// Starting a game resets the complete state. Updates require start metadata.
/// Scratch vectors preserve atomicity and are reused across updates. The owned
/// map retains its allocation on stable-size commits, with disjoint scratch
/// ensuring source and destination never overlap during reconstruction.
#[derive(Debug, Default)]
pub struct GameState {
    start: Option<Box<GameStartData>>,
    map: Option<Map>,
    cities: Vec<i64>,
    deserts: Option<Vec<i64>>,
    observation: Option<Observation>,
    map_scratch: Vec<i64>,
    cities_scratch: Vec<i64>,
    deserts_scratch: Vec<i64>,
}

impl GameState {
    /// Validates metadata and resets observations for the next game atomically.
    ///
    /// # Errors
    /// Returns an error for invalid identifiers or special tile positions.
    pub fn start(&mut self, data: Box<GameStartData>) -> Result<(), Error> {
        PlayerId::try_from(data.player_index)?;
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
        self.start = Some(data);
        self.map = None;
        self.cities.clear();
        self.deserts = None;
        self.observation = None;
        self.map_scratch.clear();
        self.cities_scratch.clear();
        self.deserts_scratch.clear();
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
        super::diff::patch(old, &data.map_diff, &mut self.map_scratch)?;
        validate_map(&self.map_scratch)?;
        let size = (self.map_scratch.len() - 2) / 2;
        super::diff::patch(&self.cities, &data.cities_diff, &mut self.cities_scratch)?;
        positions(&self.cities_scratch, size, false)?;
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
            positions(positions_list, size, false)?;
        }
        if let Some(diff) = &data.deserts_diff {
            let old = self
                .deserts
                .as_deref()
                .or(start.deserts.as_deref())
                .unwrap_or(&[]);
            super::diff::patch(old, diff, &mut self.deserts_scratch)?;
            positions(&self.deserts_scratch, size, false)?;
        } else if let Some(deserts) = &self.deserts {
            positions(deserts, size, false)?;
        }
        nonnegative(data.turn)?;
        sentinel_integer(data.attack_index)?;
        positions(&data.generals, size, true)?;
        for score in &data.scores {
            nonnegative(score.total)?;
            nonnegative(score.tiles)?;
            PlayerId::try_from(score.i)?;
            if let Some(turns) = score.neutralize_in {
                nonnegative(turns)?;
            }
        }
        for values in [&data.stars, &data.uncertainties, &data.raw_visible]
            .into_iter()
            .flatten()
        {
            for &value in values {
                if !(-super::MAX_SAFE_INTEGER..=super::MAX_SAFE_INTEGER).contains(&value) {
                    return Err(Error("optional integer exceeds JavaScript safe range"));
                }
            }
        }

        if let Some(map) = &mut self.map {
            map.raw.clear();
            map.raw.extend_from_slice(&self.map_scratch);
        } else {
            self.map = Some(Map {
                raw: std::mem::take(&mut self.map_scratch),
            });
        }
        self.cities.clear();
        self.cities.extend_from_slice(&self.cities_scratch);
        if data.deserts_diff.is_some() {
            let deserts = self.deserts.get_or_insert_with(Vec::new);
            deserts.clear();
            deserts.extend_from_slice(&self.deserts_scratch);
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

fn sentinel_integer(value: i64) -> Result<(), Error> {
    if value == -1 {
        return Ok(());
    }
    nonnegative(value)
}

fn positions(values: &[i64], size: usize, sentinel: bool) -> Result<(), Error> {
    for &value in values {
        if sentinel && value == -1 {
            continue;
        }
        nonnegative(value)?;
        let index = usize::try_from(value).map_err(|_| Error("position overflow"))?;
        if index >= size {
            return Err(Error("position outside map"));
        }
    }
    Ok(())
}
