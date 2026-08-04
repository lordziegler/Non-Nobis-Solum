//! Reads `data/reference/<profile>/liming_materials.csv`. Same shape as
//! `CsvFertilizerSourcesRepo`, kept separate because CaO/MgO grades
//! (neutralizing value) are not elemental Ca/Mg (nutrient supply).

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
    /// Both empty for a material meant to be applied on its own.
    #[serde(default)]
    mixture_id: Option<String>,
    #[serde(default)]
    mixture_share_pct: Option<f64>,
    #[serde(default)]
    source: Option<String>,
    restrictions: Option<String>,
}

/// Reads the liming material catalog from a CSV file.
///
/// Holds only the path: the file is opened per query rather than
/// cached, so an edit made while the app runs is picked up on the next
/// read.
pub struct CsvLimingMaterialsRepo {
    path: PathBuf,
}

impl CsvLimingMaterialsRepo {
    /// Points the repository at the liming materials CSV.
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
                mixture_id: row.mixture_id.filter(|s| !s.is_empty()),
                mixture_share_pct: row.mixture_share_pct,
                source: row.source.unwrap_or_default(),
                restrictions,
            });
        }
        Ok(materials)
    }
}
