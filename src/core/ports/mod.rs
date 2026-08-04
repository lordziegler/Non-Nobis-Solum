//! The traits that bound the core.
//!
//! `input` is what the outside may ask of the domain; `output` is what the
//! domain needs from the outside. Everything in `infra` implements one of
//! these and nothing in `core` names anything that isn't one.

pub mod input;
pub mod output;

pub use input::*;
pub use output::*;
