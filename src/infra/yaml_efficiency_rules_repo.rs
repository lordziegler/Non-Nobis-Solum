//! Reads `data/reference/<profile>/efficiency_rules.yaml` — nutrient use
//! efficiency ranges by texture and irrigation system, from agronomic
//! guidelines (e.g. MSU Nutrient Guidelines).
//!
//! The curated grid only covers `{loam, clay_loam} x {rainfed, drip}`, 4
//! of the domain's 48 texture x irrigation combinations, and the missing
//! 44 need per-class field data nobody has transcribed yet. Rather than
//! fabricate them, lookup falls back to the sentinel row
//! `texture: "any", irrigation: "any"` — the same "exact match first,
//! sentinel second" shape `CsvCriticalLevelsRepo` uses for texture. The
//! sentinel rows are tagged in their `source` field as a fallback, not as
//! literature; see the header comment of either `efficiency_rules.yaml`.

use std::path::Path;

use serde::Deserialize;

use crate::core::domain::{DomainError, IrrigationSystem, Texture};
use crate::core::ports::EfficiencyRulesRepository;

const ANY: &str = "any";

#[derive(Debug, Deserialize, Clone)]
struct EfficiencyRuleRow {
    texture: String,
    irrigation: String,
    nutrient: String,
    efficiency_min: f64,
    efficiency_max: f64,
    #[allow(dead_code)]
    source: String,
    #[allow(dead_code)]
    region: String,
}

pub struct YamlEfficiencyRulesRepo {
    rules: Vec<EfficiencyRuleRow>,
}

impl YamlEfficiencyRulesRepo {
    pub fn from_yaml_file(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
        Self::from_yaml_str(&text)
    }

    fn from_yaml_str(text: &str) -> Result<Self, DomainError> {
        let rules: Vec<EfficiencyRuleRow> = serde_yaml::from_str(text).map_err(|e| DomainError::DataSource(e.to_string()))?;
        Ok(Self { rules })
    }
}

impl EfficiencyRulesRepository for YamlEfficiencyRulesRepo {
    fn get_efficiency_range(
        &self,
        texture: &Texture,
        irrigation: &IrrigationSystem,
        nutrient_id: &str,
    ) -> Result<(f64, f64), DomainError> {
        let texture_str = texture.to_string();
        let irrigation_str = irrigation.to_string();

        let exact = |r: &&EfficiencyRuleRow| r.texture == texture_str && r.irrigation == irrigation_str;
        let sentinel = |r: &&EfficiencyRuleRow| r.texture == ANY && r.irrigation == ANY;
        let matching = |accept: &dyn Fn(&&EfficiencyRuleRow) -> bool| {
            self.rules.iter().find(|r| r.nutrient == nutrient_id && accept(r))
        };

        matching(&exact)
            .or_else(|| matching(&sentinel))
            .map(|r| (r.efficiency_min, r.efficiency_max))
            .ok_or_else(|| {
                DomainError::NotFound(format!(
                    "no efficiency rule for texture={texture_str} irrigation={irrigation_str} nutrient={nutrient_id}"
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two nutrients: `N` has an exact `loam`/`rainfed` row plus a
    /// sentinel, `P` has only the sentinel. Literal input, so growing the
    /// shipped reference files can never break this.
    const YAML: &str = r#"
- texture: "loam"
  irrigation: "rainfed"
  nutrient: "N"
  efficiency_min: 0.55
  efficiency_max: 0.65
  source: "literature"
  region: "test"
- texture: "any"
  irrigation: "any"
  nutrient: "N"
  efficiency_min: 0.45
  efficiency_max: 0.80
  source: "fallback"
  region: "test"
- texture: "any"
  irrigation: "any"
  nutrient: "P"
  efficiency_min: 0.35
  efficiency_max: 0.55
  source: "fallback"
  region: "test"
"#;

    fn repo() -> YamlEfficiencyRulesRepo {
        YamlEfficiencyRulesRepo::from_yaml_str(YAML).expect("test fixture parses")
    }

    #[test]
    fn an_exact_row_wins_over_the_sentinel() {
        let range = repo()
            .get_efficiency_range(&Texture::Loam, &IrrigationSystem::Rainfed, "N")
            .expect("exact row");

        assert_eq!(range, (0.55, 0.65));
    }

    #[test]
    fn an_uncovered_combination_falls_back_to_the_sentinel() {
        let repo = repo();

        // Neither the texture nor the irrigation system is covered.
        assert_eq!(
            repo.get_efficiency_range(&Texture::SandyLoam, &IrrigationSystem::Sprinkler, "N")
                .expect("sentinel row"),
            (0.45, 0.80)
        );
        // Covered texture, uncovered irrigation system: still the sentinel.
        assert_eq!(
            repo.get_efficiency_range(&Texture::Loam, &IrrigationSystem::Gravity, "N")
                .expect("sentinel row"),
            (0.45, 0.80)
        );
        // A nutrient that only ever had a sentinel row.
        assert_eq!(
            repo.get_efficiency_range(&Texture::Clay, &IrrigationSystem::Drip, "P")
                .expect("sentinel row"),
            (0.35, 0.55)
        );
    }

    #[test]
    fn a_nutrient_with_no_row_at_all_still_errors() {
        let result = repo().get_efficiency_range(&Texture::Loam, &IrrigationSystem::Rainfed, "K");

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
