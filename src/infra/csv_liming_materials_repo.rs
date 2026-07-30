//! Reads `data/reference/<profile>/liming_materials.csv` — commercial
//! liming materials and their neutralizing composition. Same shape as
//! `CsvFertilizerSourcesRepo`, kept as a separate file/repo because CaO/MgO
//! grades (neutralizing value) aren't the same thing as elemental Ca/Mg
//! composition (nutrient supply) — see `LimingMaterial`'s doc comment.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::domain::{DomainError, LimingMaterial};
use crate::core::ports::LimingMaterialRepository;

#[derive(Debug, Deserialize)]
struct LimingMaterialRow {
    source_id: String,
    name: String,
    cao_pct: f64,
    mgo_pct: f64,
    granulometric_efficiency_pct: f64,
    restrictions: Option<String>,
}

pub struct CsvLimingMaterialsRepo {
    path: PathBuf,
}

impl CsvLimingMaterialsRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl LimingMaterialRepository for CsvLimingMaterialsRepo {
    fn list_materials(&self) -> Result<Vec<LimingMaterial>, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        let mut materials = Vec::new();
        for row in reader.deserialize::<LimingMaterialRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            let restrictions = row
                .restrictions
                .filter(|s| !s.is_empty())
                .map(|s| s.split(';').map(str::trim).map(String::from).collect())
                .unwrap_or_default();

            materials.push(LimingMaterial {
                source_id: row.source_id,
                name: row.name,
                cao_pct: row.cao_pct,
                mgo_pct: row.mgo_pct,
                granulometric_efficiency_pct: row.granulometric_efficiency_pct,
                restrictions,
            });
        }
        Ok(materials)
    }
}
