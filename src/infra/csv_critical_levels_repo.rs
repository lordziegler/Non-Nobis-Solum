//! Reads `data/reference/<profile>/critical_levels.csv` — low/medium/high
//! thresholds by nutrient, texture, region and lab extraction method.
//!
//! Three of those four are sentinel axes carrying `"any"`, and one is not:
//!
//! - `texture`: the literature (Castro y Gómez 2009, Tabla 12) does not
//!   differentiate thresholds by USDA class, so every row is `any`.
//! - `region`: an independent knob from `--profile`, and a reference file
//!   already lives inside a profile directory, so its rows answer for
//!   whatever region a lot claims.
//! - `extraction_method`: real for P, where Tabla 12 gives Bray II and
//!   Olsen their own boundaries. `any` elsewhere, because the same table
//!   gives one set of numbers for the exchangeable cations regardless of
//!   which extractant reported them.
//!
//! An exact match wins over a sentinel on every axis.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{CriticalLevel, DomainError, Texture};
use crate::core::ports::CriticalLevelsRepository;

const ANY: &str = "any";

#[derive(Debug, Deserialize)]
struct CriticalLevelRow {
    nutrient_id: String,
    texture: String,
    region: String,
    extraction_method: String,
    unit: String,
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
            unit: row.unit,
            extraction_method: row.extraction_method,
            source: row.source,
            year: row.year,
        }
    }
}

/// Lab method names travel by hand through CSVs written by different
/// people: `Bray II`, `bray_ii` and `Bray-II` are the same extraction.
/// Compared on their letters and digits alone rather than demanding one
/// spelling, which would fail silently — an unmatched method falls through
/// to the `any` row and classifies against the wrong boundaries.
fn same_method(a: &str, b: &str) -> bool {
    let letters = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    letters(a) == letters(b)
}

/// Reads the per-nutrient critical levels from a CSV file.
///
/// Holds only the path: the file is opened per query rather than
/// cached, so an edit made while the app runs is picked up on the next
/// read.
pub struct CsvCriticalLevelsRepo {
    path: PathBuf,
}

impl CsvCriticalLevelsRepo {
    /// Points the repository at the critical levels CSV.
    ///
    /// # Arguments
    /// * `path` — the file to read. Not opened here, so a path that
    ///   does not exist yet is accepted and fails at the first query.
    ///
    /// # Returns
    /// A repository reading that file.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl CriticalLevelsRepository for CsvCriticalLevelsRepo {
    fn get_critical_level(
        &self,
        nutrient_id: &str,
        texture: &Texture,
        region: &str,
        extraction_method: Option<&str>,
    ) -> Result<CriticalLevel, DomainError> {
        let texture_str = texture.to_string();
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        let mut fallback: Option<CriticalLevelRow> = None;
        for row in reader.deserialize::<CriticalLevelRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.nutrient_id != nutrient_id || (row.region != region && row.region != ANY) {
                continue;
            }
            if row.texture != texture_str && row.texture != ANY {
                continue;
            }

            let method_matches = extraction_method.is_some_and(|method| same_method(&row.extraction_method, method));
            if method_matches && row.texture == texture_str {
                return Ok(row.into());
            }
            // A row naming this sample's own extraction beats one that
            // names no method, whatever the texture axis says: the method
            // is the axis the numbers actually differ along.
            if method_matches || (row.extraction_method == ANY && fallback.is_none()) {
                fallback = Some(row);
            }
        }

        fallback.map(Into::into).ok_or_else(|| {
            DomainError::NotFound(format!(
                "no critical level for nutrient_id={nutrient_id} texture={texture_str} region={region} method={}",
                extraction_method.unwrap_or("<unspecified>")
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::SoilStatus;

    fn repo() -> CsvCriticalLevelsRepo {
        CsvCriticalLevelsRepo::new("data/reference/global/critical_levels.csv")
    }

    /// The point of the whole column: Tabla 12 gives P two sets of
    /// boundaries, and the same reading classifies differently under each.
    #[test]
    fn phosphorus_thresholds_follow_the_lab_method_that_produced_the_reading() {
        let bray = repo().get_critical_level("P", &Texture::Loam, "any", Some("Bray II")).expect("Bray II");
        let olsen = repo().get_critical_level("P", &Texture::Loam, "any", Some("Olsen")).expect("Olsen");

        assert_eq!((bray.low_threshold, bray.medium_threshold), (20.0, 40.0));
        assert_eq!((olsen.low_threshold, olsen.medium_threshold), (16.0, 35.0));

        // 18 mg/kg is "low" by Bray II and "medium" by Olsen. Getting this
        // backwards is a real P recommendation moved by a whole tier.
        assert_eq!(bray.classify(18.0), SoilStatus::Low);
        assert_eq!(olsen.classify(18.0), SoilStatus::Medium);
    }

    #[test]
    fn method_names_match_across_spelling() {
        for spelling in ["Bray II", "bray_ii", "Bray-II", "BRAYII"] {
            let level = repo().get_critical_level("P", &Texture::Loam, "any", Some(spelling)).expect(spelling);
            assert_eq!(level.low_threshold, 20.0, "{spelling} did not reach the Bray II row");
        }
    }

    /// A nutrient whose thresholds do not vary by extractant answers for
    /// whatever the lab wrote in the method column.
    #[test]
    fn a_nutrient_with_one_set_of_thresholds_ignores_the_method() {
        let level = repo().get_critical_level("K", &Texture::Sand, "andina_colombia", Some("AcONH4_1N_pH7")).expect("K");

        assert_eq!(level.low_threshold, 0.4);
        assert_eq!(level.unit, "cmolc_per_kg");
    }

    /// Both shipped lots claim `andina_colombia` whatever profile is
    /// chosen; without the region fallback that drops every soil status.
    #[test]
    fn a_region_the_profile_does_not_name_falls_back_to_the_sentinel() {
        let level = repo().get_critical_level("Ca", &Texture::Loam, "andina_colombia", None).expect("Ca");

        assert_eq!(level.low_threshold, 3.0);
    }

    #[test]
    fn unknown_nutrient_still_errors() {
        assert!(matches!(
            repo().get_critical_level("Mo", &Texture::Sand, "global", None),
            Err(DomainError::NotFound(_))
        ));
    }
}
