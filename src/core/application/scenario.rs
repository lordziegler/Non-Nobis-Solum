use crate::core::domain::YieldTarget;

/// Everything a caller must supply to plan. `yield_override` falls back
/// to `YieldTargetRepository` when absent; no reference data lives here.
#[derive(Debug, Clone)]
pub struct FertilityScenario {
    pub sample_id: String,
    pub field_id: String,
    pub crop_id: String,
    pub product: String,
    pub yield_override: Option<YieldTarget>,
}
