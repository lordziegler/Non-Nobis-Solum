//! The question a caller asks: which lot, which sample, which crop.
//!
//! Its own module because both halves of a plan take one and neither owns
//! it, and because it is the whole input surface of the core — anything not
//! in here is curated data or reference data, not a per-run choice.

use crate::core::domain::{NutrientDemandMode, YieldTarget};

/// Everything a caller must supply to plan. `yield_override` falls back
/// to `YieldTargetRepository` when absent; no reference data lives here.
#[derive(Debug, Clone)]
pub struct FertilityScenario {
    /// Which lab report to plan from.
    pub sample_id: String,
    /// Which lot to plan for.
    pub field_id: String,
    /// Which crop is going in.
    pub crop_id: String,
    /// Which of the reference table's two coefficients sizes the demand.
    /// Extraction is the maintenance default; see [`NutrientDemandMode`].
    pub demand_mode: NutrientDemandMode,
    /// A goal for this run only, overriding the curated one. Never stored:
    /// answering "what if I aimed higher" must not silently rewrite the
    /// lot's plan of record.
    pub yield_override: Option<YieldTarget>,
}
