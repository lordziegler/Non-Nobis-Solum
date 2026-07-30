//! Reads `data/reference/<profile>/critical_levels.csv` — thresholds to
//! interpret a raw soil test value as low/medium/high, by nutrient,
//! texture and region.
//!
//! The critical-level literature backing this file (Olsen guidelines,
//! Castro-Gomez 2009 Tabla 12) doesn't actually differentiate thresholds
//! by texture, so rows use the sentinel texture `"any"` instead of being
//! duplicated per USDA texture class. A row with an exact texture match
//! wins if one is ever added for a specific class; otherwise lookup falls
//! back to `"any"`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{CriticalLevel, DomainError, Texture};
use crate::core::ports::CriticalLevelsRepository;

const ANY_TEXTURE: &str = "any";

#[derive(Debug, Deserialize)]
struct CriticalLevelRow {
    nutrient_id: String,
    texture: String,
    region: String,
    low_threshold: f64,
    medium_threshold: f64,
    high_threshold: f64,
    source: String,
    year: u16,
}

impl From<CriticalLevelRow> for CriticalLevel {
    fn from(row: CriticalLevelRow) -> Self {
        CriticalLevel {
            low_threshold: row.low_threshold,
            medium_threshold: row.medium_threshold,
            high_threshold: row.high_threshold,
            source: row.source,
            year: row.year,
        }
    }
}

pub struct CsvCriticalLevelsRepo {
    path: PathBuf,
}

impl CsvCriticalLevelsRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl CriticalLevelsRepository for CsvCriticalLevelsRepo {
    fn get_critical_level(&self, nutrient_id: &str, texture: &Texture, region: &str) -> Result<CriticalLevel, DomainError> {
        let texture_str = texture.to_string();
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        let mut any_texture_fallback: Option<CriticalLevelRow> = None;
        for row in reader.deserialize::<CriticalLevelRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.nutrient_id != nutrient_id || row.region != region {
                continue;
            }
            if row.texture == texture_str {
                return Ok(row.into());
            }
            if row.texture == ANY_TEXTURE {
                any_texture_fallback = Some(row);
            }
        }

        any_texture_fallback.map(Into::into).ok_or_else(|| {
            DomainError::NotFound(format!(
                "no critical level for nutrient_id={nutrient_id} texture={texture_str} region={region}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_any_texture_when_no_exact_match_exists() {
        let repo = CsvCriticalLevelsRepo::new("data/reference/global/critical_levels.csv");

        let level = repo.get_critical_level("P", &Texture::Sand, "global").unwrap();

        assert_eq!(level.low_threshold, 10.0);
        assert_eq!(level.medium_threshold, 20.0);
    }

    #[test]
    fn unknown_nutrient_still_errors() {
        let repo = CsvCriticalLevelsRepo::new("data/reference/global/critical_levels.csv");

        let result = repo.get_critical_level("Mo", &Texture::Sand, "global");

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
