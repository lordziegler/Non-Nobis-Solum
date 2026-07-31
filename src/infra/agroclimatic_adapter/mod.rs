//! The only part of the codebase that talks to a network. A different
//! provider is a new file here plus one line in `bootstrap.rs`.
//!
//! Adapters are best-effort: they return
//! `DomainError::ExternalServiceUnavailable` and the caller degrades to a
//! climate-free plan rather than failing.

pub mod cache;
pub mod nasa_power;

pub use cache::{CachedAgroclimaticRepo, PrewarmedAgroclimaticRepo};
pub use nasa_power::NasaPowerRepo;
