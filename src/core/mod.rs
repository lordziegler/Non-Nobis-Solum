//! Core: domain and application logic, free of any IO or framework
//! dependency. Everything here is pure Rust — no file paths, no CSV/TOML
//! parsing, no CLI framework. Adapters in `crate::infra` implement the
//! traits declared in `ports` and are wired in at the composition root.

pub mod application;
pub mod domain;
pub mod ports;
