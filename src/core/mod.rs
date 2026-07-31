//! Domain and application logic, free of IO and framework dependencies.
//! Adapters in `crate::infra` implement the traits declared in `ports`.

pub mod application;
pub mod domain;
pub mod ports;
