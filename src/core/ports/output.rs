//! Output ports: everything the domain needs from the outside world.
//!
//! The first group (`SoilTestRepository`, `FieldContextRepository`,
//! `YieldTargetRepository`) reads curated, scenario-specific data — one
//! field, one sample, one planning cycle.
//!
//! The second group is the reference-data catalog: crops, removal
//! coefficients, unit conversions, efficiency rules, critical levels and
//! fertilizer sources. This is the scientific literature encoded as
//! tables, versioned in Git under `data/reference/<profile>/`. The end
//! user only ever picks a profile — nobody re-types these tables per
//! scenario.

use crate::core::domain::{
    AnnualClimatology, Crop, CriticalLevel, DomainError, FertilizerSource, FieldContext, IrrigationSystem,
    LimingMaterial, LotYieldTarget, RemovalReference, SoilTest, Texture, YieldTarget,
};

pub trait SoilTestRepository {
    fn get_tests_by_sample_id(&self, sample_id: &str) -> Result<Vec<SoilTest>, DomainError>;
}

pub trait FieldContextRepository {
    fn get_context_by_field_id(&self, field_id: &str) -> Result<FieldContext, DomainError>;
    /// Every curated lot, in file order. This is the authoritative list of
    /// what exists: a lot is a lot whether or not anything is planned on it.
    fn list_contexts(&self) -> Result<Vec<FieldContext>, DomainError>;
}

/// Scenario-specific yield goals (curated planning data, not science
/// literature — see `data/curated/yield_targets.csv`).
pub trait YieldTargetRepository {
    fn get_yield_target(&self, field_id: &str, crop_id: &str) -> Result<YieldTarget, DomainError>;
    fn list_targets(&self) -> Result<Vec<LotYieldTarget>, DomainError>;
}

/// The one write port in the project: appends new rows to the curated
/// files under `data/curated/`.
///
/// Deliberately append-only. Creating a lot, a sample or a planning row is
/// all the write access anything needs today; editing an existing row
/// means a read-modify-rename cycle, which is a different contract and can
/// be added when something actually asks for it.
///
/// Every method takes already-validated domain types — validation belongs
/// to the use case that owns the trust boundary (`RegisterLot`), not to
/// the adapter that serializes.
pub trait CuratedDataWriter {
    fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError>;
    fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError>;
    fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError>;
}

pub trait CropCatalogRepository {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError>;
    fn get_crop(&self, crop_id: &str) -> Result<Crop, DomainError>;
}

/// Crop nutrient removal/absorption coefficients, per unit of yield.
pub trait NutrientRemovalRepository {
    /// Total nutrient removed at `yield_target` (in `yield_unit`), in kg/ha.
    fn get_removal(
        &self,
        crop_id: &str,
        product: &str,
        nutrient_id: &str,
        yield_target: f64,
        yield_unit: &str,
    ) -> Result<f64, DomainError>;

    /// Raw coefficient plus dataset provenance, for `InspectScenario`.
    fn describe_removal(&self, crop_id: &str, product: &str, nutrient_id: &str) -> Result<RemovalReference, DomainError>;
}

/// Unit and chemical-form conversions (cmolc/kg -> mg/kg, P -> P2O5, etc).
pub trait ConversionFactorsRepository {
    fn convert(&self, from_unit: &str, to_unit: &str, nutrient_id: &str, value: f64) -> Result<f64, DomainError>;
}

/// Nutrient use efficiency ranges by texture and irrigation system.
pub trait EfficiencyRulesRepository {
    /// Returns `(efficiency_min, efficiency_max)` as fractions (0.0-1.0).
    fn get_efficiency_range(
        &self,
        texture: &Texture,
        irrigation: &IrrigationSystem,
        nutrient_id: &str,
    ) -> Result<(f64, f64), DomainError>;
}

/// Thresholds to interpret a raw soil test value as low/medium/high.
pub trait CriticalLevelsRepository {
    fn get_critical_level(&self, nutrient_id: &str, texture: &Texture, region: &str) -> Result<CriticalLevel, DomainError>;
}

pub trait FertilizerSourceRepository {
    fn list_sources(&self) -> Result<Vec<FertilizerSource>, DomainError>;
    fn get_source(&self, source_id: &str) -> Result<FertilizerSource, DomainError>;
}

/// Literature constants for liming: how much CaCO3-equivalent one unit of
/// exchangeable Al³⁺ requires, and what base saturation to aim for.
pub trait LimingRulesRepository {
    fn al_factor(&self, region: &str) -> Result<f64, DomainError>;
    fn target_base_saturation_pct(&self, region: &str) -> Result<f64, DomainError>;
}

/// Commercial liming materials and their neutralizing composition
/// (CaO/MgO, granulometric efficiency) — kept separate from
/// `FertilizerSourceRepository` (see `LimingMaterial`'s doc comment).
pub trait LimingMaterialRepository {
    fn list_materials(&self) -> Result<Vec<LimingMaterial>, DomainError>;
}

/// Long-term climatology for a point on the globe.
///
/// Deliberately the thinnest possible contract: coordinates in, an
/// [`AnnualClimatology`] out. It names no provider, no time window and no
/// parameter codes, so NASA POWER, Open-Meteo or Agromonitoring are all
/// swappable behind it without `core` noticing. Which variables a given
/// provider can actually supply is expressed by leaving fields `None`,
/// not by widening this trait.
///
/// Unlike every other repository here this one talks to a network, so
/// callers must treat `Err` as "degrade", not "fail" — see
/// `DomainError::ExternalServiceUnavailable`.
pub trait AgroclimaticRepository {
    fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError>;
}
