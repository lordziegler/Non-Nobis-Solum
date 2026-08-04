//! The one error type every layer maps into.
//!
//! `core` names no `csv`, `toml`, `serde_yaml` or `std::io` error; each
//! adapter converts its own into one of these four variants, so a caller
//! can branch on what went wrong without knowing what read the file.

/// Adapters map their own IO/parsing errors into this type, keeping
/// `core` free of `csv`, `toml`, `serde_yaml` and `std::io`.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// A record the caller named does not exist: a lot, a sample, a crop,
    /// or a reference-table row for a combination the literature never
    /// covered. Carries what was looked for.
    #[error("not found: {0}")]
    NotFound(String),
    /// The caller's own input is wrong — unparseable, out of range, a
    /// duplicate id, or a unit nothing can convert. The only variant that
    /// means "the user can fix this by typing something else".
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// A local file could not be read, written or parsed. A broken install
    /// or a corrupted curated file, not a user mistake.
    #[error("data source error: {0}")]
    DataSource(String),
    /// A remote provider was unreachable. Distinct from `DataSource`
    /// because callers are expected to *degrade* on this one, not fail.
    #[error("external service unavailable: {0}")]
    ExternalServiceUnavailable(String),
}
