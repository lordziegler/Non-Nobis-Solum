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
}
