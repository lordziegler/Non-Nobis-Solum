//! Input ports: what the outside world (CLI, TUI, ...) can ask the core to do.

use crate::core::application::{
    FertilityScenario, FormulationRequest, LotRegistration, LotSummary, ScenarioInspection, SoilTestEntry,
};
use crate::core::domain::{Crop, DomainError, FertilityPlan, FertilizerRecommendationReport};

/// Computes the balance half of a plan.
pub trait FertilityCalculatorPort {
    /// # Errors
    /// `NotFound` when the scenario names a lot, sample, crop or yield goal
    /// that is not curated, or when a reference table states nothing for a
    /// nutrient the plan needs; `InvalidInput` when a reading arrives in a
    /// unit the conversion table cannot resolve; `DataSource` when a
    /// reference or curated file cannot be read.
    ///
    /// Never fails for want of climate: an unreachable provider degrades to
    /// the baseline mineralization factor and is reported as a warning on
    /// the plan.
    fn calculate(&self, scenario: FertilityScenario) -> Result<FertilityPlan, DomainError>;
}

/// The product half of a plan: which bags, how many, and why.
///
/// Takes an already-computed [`FertilityPlan`] rather than a scenario, so
/// the two halves compose without either owning the other — a caller that
/// only wants the balance never pays for the catalog scan.
pub trait RecommendFertilizerProgramPort {
    /// # Errors
    /// `InvalidInput` when the request carries a non-positive area or bag
    /// weight; `DataSource` when the fertilizer catalog or the conversion
    /// table cannot be read.
    ///
    /// A requirement no product in the catalog can carry is *not* an error:
    /// it comes back as an uncovered remainder on the report, because
    /// hiding it would be worse than reporting it.
    fn recommend(
        &self,
        plan: &FertilityPlan,
        request: &FormulationRequest,
    ) -> Result<FertilizerRecommendationReport, DomainError>;
}

/// Answers which crops a plan can be written for.
pub trait ListCropsPort {
    /// # Errors
    /// `DataSource` when the crop catalog cannot be read or a row does not
    /// parse.
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError>;
}

/// Answers which lots are curated, with enough of each to pick one.
pub trait ListLotsPort {
    /// # Errors
    /// `DataSource` when the curated lots or their planning rows cannot be
    /// read. No lots curated yet is an empty vector, not an error.
    fn list_lots(&self) -> Result<Vec<LotSummary>, DomainError>;
}

/// Reads and interprets a lot without planning a dose for it.
pub trait InspectScenarioPort {
    /// # Errors
    /// `NotFound` when the scenario names a lot or sample that is not
    /// curated; `DataSource` when a curated or reference file cannot be
    /// read. A property no threshold table covers is left uninterpreted
    /// rather than failing the inspection.
    fn inspect(&self, scenario: &FertilityScenario) -> Result<ScenarioInspection, DomainError>;
}

/// The only input port that changes anything on disk. Both methods take
/// raw text: parsing and validation are the use case's job, precisely so
/// that no front-end can skip them.
pub trait RegisterLotPort {
    /// # Errors
    /// `InvalidInput` when a field does not parse or falls outside its
    /// admissible range, reported one field at a time, and when the
    /// `field_id` is already curated — registering may not overwrite a lot;
    /// `DataSource` when the curated files cannot be read or written.
    fn register_lot(&self, registration: &LotRegistration) -> Result<(), DomainError>;
    /// # Errors
    /// `NotFound` when no curated lot carries that `field_id` — a reading
    /// needs a lot to belong to; `InvalidInput` when an entry does not parse
    /// or names a nutrient or unit the catalog does not know; `DataSource`
    /// when the analyses file cannot be read or written.
    fn add_soil_tests(&self, field_id: &str, entries: &[SoilTestEntry]) -> Result<(), DomainError>;

    /// Rewrites an existing lot. Takes the same raw-text
    /// [`LotRegistration`] as `register_lot` and runs it through the same
    /// validation — an edit is not a lesser kind of write.
    ///
    /// # Errors
    /// `NotFound` when no curated lot carries that `field_id`;
    /// `InvalidInput` on the same parsing and range failures as
    /// `register_lot`; `DataSource` when the curated files cannot be read,
    /// rewritten or renamed.
    fn edit_lot(&self, registration: &LotRegistration) -> Result<(), DomainError>;

    /// Removes a lot and everything attached to it. Returns how many rows
    /// went, across all three curated files.
    ///
    /// # Errors
    /// `NotFound` when no curated lot carries that `field_id`, checked
    /// before anything is removed; `DataSource` when any of the three files
    /// cannot be read, rewritten or renamed.
    fn delete_lot(&self, field_id: &str) -> Result<usize, DomainError>;

    /// Curates the yield goal for one lot and crop.
    ///
    /// The gap this closed: `register_lot` writes a lot's *first* planning
    /// row and nothing wrote a second. A lot planted with a different crop
    /// next season could only get a goal by editing the whole lot, or by
    /// typing one on stage ③ — which is a per-run override and is never
    /// stored. A goal is a fact about (lot, crop), so it gets its own door.
    ///
    /// # Errors
    /// `NotFound` when no curated lot carries that `field_id`;
    /// `InvalidInput` when the goal does not parse as a positive number or
    /// names a unit the catalog does not know; `DataSource` when the
    /// planning file cannot be read or written.
    fn set_yield_target(
        &self,
        field_id: &str,
        crop_id: &str,
        yield_value: &str,
        yield_unit: &str,
    ) -> Result<(), DomainError>;
}
