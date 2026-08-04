//! Reads `data/reference/<profile>/crops.csv` — the crop catalog.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{Crop, DomainError};
use crate::core::ports::CropCatalogRepository;

#[derive(Debug, Deserialize)]
struct CropRow {
    crop_id: String,
    name: String,
    crop_type: String,
    family: String,
}

/// Reads the crop catalog from a CSV file.
///
/// Holds only the path: the file is opened per query rather than
/// cached, so an edit made while the app runs is picked up on the next
/// read.
pub struct CsvCropCatalogRepo {
    path: PathBuf,
}

impl CsvCropCatalogRepo {
    /// Points the repository at the crop catalog CSV.
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

impl CropCatalogRepository for CsvCropCatalogRepo {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        reader
            .deserialize::<CropRow>()
            .map(|row| {
                let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
                Ok(Crop { crop_id: row.crop_id, name: row.name, crop_type: row.crop_type, family: row.family })
            })
            .collect()
    }
}
