//! Reads `data/reference/<profile>/fertilizer_sources.csv`. One row per
//! (source, nutrient) pair, grouped by `source_id` on load.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::core::domain::{DomainError, FertilizerForm, FertilizerSource, Nutrient};
use crate::core::ports::FertilizerSourceRepository;

#[derive(Debug, Deserialize)]
struct FertilizerSourceRow {
    source_id: String,
    name: String,
    nutrient_id: String,
    pct: f64,
    density_kg_l: Option<f64>,
    /// Optional so a catalog written before the column still loads. An
    /// absent column and an empty cell both mean `Unknown`; a cell with
    /// something unrecognised in it fails the load by name.
    #[serde(default)]
    form: Option<String>,
    restrictions: Option<String>,
}

pub struct CsvFertilizerSourcesRepo {
    path: PathBuf,
}

impl CsvFertilizerSourcesRepo {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl FertilizerSourceRepository for CsvFertilizerSourcesRepo {
    fn list_sources(&self) -> Result<Vec<FertilizerSource>, DomainError> {
        let mut reader = csv::Reader::from_path(&self.path)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", self.path.display())))?;

        let mut sources: Vec<FertilizerSource> = Vec::new();
        for row in reader.deserialize::<FertilizerSourceRow>() {
            let row = row.map_err(|e| DomainError::DataSource(e.to_string()))?;
            let nutrient = Nutrient::from_str(&row.nutrient_id)?;
            let form = FertilizerForm::from_str(row.form.as_deref().unwrap_or_default())
                .map_err(|e| DomainError::DataSource(format!("{}, source_id={}: {e}", self.path.display(), row.source_id)))?;
            let restrictions = row
                .restrictions
                .filter(|s| !s.is_empty())
                .map(|s| s.split(';').map(str::trim).map(String::from).collect())
                .unwrap_or_default();

            match sources.iter_mut().find(|s| s.source_id == row.source_id) {
                Some(source) => {
                    source.composition_pct.push((nutrient, row.pct));
                    if source.density_kg_l.is_none() {
                        source.density_kg_l = row.density_kg_l;
                    }
                    // One product, one form. A second row naming a
                    // different one is a blend of forms, not a correction.
                    if source.form != form {
                        source.form = match (source.form, form) {
                            (FertilizerForm::Unknown, stated) | (stated, FertilizerForm::Unknown) => stated,
                            _ => FertilizerForm::Mixed,
                        };
                    }
                }
                None => sources.push(FertilizerSource {
                    source_id: row.source_id,
                    name: row.name,
                    composition_pct: vec![(nutrient, row.pct)],
                    density_kg_l: row.density_kg_l,
                    form,
                    restrictions,
                }),
            }
        }
        Ok(sources)
    }
}
