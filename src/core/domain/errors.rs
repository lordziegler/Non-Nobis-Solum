/// Adapters map their own IO/parsing errors into this type, keeping
/// `core` free of `csv`, `toml`, `serde_yaml` and `std::io`.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("data source error: {0}")]
    DataSource(String),
    /// A remote provider was unreachable. Distinct from `DataSource`
    /// because callers are expected to *degrade* on this one, not fail.
    #[error("external service unavailable: {0}")]
    ExternalServiceUnavailable(String),
}
