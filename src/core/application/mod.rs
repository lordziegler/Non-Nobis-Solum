//! Use cases: one module per thing a front-end can ask for.
//!
//! Each one owns a workflow end to end — it holds the ports it needs, and
//! nothing here reaches for a file, a terminal or a network. The re-exports
//! below are the whole application-layer vocabulary; a front-end should not
//! need to name a submodule.

pub mod calculate_fertility_plan;
pub mod inspect_scenario;
/// Lists the curated lots with enough of each to choose one.
pub mod list_lots;
pub mod list_supported_crops;
pub mod recommend_fertilizer_program;
pub mod register_lot;
pub mod scenario;

pub use calculate_fertility_plan::CalculateFertilityPlan;
pub use inspect_scenario::{InspectScenario, ScenarioInspection};
pub use list_lots::{ListLots, LotSummary};
pub use list_supported_crops::ListSupportedCrops;
pub use recommend_fertilizer_program::{FormulationRequest, RecommendFertilizerProgram};
pub use register_lot::{LotRegistration, RegisterLot, SoilTestEntry};
pub use scenario::FertilityScenario;
