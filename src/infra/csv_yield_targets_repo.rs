//! Reads `data/curated/yield_targets.csv` — planning-cycle yield goals
//! per field and crop. Used only as a fallback when the caller doesn't
//! supply an explicit yield override.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{DomainError, YieldTarget};
use crate::core::ports::YieldTargetRepository;

#[derive(Debug, Deserialize)]
struct YieldTargetRow {
    field_id: String,
    crop_id: String,
    yield_value: f64,
    yield_unit: String,
}

pub struct CsvYieldTargetsRepo {
    path: PathBuf,
}

impl CsvYieldTargetsRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl YieldTargetRepository for CsvYieldTargetsRepo {
    fn get_yield_target(&self, field_id: &str, crop_id: &str) -> Result<YieldTarget, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        for row in reader.deserialize::<YieldTargetRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.field_id == field_id && row.crop_id == crop_id {
                return Ok(YieldTarget { value: row.yield_value, unit: row.yield_unit });
            }
        }

        Err(DomainError::NotFound(format!(
            "no yield target for field_id={field_id} crop_id={crop_id}"
        )))
    }
}
