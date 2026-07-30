//! Input ports: what the outside world (CLI, TUI, ...) can ask the core to do.

use crate::core::application::{FertilityScenario, LotRegistration, LotSummary, ScenarioInspection, SoilTestEntry};
use crate::core::domain::{Crop, DomainError, FertilityPlan};

pub trait FertilityCalculatorPort {
    fn calculate(&self, scenario: FertilityScenario) -> Result<FertilityPlan, DomainError>;
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
}
