//! Input ports: what the outside world (CLI, TUI, ...) can ask the core to do.

use crate::core::application::{
    FertilityScenario, FormulationRequest, LotRegistration, LotSummary, ScenarioInspection, SoilTestEntry,
};
use crate::core::domain::{Crop, DomainError, FertilityPlan, FertilizerRecommendationReport};

pub trait FertilityCalculatorPort {
    fn calculate(&self, scenario: FertilityScenario) -> Result<FertilityPlan, DomainError>;
}

/// The product half of a plan: which bags, how many, and why.
///
/// Takes an already-computed [`FertilityPlan`] rather than a scenario, so
/// the two halves compose without either owning the other — a caller that
/// only wants the balance never pays for the catalog scan.
pub trait RecommendFertilizerProgramPort {
    fn recommend(
        &self,
        plan: &FertilityPlan,
        request: &FormulationRequest,
    ) -> Result<FertilizerRecommendationReport, DomainError>;
}

pub trait ListCropsPort {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError>;
}

pub trait ListLotsPort {
    fn list_lots(&self) -> Result<Vec<LotSummary>, DomainError>;
}

pub trait InspectScenarioPort {
    fn inspect(&self, scenario: &FertilityScenario) -> Result<ScenarioInspection, DomainError>;
}

/// The only input port that changes anything on disk. Both methods take
/// raw text: parsing and validation are the use case's job, precisely so
/// that no front-end can skip them.
pub trait RegisterLotPort {
    fn register_lot(&self, registration: &LotRegistration) -> Result<(), DomainError>;
    fn add_soil_tests(&self, field_id: &str, entries: &[SoilTestEntry]) -> Result<(), DomainError>;

    /// Rewrites an existing lot. Takes the same raw-text
    /// [`LotRegistration`] as `register_lot` and runs it through the same
    /// validation — an edit is not a lesser kind of write.
    fn edit_lot(&self, registration: &LotRegistration) -> Result<(), DomainError>;

    /// Removes a lot and everything attached to it. Returns how many rows
    /// went, across all three curated files.
    fn delete_lot(&self, field_id: &str) -> Result<usize, DomainError>;

    /// Curates the yield goal for one lot and crop.
    ///
    /// The gap this closed: `register_lot` writes a lot's *first* planning
    /// row and nothing wrote a second. A lot planted with a different crop
    /// next season could only get a goal by editing the whole lot, or by
    /// typing one on stage ③ — which is a per-run override and is never
    /// stored. A goal is a fact about (lot, crop), so it gets its own door.
    fn set_yield_target(
        &self,
        field_id: &str,
        crop_id: &str,
        yield_value: &str,
        yield_unit: &str,
    ) -> Result<(), DomainError>;
}
