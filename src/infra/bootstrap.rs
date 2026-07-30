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
    CsvCriticalLevelsRepo, CsvCropCatalogRepo, CsvFertilizerSourcesRepo, CsvFieldContextRepo, CsvLimingMaterialsRepo,
    CsvNutrientRemovalRepo, CsvSoilTestsRepo, CsvYieldTargetsRepo, StaticConversionFactorsRepo, StaticLimingRulesRepo,
    YamlEfficiencyRulesRepo,
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

/// What a long-lived front-end (the TUI) holds instead of a single wired
/// use case: the data root plus the profile currently selected. Use cases
/// are rebuilt from [`App::layout`] on every action, because switching
/// profile at runtime switches which reference files back them.
pub struct App {
    pub data_root: PathBuf,
    pub profile: String,
}

/// Default composition for the TUI: the same `data/` root and `global`
/// profile the CLI defaults to.
pub fn build_app() -> App {
    App { data_root: PathBuf::from("data"), profile: "global".to_string() }
}

/// One curated planning row: a lot, the crop planned on it, and its yield
/// goal. TODO(gap): there is no `ListLots` use case in `core::ports`, so
/// the TUI's lot selector is fed straight from the curated planning file
/// here in the composition root rather than through a port.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LotRow {
    pub field_id: String,
    pub crop_id: String,
    pub yield_value: f64,
    pub yield_unit: String,
}

impl App {
    pub fn layout(&self) -> DataLayout {
        DataLayout::new(&self.data_root, &self.profile)
    }

    pub fn reference_dir(&self) -> PathBuf {
        self.data_root.join("reference").join(&self.profile)
    }

    pub fn curated_dir(&self) -> PathBuf {
        self.data_root.join("curated")
    }

    /// Reference profiles available on disk, so the front-end never has to
    /// hardcode `global`/`andina_colombia`.
    pub fn profiles(&self) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(self.data_root.join("reference"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    pub fn lots(&self) -> Result<Vec<LotRow>, DomainError> {
        let mut reader = csv::Reader::from_path(self.curated_dir().join("yield_targets.csv"))
            .map_err(|e| DomainError::DataSource(e.to_string()))?;
        reader
            .deserialize()
            .collect::<Result<Vec<LotRow>, _>>()
            .map_err(|e| DomainError::DataSource(e.to_string()))
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
        Box::new(StaticLimingRulesRepo::from_toml_file(layout.reference("liming_rules.toml"))?),
        Box::new(CsvLimingMaterialsRepo::new(layout.reference("liming_materials.csv"))),
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
