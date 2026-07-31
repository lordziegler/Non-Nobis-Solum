//! Reads `data/reference/<profile>/crops.csv` — the crop catalog. Part of
//! the versioned scientific reference data: the user picks a profile,
//! never re-types crop metadata.

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

pub struct CsvCropCatalogRepo {
    path: PathBuf,
}

impl CsvCropCatalogRepo {
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
