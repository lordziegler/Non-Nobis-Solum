//! Reads `conversion_factors.toml` — unit and chemical-form conversions,
//! loaded once and kept in memory.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::core::domain::DomainError;
use crate::core::ports::ConversionFactorsRepository;

#[derive(Debug, Deserialize)]
struct ConversionFactorsFile {
    #[serde(default)]
    unit_conversion: HashMap<String, f64>,
}

pub struct StaticConversionFactorsRepo {
    unit_conversion: HashMap<String, f64>,
}

impl StaticConversionFactorsRepo {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
        let file: ConversionFactorsFile = toml::from_str(&text).map_err(|e| DomainError::DataSource(e.to_string()))?;
        Ok(Self { unit_conversion: file.unit_conversion })
    }
}

impl ConversionFactorsRepository for StaticConversionFactorsRepo {
    fn convert(&self, from_unit: &str, to_unit: &str, nutrient_id: &str, value: f64) -> Result<f64, DomainError> {
        let per_nutrient_key = format!("{from_unit}_to_{to_unit}.{nutrient_id}");
        let generic_key = format!("{from_unit}_to_{to_unit}");

        let factor = self
            .unit_conversion
            .get(&per_nutrient_key)
            .or_else(|| self.unit_conversion.get(&generic_key))
            .ok_or_else(|| {
                DomainError::NotFound(format!("no conversion factor for {from_unit} -> {to_unit} ({nutrient_id})"))
            })?;

        Ok(value * factor)
    }
}
