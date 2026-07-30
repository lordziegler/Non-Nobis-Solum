//! Input ports: what the outside world (CLI, TUI, ...) can ask the core to do.

use crate::core::application::FertilityScenario;
use crate::core::domain::{Crop, DomainError, FertilityPlan};

pub trait FertilityCalculatorPort {
    fn calculate(&self, scenario: FertilityScenario) -> Result<FertilityPlan, DomainError>;
}

pub trait ListCropsPort {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError>;
}
