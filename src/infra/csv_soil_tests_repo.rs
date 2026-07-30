//! Reads `data/curated/soil_tests.csv` — lab results for a specific
//! sample, entered once per soil sample and reused across every planning
//! run for that sample. Not reference data: this is what the user's lab
//! actually measured.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::core::domain::{Depth, DomainError, Nutrient, SoilTest};
use crate::core::ports::SoilTestRepository;

#[derive(Debug, Deserialize)]
struct SoilTestRow {
    sample_id: String,
    nutrient_id: String,
    value: f64,
    unit: String,
    method_id: String,
    depth_from_cm: f64,
    depth_to_cm: f64,
}

pub struct CsvSoilTestsRepo {
    path: PathBuf,
}

impl CsvSoilTestsRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl SoilTestRepository for CsvSoilTestsRepo {
    fn get_tests_by_sample_id(&self, sample_id: &str) -> Result<Vec<SoilTest>, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        let mut tests: Vec<SoilTest> = Vec::new();
        for row in reader.deserialize::<SoilTestRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.sample_id != sample_id {
                continue;
            }
            let test = SoilTest {
                sample_id: row.sample_id,
                nutrient: Nutrient::from_str(&row.nutrient_id)?,
                value: row.value,
                unit: row.unit,
                method: row.method_id,
                layer: Depth { from_cm: row.depth_from_cm, to_cm: row.depth_to_cm },
            };
            // This file is append-only: `CuratedDataWriter` never rewrites
            // a row, so a corrected lab value arrives as a *second* row for
            // the same nutrient at the same depth. The later row is the
            // correction and wins — otherwise the correction would be
            // written, accepted, and then silently ignored by every plan.
            //
            // Keyed on the depth as well as the nutrient: P at 0-20 and P
            // at 20-40 are two different measurements, not a correction.
            match tests.iter_mut().find(|t| t.nutrient == test.nutrient && t.layer == test.layer) {
                Some(superseded) => *superseded = test,
                None => tests.push(test),
            }
        }

        if tests.is_empty() {
            return Err(DomainError::NotFound(format!("no soil tests for sample_id={sample_id}")));
        }
        Ok(tests)
    }
}
