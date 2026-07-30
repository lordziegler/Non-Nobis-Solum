//! Composition root helpers: pick a reference profile, instantiate the
//! concrete adapters for it, and wire them into the use cases. This is
//! the only place in the codebase that knows about file paths.
//!
//! Adding a new reference profile (say `usa_midwest`) never touches
//! `core`: create `data/reference/usa_midwest/` with the same five files
//! as `global`, and pass `--profile usa_midwest` on the CLI.

use std::path::{Path, PathBuf};

use crate::core::application::{CalculateFertilityPlan, InspectScenario, ListSupportedCrops};
use crate::core::domain::DomainError;
use crate::infra::{
    CsvCriticalLevelsRepo, CsvCropCatalogRepo, CsvFertilizerSourcesRepo, CsvFieldContextRepo, CsvNutrientRemovalRepo,
    CsvSoilTestsRepo, CsvYieldTargetsRepo, StaticConversionFactorsRepo, YamlEfficiencyRulesRepo,
};

/// Resolves paths for a chosen reference profile plus the fixed curated
/// data directory. `conversion_factors.toml` is intentionally shared
/// across every profile: atomic weights and unit conversions are
/// universal science, not regional agronomy.
pub struct DataLayout {
    data_root: PathBuf,
    profile: String,
}

impl DataLayout {
    pub fn new(data_root: impl AsRef<Path>, profile: impl Into<String>) -> Self {
        Self { data_root: data_root.as_ref().to_path_buf(), profile: profile.into() }
    }

    fn reference(&self, filename: &str) -> PathBuf {
        self.data_root.join("reference").join(&self.profile).join(filename)
    }

    fn shared_reference(&self, filename: &str) -> PathBuf {
        self.data_root.join("reference").join("global").join(filename)
    }

    fn curated(&self, filename: &str) -> PathBuf {
        self.data_root.join("curated").join(filename)
    }
}

pub fn build_calculate_fertility_plan(layout: &DataLayout) -> Result<CalculateFertilityPlan, DomainError> {
    Ok(CalculateFertilityPlan::new(
        Box::new(CsvSoilTestsRepo::new(layout.curated("soil_tests.csv"))),
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
        Box::new(CsvNutrientRemovalRepo::new(layout.reference("nutrient_removal.csv"))),
        Box::new(StaticConversionFactorsRepo::from_toml_file(layout.shared_reference("conversion_factors.toml"))?),
        Box::new(YamlEfficiencyRulesRepo::from_yaml_file(layout.reference("efficiency_rules.yaml"))?),
        Box::new(CsvCriticalLevelsRepo::new(layout.reference("critical_levels.csv"))),
        Box::new(CsvFertilizerSourcesRepo::new(layout.reference("fertilizer_sources.csv"))),
    ))
}

pub fn build_list_supported_crops(layout: &DataLayout) -> ListSupportedCrops {
    ListSupportedCrops::new(Box::new(CsvCropCatalogRepo::new(layout.shared_reference("crops.csv"))))
}

pub fn build_inspect_scenario(layout: &DataLayout) -> Result<InspectScenario, DomainError> {
    Ok(InspectScenario::new(
        Box::new(CsvSoilTestsRepo::new(layout.curated("soil_tests.csv"))),
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
        Box::new(CsvNutrientRemovalRepo::new(layout.reference("nutrient_removal.csv"))),
        Box::new(YamlEfficiencyRulesRepo::from_yaml_file(layout.reference("efficiency_rules.yaml"))?),
        Box::new(CsvCriticalLevelsRepo::new(layout.reference("critical_levels.csv"))),
    ))
}
