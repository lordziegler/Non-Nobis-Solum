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
    Crop, CriticalLevel, DomainError, FertilizerSource, FieldContext, IrrigationSystem, RemovalReference, SoilTest,
    Texture, YieldTarget,
};

pub trait SoilTestRepository {
    fn get_tests_by_sample_id(&self, sample_id: &str) -> Result<Vec<SoilTest>, DomainError>;
}

pub trait FieldContextRepository {
    fn get_context_by_field_id(&self, field_id: &str) -> Result<FieldContext, DomainError>;
}

/// Scenario-specific yield goals (curated planning data, not science
/// literature — see `data/curated/yield_targets.csv`).
pub trait YieldTargetRepository {
    fn get_yield_target(&self, field_id: &str, crop_id: &str) -> Result<YieldTarget, DomainError>;
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
