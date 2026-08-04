//! Reads `data/reference/<profile>/liming_rules.toml` — Al-toxicity
//! factor and target base saturation.
//!
//! A lot's `region` and the active `--profile` are independent knobs, so a
//! row may carry the sentinel region `"any"`: exact region first, sentinel
//! second, the shape `CsvCriticalLevelsRepo` also uses.

use std::path::Path;

use serde::Deserialize;

use crate::core::domain::DomainError;
use crate::core::ports::LimingRulesRepository;

const ANY_REGION: &str = "any";

#[derive(Debug, Deserialize)]
struct LimingRulesRow {
    region: String,
    al_factor: f64,
    target_base_saturation_pct: f64,
    // Parsed and never read. Keeping the field is what makes serde
    // *require* it: a liming row without its citation fails to load
    // instead of planning off an untraceable figure. See
    // `data/reference/README.md`.
    #[allow(dead_code)]
    source: String,
}

#[derive(Debug, Deserialize)]
struct LimingRulesFile {
    rules: Vec<LimingRulesRow>,
}

/// The profile's liming constants, loaded once into memory.
///
/// Keyed by region with a sentinel fallback, so a lot naming a region the
/// profile does not list still gets the profile's general rule.
pub struct StaticLimingRulesRepo {
    rules: Vec<LimingRulesRow>,
}

impl StaticLimingRulesRepo {
    /// Loads the profile's liming constants once, at construction.
    ///
    /// # Errors
    /// `DataSource` when the file cannot be read or is not the TOML shape
    /// the rules are written in.
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
            .or_else(|| self.rules.iter().find(|r| r.region == ANY_REGION))
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

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = r#"
[[rules]]
region = "andina_colombia"
al_factor = 2.0
target_base_saturation_pct = 70.0
source = "test"

[[rules]]
region = "any"
al_factor = 1.5
target_base_saturation_pct = 80.0
source = "test"
"#;

    fn repo() -> StaticLimingRulesRepo {
        let file: LimingRulesFile = toml::from_str(RULES).expect("test fixture parses");
        StaticLimingRulesRepo { rules: file.rules }
    }

    #[test]
    fn a_named_region_wins_over_the_sentinel() {
        assert_eq!(repo().al_factor("andina_colombia").unwrap(), 2.0);
    }

    /// `LimingRulesRepository` propagates, so without the fallback a lot
    /// whose region the profile doesn't name kills the whole plan.
    #[test]
    fn an_unnamed_region_falls_back_to_the_sentinel() {
        assert_eq!(repo().al_factor("somewhere_else").unwrap(), 1.5);
        assert_eq!(repo().target_base_saturation_pct("somewhere_else").unwrap(), 80.0);
    }
}
