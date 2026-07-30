use non_nobis_solum::infra::{bootstrap, tui_adapter};

fn main() {
    if let Err(e) = tui_adapter::run(bootstrap::build_app()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
