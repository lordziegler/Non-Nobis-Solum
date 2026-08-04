//! The domain: agronomy as types and pure functions.
//!
//! Nothing in here does IO, and nothing in here knows a file format. The
//! split is by what a thing *is*: `entities` and `value_objects` for the
//! data model, `services` for the agronomic formulas, `efficiency` and
//! `formulation` for the two calculations big enough to own a module, and
//! `errors` for the one error type every layer above maps into.

pub mod efficiency;
pub mod entities;
pub mod errors;
pub mod formulation;
pub mod nutrient;
pub mod services;
pub mod value_objects;

pub use efficiency::{
    AdjustedEfficiency, BandGroup, EfficiencyBandRule, EfficiencyBandRules, EfficiencyModifier, ScenarioConditions,
    SulfurForm,
};
pub use entities::*;
pub use errors::DomainError;
pub use formulation::*;
pub use nutrient::Nutrient;
pub use value_objects::*;
