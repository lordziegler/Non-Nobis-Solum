//! Reads `data/reference/<profile>/nutrient_removal.csv` — crop nutrient
//! coefficients from the agronomic literature.
//!
//! Two coefficients per row, because the source tables (Tabla 10/11) give
//! two: what leaves the field in the harvest (*extracción*) and what the
//! whole plant takes up over the cycle (*absorción total*). A blank cell
//! is a dash in the table, meaning that study did not report that basis —
//! never a zero, and never to be filled in from the other column.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{DomainError, RemovalReference};
use crate::core::ports::NutrientRemovalRepository;

#[derive(Debug, Deserialize, Clone)]
struct RemovalRow {
    crop_id: String,
    harvested_organ: String,
    yield_unit: String,
    nutrient_id: String,
    /// `Option` maps an empty CSV cell to `None` rather than refusing the
    /// row, which is what carries "the table prints a dash here" through.
    extraction_kg_per_unit: Option<f64>,
    absorption_kg_per_unit: Option<f64>,
    source: String,
    region: String,
    year: u16,
    dataset_version: String,
}

/// Reads the crop nutrient-removal coefficients from a CSV file.
///
/// Holds only the path: the file is opened per query rather than
/// cached, so an edit made while the app runs is picked up on the next
/// read.
pub struct CsvNutrientRemovalRepo {
    path: PathBuf,
}

impl CsvNutrientRemovalRepo {
    /// Points the repository at the nutrient removal CSV.
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

impl NutrientRemovalRepository for CsvNutrientRemovalRepo {
    fn describe_removal(&self, crop_id: &str, nutrient_id: &str) -> Result<RemovalReference, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        for row in reader.deserialize::<RemovalRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.crop_id == crop_id && row.nutrient_id == nutrient_id {
                return Ok(RemovalReference {
                    extraction_kg_per_unit: row.extraction_kg_per_unit,
                    absorption_kg_per_unit: row.absorption_kg_per_unit,
                    harvested_organ: row.harvested_organ,
                    yield_unit: row.yield_unit,
                    source: row.source,
                    region: row.region,
                    year: row.year,
                    dataset_version: row.dataset_version,
                });
            }
        }

        Err(DomainError::NotFound(format!(
            "no removal coefficient for crop_id={crop_id} nutrient_id={nutrient_id}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::NutrientDemandMode;

    fn repo() -> CsvNutrientRemovalRepo {
        CsvNutrientRemovalRepo::new("data/reference/global/nutrient_removal.csv")
    }

    #[test]
    fn both_bases_of_a_row_survive_the_read() {
        // Tabla 10, maíz: extracción 15 kg N/t, absorción total 24 kg N/t.
        let reference = repo().describe_removal("corn", "N").expect("corn N");

        assert_eq!(reference.coefficient(NutrientDemandMode::Extraction), Some(15.0));
        assert_eq!(reference.coefficient(NutrientDemandMode::Absorption), Some(24.0));
        assert_eq!(reference.harvested_organ, "grain");
    }

    /// The defect this schema exists to make impossible: banana's Mg was
    /// carried on the extraction row even though Tabla 10 prints a dash
    /// there, silently promoting an absorption figure into a maintenance
    /// plan. A blank must arrive as `None`, not as the other column.
    #[test]
    fn a_dash_in_the_source_table_stays_absent() {
        let reference = repo().describe_removal("banana", "Mg").expect("banana Mg");

        assert_eq!(reference.coefficient(NutrientDemandMode::Extraction), None);
        assert_eq!(reference.coefficient(NutrientDemandMode::Absorption), Some(1.5));
    }

    #[test]
    fn a_crop_nutrient_pair_the_table_never_measured_is_not_found() {
        // Tabla 10 gives artichoke N/P/K only, on the absorption basis.
        assert!(matches!(repo().describe_removal("artichoke", "S"), Err(DomainError::NotFound(_))));
    }
}
