//! Reads `data/reference/<profile>/soil_quality_thresholds.csv` — the
//! qualitative half of Tabla 12 plus Tabla 4, as named bands over numeric
//! intervals.
//!
//! Six properties key on `climate_zone: any`; only organic matter varies
//! by thermal belt, and it varies enough that the same 3% reads as *high*
//! in the lowlands and *very low* above 2000 m.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{DomainError, QualitativeBand};
use crate::core::ports::SoilQualityThresholdsRepository;

const ANY: &str = "any";

#[derive(Debug, Deserialize)]
struct BandRow {
    property: String,
    climate_zone: String,
    category: String,
    /// Empty means unbounded on that side: the outermost band of every
    /// one of these tables is open-ended.
    min_value: Option<f64>,
    max_value: Option<f64>,
    unit: String,
    source: String,
    year: u16,
}

pub struct CsvSoilQualityThresholdsRepo {
    path: PathBuf,
}

impl CsvSoilQualityThresholdsRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl SoilQualityThresholdsRepository for CsvSoilQualityThresholdsRepo {
    fn bands(&self, property: &str, climate_zone: &str) -> Result<Vec<QualitativeBand>, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        let mut bands = Vec::new();
        for row in reader.deserialize::<BandRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.property != property || (row.climate_zone != climate_zone && row.climate_zone != ANY) {
                continue;
            }
            bands.push(QualitativeBand {
                property: row.property,
                category: row.category,
                min_value: row.min_value,
                max_value: row.max_value,
                unit: row.unit,
                source: row.source,
                year: row.year,
            });
        }
        Ok(bands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::services::classify_band;

    fn repo() -> CsvSoilQualityThresholdsRepo {
        CsvSoilQualityThresholdsRepo::new("data/reference/global/soil_quality_thresholds.csv")
    }

    fn category_of(property: &str, zone: &str, value: f64) -> Option<String> {
        let bands = repo().bands(property, zone).expect("bands");
        classify_band(&bands, value).map(|band| band.category.clone())
    }

    #[test]
    fn the_ph_bands_cover_the_line_with_no_seam_and_no_gap() {
        let bands = repo().bands("ph", "any").expect("ph bands");
        assert_eq!(bands.len(), 9);
        // Every tenth from 3.0 to 9.9 lands in exactly one band: a gap
        // would leave a real reading uninterpreted, an overlap would make
        // the answer depend on row order.
        for step in 30..=99 {
            let ph = f64::from(step) / 10.0;
            let matches = bands.iter().filter(|band| band.contains(ph)).count();
            assert_eq!(matches, 1, "pH {ph} matched {matches} bands");
        }
        assert_eq!(category_of("ph", "any", 4.4).as_deref(), Some("extremely_acid"));
        assert_eq!(category_of("ph", "any", 6.3).as_deref(), Some("slightly_acid"));
        assert_eq!(category_of("ph", "any", 7.0).as_deref(), Some("neutral"));
    }

    /// Tabla 4's whole point, and the reason `climate_zone` is a lookup
    /// axis rather than a note: one figure, three verdicts.
    #[test]
    fn the_same_organic_matter_reads_differently_in_each_thermal_belt() {
        assert_eq!(category_of("organic_matter", "warm", 3.0).as_deref(), Some("high"));
        assert_eq!(category_of("organic_matter", "temperate", 3.0).as_deref(), Some("sufficient"));
        assert_eq!(category_of("organic_matter", "cold", 3.0).as_deref(), Some("low"));
    }

    /// Tabla 12 gives Ca:Mg an ideal band of 3-5 and a magnesium-deficient
    /// one above 10, and names nothing between. A reading of 7 has to come
    /// back unclassified rather than be rounded into whichever is nearer.
    #[test]
    fn a_value_in_a_gap_the_source_table_never_named_stays_unclassified() {
        assert_eq!(category_of("ca_to_mg", "any", 4.0).as_deref(), Some("ideal"));
        assert_eq!(category_of("ca_to_mg", "any", 12.0).as_deref(), Some("magnesium_deficient"));
        assert_eq!(category_of("ca_to_mg", "any", 7.0), None);
        // Below 1 the table does name one: an inverted ratio is a
        // magnesic soil, i.e. calcium deficient.
        assert_eq!(category_of("ca_to_mg", "any", 0.8).as_deref(), Some("calcium_deficient"));
    }

    #[test]
    fn an_unknown_property_is_empty_rather_than_an_error() {
        assert!(repo().bands("nothing_like_this", "any").expect("no error").is_empty());
    }
}
