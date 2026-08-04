//! Output ports: everything the domain needs from the outside world.
//!
//! Two groups: curated, scenario-specific data (`data/curated/`) and the
//! reference-data catalog (`data/reference/<profile>/`), which is the
//! scientific literature encoded as tables. The user picks a profile;
//! nobody re-types those tables per scenario.

use crate::core::domain::{
    AnnualClimatology, Crop, CriticalLevel, DomainError, EfficiencyBandRules, FertilizerRecommendationReport,
    FertilizerSource,
    FieldContext, IrrigationSystem, LimingMaterial, LotYieldTarget, QualitativeBand, RemovalReference, SoilTest,
    Texture, YieldTarget,
};

/// The curated lab analyses.
pub trait SoilTestRepository {
    /// # Errors
    /// `DataSource` when the curated analyses cannot be read or a row does
    /// not parse. An unknown `sample_id` is an empty vector, not an error:
    /// a lot with no analyses yet is a normal state.
    fn get_tests_by_sample_id(&self, sample_id: &str) -> Result<Vec<SoilTest>, DomainError>;
}

/// The curated lots.
pub trait FieldContextRepository {
    /// # Errors
    /// `NotFound` when no curated lot carries that `field_id`;
    /// `DataSource` when the file cannot be read or a row does not parse.
    fn get_context_by_field_id(&self, field_id: &str) -> Result<FieldContext, DomainError>;
    /// Every curated lot, in file order — a lot is a lot whether or not
    /// anything is planned on it.
    ///
    /// # Errors
    /// `DataSource` when the file cannot be read or a row does not parse.
    /// No curated lots at all is an empty vector, not an error.
    fn list_contexts(&self) -> Result<Vec<FieldContext>, DomainError>;
}

/// The curated planning rows: which crop is planned on which lot, at what
/// goal.
pub trait YieldTargetRepository {
    /// # Errors
    /// `NotFound` when that lot has no goal curated for that crop — the
    /// caller is expected to ask for one rather than assume a yield;
    /// `DataSource` when the file cannot be read or a row does not parse.
    fn get_yield_target(&self, field_id: &str, crop_id: &str) -> Result<YieldTarget, DomainError>;
    /// # Errors
    /// `DataSource` when the file cannot be read or a row does not parse.
    fn list_targets(&self) -> Result<Vec<LotYieldTarget>, DomainError>;
}

/// The one write port in the project. Takes already-validated domain types
/// — validation belongs to the use case, which owns the trust boundary, not
/// to the serializer.
///
/// **Append and replace, not append and update.** The three `save_*`
/// methods append; the two `replace_*` methods rewrite the file with one
/// row changed or gone. The split is not cosmetic:
///
/// - For `soil_tests.csv` and `yield_targets.csv` an *edit* has always been
///   expressible as an append, because both readers collapse a repeated key
///   to the last row — a correction is a second row that supersedes the
///   first. Those files need `replace_*` only to **delete**, and to stop
///   growing forever.
/// - For `field_context.csv` an append is refused outright (a duplicate
///   `field_id` is a mistake, not a revision), so editing a lot was
///   impossible until `replace_field_context` existed.
///
/// Every `replace_*` is a read-modify-rename: the new file is written
/// beside the old one and renamed over it, so an interrupted edit leaves
/// the original intact rather than a half-written file. This is somebody's
/// only copy of their own soil analyses.
pub trait CuratedDataWriter {
    /// # Errors
    /// `InvalidInput` when a row already carries that `field_id` — an
    /// append may not become a silent overwrite, use `replace_field_context`
    /// to edit; `DataSource` when the file cannot be read or written.
    fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError>;
    /// # Errors
    /// `DataSource` when the file cannot be read or written. A repeated
    /// reading is not an error: the reader takes the last row, so an append
    /// is how a correction supersedes the value it replaces.
    fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError>;
    /// # Errors
    /// `DataSource` when the file cannot be read or written. As with the
    /// analyses, a repeated `(field_id, crop_id)` supersedes rather than
    /// conflicts.
    fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError>;

    /// Rewrites the lot's row in place. `NotFound` when no row carries that
    /// `field_id`, so an edit can never silently become an insert.
    ///
    /// # Errors
    /// `NotFound` when no row carries that `field_id`; `DataSource` when
    /// the file cannot be read, or the rewritten file cannot be written or
    /// renamed over the original.
    fn replace_field_context(&self, context: &FieldContext) -> Result<(), DomainError>;

    /// Drops every row for `field_id` from all three curated files: the
    /// lot, its analyses and its planning rows. Returns how many rows went.
    ///
    /// The one destructive operation in the project, which is why it lives
    /// on the port rather than being assembled from three calls by a
    /// front-end that could get half way and stop.
    ///
    /// # Errors
    /// `NotFound` when no lot carries that `field_id`, checked before
    /// anything is removed; `DataSource` when any of the three files cannot
    /// be read, rewritten or renamed.
    fn delete_lot(&self, field_id: &str) -> Result<usize, DomainError>;
}

/// The crop catalog.
pub trait CropCatalogRepository {
    /// # Errors
    /// `DataSource` when the crop catalog cannot be read or a row does not
    /// parse.
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError>;
}

/// The crop demand coefficients: how much of a nutrient a tonne of yield
/// asks for.
pub trait NutrientRemovalRepository {
    /// Both coefficients plus dataset provenance. `NotFound` means the
    /// table has no row at all for this crop and nutrient; a row that
    /// exists but leaves one basis blank is a `Some` reference with a
    /// `None` coefficient, which the caller resolves — the two are
    /// different facts and only the caller knows what to do about each.
    ///
    /// # Errors
    /// `NotFound` when the table has no row for that crop and nutrient;
    /// `DataSource` when the table cannot be read or a row does not parse.
    fn describe_removal(&self, crop_id: &str, nutrient_id: &str) -> Result<RemovalReference, DomainError>;
}

/// cmolc/kg -> mg/kg, P -> P2O5, etc.
pub trait ConversionFactorsRepository {
    /// # Errors
    /// `NotFound` when the table states no factor between those two units
    /// for that nutrient — which is what stops a reading in an
    /// unconvertible unit from silently entering a plan.
    fn convert(&self, from_unit: &str, to_unit: &str, nutrient_id: &str, value: f64) -> Result<f64, DomainError>;
}

/// The per-profile modifier table that moves a base efficiency to a real
/// site. Separate from [`EfficiencyRulesRepository`], which gives the base
/// range: one says what a nutrient recovers under unstated conditions, the
/// other says what this lot's conditions do to it.
///
/// No lookup key. The table is per profile and every row is keyed on a
/// measured condition, so there is no region axis for a sentinel to
/// reconcile — see `toml_efficiency_bands_repo`.
pub trait EfficiencyBandRepository {
    /// # Errors
    /// `DataSource` when the profile's band table cannot be read or does
    /// not parse, including a band whose factor is outside the range a
    /// modifier may take — the table is refused at load rather than
    /// producing an impossible efficiency later.
    fn band_rules(&self) -> Result<EfficiencyBandRules, DomainError>;
}

/// The base efficiency ranges, before any site condition moves them.
pub trait EfficiencyRulesRepository {
    /// `(efficiency_min, efficiency_max)` as fractions (0.0-1.0).
    ///
    /// # Errors
    /// `NotFound` when the profile states no row for that nutrient, not
    /// even the sentinel that covers every texture and irrigation system;
    /// `DataSource` when the rules file cannot be read or does not parse.
    fn get_efficiency_range(
        &self,
        texture: &Texture,
        irrigation: &IrrigationSystem,
        nutrient_id: &str,
    ) -> Result<(f64, f64), DomainError>;
}

/// Thresholds to interpret a raw soil test value as low/medium/high.
///
/// `extraction_method` is the lab method the reading came from, when the
/// sample states one. It is a lookup axis rather than metadata because P
/// thresholds genuinely differ between Bray II and Olsen; `None` means the
/// caller cannot say, and a nutrient whose thresholds do differ by method
/// then answers `NotFound` rather than guessing one.
pub trait CriticalLevelsRepository {
    /// # Errors
    /// `NotFound` when no row covers that nutrient, or when the nutrient's
    /// thresholds differ by extraction method and the caller named none;
    /// `DataSource` when the table cannot be read or a row does not parse.
    fn get_critical_level(
        &self,
        nutrient_id: &str,
        texture: &Texture,
        region: &str,
        extraction_method: Option<&str>,
    ) -> Result<CriticalLevel, DomainError>;
}

/// The qualitative interpretation tables: pH classes, organic matter by
/// thermal belt, electrical conductivity, CEC, the acidity diagnosis, and
/// the cation balance ratios.
///
/// One port for all of them because they share one shape — a named
/// category over a numeric interval — and splitting them would mean five
/// adapters over one file.
pub trait SoilQualityThresholdsRepository {
    /// Every band for `property`, in file order. `climate_zone` selects
    /// among belt-specific rows; a property whose thresholds do not vary
    /// by belt carries the sentinel `any` and answers for all of them.
    ///
    /// An empty vector rather than `NotFound` for an unknown property:
    /// nothing here is required for a plan, so a missing table leaves a
    /// reading uninterpreted instead of failing anything.
    ///
    /// # Errors
    /// `DataSource` when the table cannot be read or a row does not parse.
    /// An unknown property is an empty vector, never an error.
    fn bands(&self, property: &str, climate_zone: &str) -> Result<Vec<QualitativeBand>, DomainError>;
}

/// The catalog of purchasable products.
pub trait FertilizerSourceRepository {
    /// # Errors
    /// `DataSource` when the fertilizer catalog cannot be read or a row
    /// does not parse.
    fn list_sources(&self) -> Result<Vec<FertilizerSource>, DomainError>;
}

/// Writes a finished recommendation somewhere outside the process.
///
/// The domain hands over a [`FertilizerRecommendationReport`] — structured
/// data — and never a rendered page: which columns to draw, what a heading
/// looks like and whether the file is PDF, Markdown or anything else is the
/// adapter's business. `destination` is a `Path` because the user names it
/// per run; the adapter still owns every byte written to it.
pub trait ReportExporter {
    /// # Errors
    /// `InvalidInput` when `destination` carries an extension no adapter
    /// writes; `DataSource` when the file cannot be created or written.
    fn export(&self, report: &FertilizerRecommendationReport, destination: &std::path::Path) -> Result<(), DomainError>;
}

/// Literature constants for liming.
pub trait LimingRulesRepository {
    /// # Errors
    /// `NotFound` when neither the named region nor the sentinel that
    /// stands in for it states a factor; `DataSource` when the rules file
    /// cannot be read or does not parse.
    fn al_factor(&self, region: &str) -> Result<f64, DomainError>;
    /// # Errors
    /// `NotFound` when neither the named region nor the sentinel states a
    /// target; `DataSource` when the rules file cannot be read or does not
    /// parse.
    fn target_base_saturation_pct(&self, region: &str) -> Result<f64, DomainError>;
}

/// Kept separate from `FertilizerSourceRepository`: see `LimingMaterial`.
pub trait LimingMaterialRepository {
    /// # Errors
    /// `DataSource` when the materials table cannot be read or a row does
    /// not parse.
    fn list_materials(&self) -> Result<Vec<LimingMaterial>, DomainError>;
}

/// Long-term climatology for a point on the globe. Names no provider, no
/// time window and no parameter codes, so any of them is swappable behind
/// it; a variable a provider can't supply is `None`, not a wider trait.
///
/// The only repository here that talks to a network, so callers must treat
/// `Err` as "degrade", not "fail" — see
/// `DomainError::ExternalServiceUnavailable`.
pub trait AgroclimaticRepository {
    /// # Errors
    /// `ExternalServiceUnavailable` when the provider cannot be reached or
    /// answers with something that is not a climatology — callers degrade
    /// to the baseline on this rather than failing the plan.
    fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError>;
}
