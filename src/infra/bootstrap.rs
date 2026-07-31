//! Composition root helpers: pick a reference profile, instantiate the
//! concrete adapters for it, and wire them into the use cases. This is
//! the only place in the codebase that knows about file paths.
//!
//! Adding a new reference profile (say `usa_midwest`) never touches
//! `core`: create `data/reference/usa_midwest/` with the same five files
//! as `global`, and pass `--profile usa_midwest` on the CLI.

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

/// The live agroclimatic provider, for a one-shot CLI run: NASA POWER,
/// uncached — a single run fetches one climatology and exits, so a cache
/// in front of it would never be read.
///
/// Returns `None` rather than an error if the HTTP client can't even be
/// constructed — at this layer that is indistinguishable from the API
/// being down, and neither is allowed to stop a plan.
///
/// Swapping providers happens here and nowhere else: build a different
/// `AgroclimaticRepository` and the use case is none the wiser.
pub fn build_agroclimatic_repo() -> Option<Box<dyn AgroclimaticRepository>> {
    Some(Box::new(NasaPowerRepo::new().ok()?))
}

/// The same provider, but as a cache a long-lived front-end can keep and
/// share with a background thread. The TUI fills this off the render loop
/// and reads it through [`crate::infra::PrewarmedAgroclimaticRepo`], which
/// never blocks; a CLI run has nothing to share and uses the function
/// above instead.
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

/// Lot picker data. Reads the curated lots themselves, not the planning
/// rows: a lot exists whether or not a crop is planned on it.
pub fn build_list_lots(layout: &DataLayout) -> ListLots {
    ListLots::new(
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
    )
}

/// The only use case wired to a writer. Curated data is profile-independent,
/// so this is the same set of files whichever reference profile is active.
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
