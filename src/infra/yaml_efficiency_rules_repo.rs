//! Reads `data/reference/<profile>/efficiency_rules.yaml` — nutrient use
//! efficiency ranges by texture and irrigation system, from agronomic
//! guidelines (e.g. MSU Nutrient Guidelines).

use std::path::Path;

use serde::Deserialize;

use crate::core::domain::{DomainError, IrrigationSystem, Texture};
use crate::core::ports::EfficiencyRulesRepository;

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
        let rules: Vec<EfficiencyRuleRow> = serde_yaml::from_str(&text).map_err(|e| DomainError::DataSource(e.to_string()))?;
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

        self.rules
            .iter()
            .find(|r| r.texture == texture_str && r.irrigation == irrigation_str && r.nutrient == nutrient_id)
            .map(|r| (r.efficiency_min, r.efficiency_max))
            .ok_or_else(|| {
                DomainError::NotFound(format!(
                    "no efficiency rule for texture={texture_str} irrigation={irrigation_str} nutrient={nutrient_id}"
                ))
            })
    }
}
