//! Library root: both front-ends (`non_nobis_solum` CLI and `nns-tui`)
//! are thin binaries over these modules, so the core and its adapters are
//! compiled once and shared.

pub mod core;
pub mod infra;
