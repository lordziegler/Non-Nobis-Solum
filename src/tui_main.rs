use non_nobis_solum::core::domain::DomainError;
use non_nobis_solum::infra::bootstrap::CuratedSeed;
use non_nobis_solum::infra::{bootstrap, tui_adapter};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// The catalog has to exist before the first screen draws: the TUI reads
/// the lot list on startup, and an empty data root would open on an error
/// banner instead of on an empty list.
fn run() -> Result<(), DomainError> {
    let app = bootstrap::build_app();
    bootstrap::ensure_data_root(&app.data_root, CuratedSeed::HeadersOnly)?;
    tui_adapter::run(app)
}
