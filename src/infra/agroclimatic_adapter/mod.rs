//! Agroclimatic adapters: the only part of the codebase that talks to a
//! network. Everything here implements `AgroclimaticRepository`, so a
//! different provider (Open-Meteo, Agromonitoring) is a new file next to
//! `nasa_power.rs` and one line in `bootstrap.rs` — `core` never changes.
//!
//! Provider adapters are expected to be best-effort: they return
//! `DomainError::ExternalServiceUnavailable` and the caller degrades to a
//! climate-free plan rather than failing.

pub mod cache;
pub mod nasa_power;

pub use cache::CachedAgroclimaticRepo;
pub use nasa_power::NasaPowerRepo;
