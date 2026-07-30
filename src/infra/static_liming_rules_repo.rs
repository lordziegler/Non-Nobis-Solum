//! Reads `data/reference/<profile>/liming_rules.toml` — literature
//! constants for the liming formulas (Al-toxicity factor, target base
//! saturation). Same TOML-constants-in-memory style as
//! `StaticConversionFactorsRepo`.

use std::path::Path;

use serde::Deserialize;

use crate::core::domain::DomainError;
use crate::core::ports::LimingRulesRepository;

#[derive(Debug, Deserialize)]
struct LimingRulesRow {
    region: String,
    al_factor: f64,
    target_base_saturation_pct: f64,
    #[allow(dead_code)]
    source: String,
}

#[derive(Debug, Deserialize)]
struct LimingRulesFile {
    rules: Vec<LimingRulesRow>,
}

pub struct StaticLimingRulesRepo {
    rules: Vec<LimingRulesRow>,
}

impl StaticLimingRulesRepo {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
        let file: LimingRulesFile = toml::from_str(&text).map_err(|e| DomainError::DataSource(e.to_string()))?;
        Ok(Self { rules: file.rules })
    }

    fn row_for(&self, region: &str) -> Result<&LimingRulesRow, DomainError> {
        self.rules
            .iter()
            .find(|r| r.region == region)
            .ok_or_else(|| DomainError::NotFound(format!("no liming rules for region={region}")))
    }
}

impl LimingRulesRepository for StaticLimingRulesRepo {
    fn al_factor(&self, region: &str) -> Result<f64, DomainError> {
        self.row_for(region).map(|r| r.al_factor)
    }

    fn target_base_saturation_pct(&self, region: &str) -> Result<f64, DomainError> {
        self.row_for(region).map(|r| r.target_base_saturation_pct)
    }
}
