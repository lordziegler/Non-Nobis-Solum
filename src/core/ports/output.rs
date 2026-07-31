//! Output ports: everything the domain needs from the outside world.
//!
//! Two groups: curated, scenario-specific data (`data/curated/`) and the
//! reference-data catalog (`data/reference/<profile>/`), which is the
//! scientific literature encoded as tables. The user picks a profile;
//! nobody re-types those tables per scenario.

use crate::core::domain::{
    AnnualClimatology, Crop, CriticalLevel, DomainError, FertilizerSource, FieldContext, IrrigationSystem,
    LimingMaterial, LotYieldTarget, RemovalReference, SoilTest, Texture, YieldTarget,
};

pub trait SoilTestRepository {
    fn get_tests_by_sample_id(&self, sample_id: &str) -> Result<Vec<SoilTest>, DomainError>;
}

pub trait FieldContextRepository {
    fn get_context_by_field_id(&self, field_id: &str) -> Result<FieldContext, DomainError>;
    /// Every curated lot, in file order — a lot is a lot whether or not
    /// anything is planned on it.
    fn list_contexts(&self) -> Result<Vec<FieldContext>, DomainError>;
}

pub trait YieldTargetRepository {
    fn get_yield_target(&self, field_id: &str, crop_id: &str) -> Result<YieldTarget, DomainError>;
    fn list_targets(&self) -> Result<Vec<LotYieldTarget>, DomainError>;
}

/// The one write port in the project, and deliberately append-only:
/// editing an existing row is a read-modify-rename cycle, a different
/// contract to add when something asks for it.
///
/// Takes already-validated domain types — validation belongs to
/// `RegisterLot`, which owns the trust boundary, not to the serializer.
pub trait CuratedDataWriter {
    fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError>;
    fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError>;
    fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError>;
}

pub trait CropCatalogRepository {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError>;
}

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

/// cmolc/kg -> mg/kg, P -> P2O5, etc.
pub trait ConversionFactorsRepository {
    fn convert(&self, from_unit: &str, to_unit: &str, nutrient_id: &str, value: f64) -> Result<f64, DomainError>;
}

pub trait EfficiencyRulesRepository {
    /// `(efficiency_min, efficiency_max)` as fractions (0.0-1.0).
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
}

/// Literature constants for liming.
pub trait LimingRulesRepository {
    fn al_factor(&self, region: &str) -> Result<f64, DomainError>;
    fn target_base_saturation_pct(&self, region: &str) -> Result<f64, DomainError>;
}

/// Kept separate from `FertilizerSourceRepository`: see `LimingMaterial`.
pub trait LimingMaterialRepository {
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
    fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError>;
}
