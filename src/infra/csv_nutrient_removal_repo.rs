//! Reads `data/reference/<profile>/nutrient_removal.csv` — crop nutrient
//! removal coefficients from the agronomic literature. This is the table
//! that makes the whole system possible: nobody re-derives "how much N
//! does a maize grain crop remove per ton" every time they run a plan.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{DomainError, RemovalReference};
use crate::core::ports::NutrientRemovalRepository;

#[derive(Debug, Deserialize, Clone)]
struct RemovalRow {
    crop_id: String,
    product: String,
    yield_unit: String,
    nutrient_id: String,
    removal_kg_per_unit: f64,
    source: String,
    region: String,
    year: u16,
    dataset_version: String,
}

pub struct CsvNutrientRemovalRepo {
    path: PathBuf,
}

impl CsvNutrientRemovalRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }

    fn find_row(&self, crop_id: &str, product: &str, nutrient_id: &str) -> Result<RemovalRow, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        for row in reader.deserialize::<RemovalRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.crop_id == crop_id && row.product == product && row.nutrient_id == nutrient_id {
                return Ok(row);
            }
        }

        Err(DomainError::NotFound(format!(
            "no removal coefficient for crop_id={crop_id} product={product} nutrient_id={nutrient_id}"
        )))
    }
}

impl NutrientRemovalRepository for CsvNutrientRemovalRepo {
    fn get_removal(
        &self,
        crop_id: &str,
        product: &str,
        nutrient_id: &str,
        yield_target: f64,
        yield_unit: &str,
    ) -> Result<f64, DomainError> {
        let row = self.find_row(crop_id, product, nutrient_id)?;
        if row.yield_unit != yield_unit {
            return Err(DomainError::InvalidInput(format!(
                "yield_unit mismatch for crop_id={crop_id}: expected {}, got {yield_unit}",
                row.yield_unit
            )));
        }
        Ok(crate::core::domain::services::crop_removal_kg_ha(row.removal_kg_per_unit, yield_target))
    }

    fn describe_removal(&self, crop_id: &str, product: &str, nutrient_id: &str) -> Result<RemovalReference, DomainError> {
        let row = self.find_row(crop_id, product, nutrient_id)?;
        Ok(RemovalReference {
            removal_kg_per_unit: row.removal_kg_per_unit,
            source: row.source,
            region: row.region,
            year: row.year,
            dataset_version: row.dataset_version,
        })
    }
}
