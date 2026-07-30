use crate::core::domain::YieldTarget;

/// Everything a caller must supply to plan a fertilization program: which
/// sample/field, which crop, and optionally which yield goal (falls back
/// to `YieldTargetRepository` when not given). No reference data lives
/// here — that comes entirely from the reference-data ports.
#[derive(Debug, Clone)]
pub struct FertilityScenario {
    pub sample_id: String,
    pub field_id: String,
    pub crop_id: String,
    pub product: String,
    pub yield_override: Option<YieldTarget>,
}
