//! Composition root: the only place that knows about file paths.
//!
//! Adding a reference profile never touches `core` — create
//! `data/reference/<name>/` with the same files as `global` and pass
//! `--profile <name>`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::application::{CalculateFertilityPlan, InspectScenario, ListLots, ListSupportedCrops, RegisterLot};
use crate::core::domain::DomainError;
use crate::core::ports::AgroclimaticRepository;
use crate::infra::{
    CachedAgroclimaticRepo, CsvCriticalLevelsRepo, CsvCropCatalogRepo, CsvCuratedWriter, CsvFertilizerSourcesRepo,
    CsvFieldContextRepo, CsvLimingMaterialsRepo, CsvNutrientRemovalRepo, CsvSoilTestsRepo, CsvYieldTargetsRepo,
    NasaPowerRepo, StaticConversionFactorsRepo, StaticLimingRulesRepo, YamlEfficiencyRulesRepo,
};

/// Paths for a chosen profile plus the curated directory.
/// `conversion_factors.toml` is shared across every profile: atomic
/// weights are universal science, not regional agronomy.
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

/// What a long-lived front-end holds instead of a wired use case. Use
/// cases are rebuilt from [`App::layout`] on every action, because
/// switching profile switches which reference files back them.
pub struct App {
    pub data_root: PathBuf,
    pub profile: String,
}

/// Default composition for the TUI: the same `data/` root and `global`
/// profile the CLI defaults to.
pub fn build_app() -> App {
    App { data_root: PathBuf::from("data"), profile: "global".to_string() }
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
}

/// For a one-shot CLI run: uncached, because a single run fetches one
/// climatology and exits.
///
/// `None` rather than an error if the HTTP client can't be built — at this
/// layer that is the API being down, and neither may stop a plan.
pub fn build_agroclimatic_repo() -> Option<Box<dyn AgroclimaticRepository>> {
    Some(Box::new(NasaPowerRepo::new().ok()?))
}

/// The same provider as a cache a front-end can share with a background
/// thread, read through [`crate::infra::PrewarmedAgroclimaticRepo`].
pub fn build_climate_cache() -> Option<Arc<CachedAgroclimaticRepo>> {
    Some(Arc::new(CachedAgroclimaticRepo::new(Box::new(NasaPowerRepo::new().ok()?))))
}

/// `agroclimatic` is `None` for an offline plan (`--no-climate`, or any
/// front-end that shouldn't block on a network call).
pub fn build_calculate_fertility_plan(
    layout: &DataLayout,
    agroclimatic: Option<Box<dyn AgroclimaticRepository>>,
) -> Result<CalculateFertilityPlan, DomainError> {
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
        agroclimatic,
    ))
}

/// Reads the curated lots, not the planning rows: a lot exists whether or
/// not a crop is planned on it.
pub fn build_list_lots(layout: &DataLayout) -> ListLots {
    ListLots::new(
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
    )
}

/// The only use case wired to a writer. Curated data is
/// profile-independent, so the files are the same under any profile.
pub fn build_register_lot(layout: &DataLayout) -> RegisterLot {
    RegisterLot::new(
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvCuratedWriter::new(
            layout.curated("field_context.csv"),
            layout.curated("soil_tests.csv"),
            layout.curated("yield_targets.csv"),
        )),
    )
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
