/// Domain-level error type. Adapters map their own IO/parsing errors into
/// this type via `DataSource`, keeping `core` free of any dependency on
/// `csv`, `toml`, `serde_yaml` or `std::io`.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("data source error: {0}")]
    DataSource(String),
    /// A remote provider (an HTTP API, not a local file) was unreachable,
    /// timed out or answered with an error status. Distinct from
    /// `DataSource` because callers are expected to *degrade* on this one
    /// rather than fail: see `CalculateFertilityPlan`'s climate handling.
    #[error("external service unavailable: {0}")]
    ExternalServiceUnavailable(String),
}
