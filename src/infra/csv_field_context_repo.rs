//! Reads `data/curated/field_context.csv` — physical/chemical context of
//! a field/lot (texture, irrigation, bulk density...), curated once per
//! field rather than re-entered on every planning run.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::core::domain::{DomainError, FieldContext, IrrigationSystem, Texture};
use crate::core::ports::FieldContextRepository;

#[derive(Debug, Deserialize)]
struct FieldContextRow {
    field_id: String,
    sample_id: String,
    texture: String,
    irrigation_system: String,
    organic_matter_percent: f64,
    ph: f64,
    cec: f64,
    bulk_density_kg_dm3: f64,
    arable_depth_m: f64,
    region: String,
    /// Optional so a curated file predating the climate work still loads:
    /// a lot with no coordinates gets no climate enrichment.
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
}

pub struct CsvFieldContextRepo {
    path: PathBuf,
}

impl CsvFieldContextRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }

    fn rows(&self) -> Result<impl Iterator<Item = Result<FieldContextRow, DomainError>>, DomainError> {
        let reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;
        Ok(reader
            .into_deserialize::<FieldContextRow>()
            .map(|row| row.map_err(|e| DomainError::DataSource(e.to_string()))))
    }
}

impl TryFrom<FieldContextRow> for FieldContext {
    type Error = DomainError;

    fn try_from(row: FieldContextRow) -> Result<Self, Self::Error> {
        Ok(FieldContext {
            field_id: row.field_id,
            sample_id: row.sample_id,
            texture: Texture::from_str(&row.texture)?,
            irrigation_system: IrrigationSystem::from_str(&row.irrigation_system)?,
            organic_matter_percent: row.organic_matter_percent,
            ph: row.ph,
            cec_cmolc_kg: row.cec,
            bulk_density_kg_dm3: row.bulk_density_kg_dm3,
            arable_depth_m: row.arable_depth_m,
            region: row.region,
            latitude: row.latitude,
            longitude: row.longitude,
        })
    }
}

impl FieldContextRepository for CsvFieldContextRepo {
    fn get_context_by_field_id(&self, field_id: &str) -> Result<FieldContext, DomainError> {
        for row in self.rows()? {
            let row = row?;
            if row.field_id == field_id {
                return row.try_into();
            }
        }

        Err(DomainError::NotFound(format!("no field context for field_id={field_id}")))
    }

    fn list_contexts(&self) -> Result<Vec<FieldContext>, DomainError> {
        self.rows()?.map(|row| row?.try_into()).collect()
    }
}
