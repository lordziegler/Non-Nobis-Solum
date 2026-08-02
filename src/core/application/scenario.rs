use crate::core::domain::{NutrientDemandMode, YieldTarget};

/// Everything a caller must supply to plan. `yield_override` falls back
/// to `YieldTargetRepository` when absent; no reference data lives here.
#[derive(Debug, Clone)]
pub struct FertilityScenario {
    pub sample_id: String,
    pub field_id: String,
    pub crop_id: String,
    /// Which of the reference table's two coefficients sizes the demand.
    /// Extraction is the maintenance default; see [`NutrientDemandMode`].
    pub demand_mode: NutrientDemandMode,
    pub yield_override: Option<YieldTarget>,
}
