//! Reads `data/curated/yield_targets.csv` — yield goals per field and
//! crop, used only when the caller supplies no explicit override.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{DomainError, LotYieldTarget, YieldTarget};
use crate::core::ports::YieldTargetRepository;

#[derive(Debug, Deserialize)]
struct YieldTargetRow {
    field_id: String,
    crop_id: String,
    yield_value: f64,
    yield_unit: String,
}

/// Reads the user's curated planning rows from a CSV file.
///
/// Holds only the path: the file is opened per query rather than
/// cached, so an edit made while the app runs is picked up on the next
/// read.
pub struct CsvYieldTargetsRepo {
    path: PathBuf,
}

impl CsvYieldTargetsRepo {
    /// Points the repository at the curated `yield_targets.csv`.
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

impl YieldTargetRepository for CsvYieldTargetsRepo {
    fn get_yield_target(&self, field_id: &str, crop_id: &str) -> Result<YieldTarget, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        // Last row wins, same append-only rule as `CsvSoilTestsRepo` — a
        // revised goal for a lot/crop pair is appended, not overwritten.
        let mut found = None;
        for row in reader.deserialize::<YieldTargetRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            if row.field_id == field_id && row.crop_id == crop_id {
                found = Some(YieldTarget { value: row.yield_value, unit: row.yield_unit });
            }
        }

        found.ok_or_else(|| DomainError::NotFound(format!("no yield target for field_id={field_id} crop_id={crop_id}")))
    }

    fn list_targets(&self) -> Result<Vec<LotYieldTarget>, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        // Same last-wins collapse, so a front-end can't list a goal the
        // planner won't use.
        let mut targets: Vec<LotYieldTarget> = Vec::new();
        for row in reader.deserialize::<YieldTargetRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            let target = LotYieldTarget {
                field_id: row.field_id,
                crop_id: row.crop_id,
                target: YieldTarget { value: row.yield_value, unit: row.yield_unit },
            };
            match targets.iter_mut().find(|t| t.field_id == target.field_id && t.crop_id == target.crop_id) {
                Some(superseded) => *superseded = target,
                None => targets.push(target),
            }
        }
        Ok(targets)
    }
}
