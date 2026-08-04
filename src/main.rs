//! `nns`: the command-line front-end. Argument parsing lives in
//! `cli_adapter`, so this is only the process boundary — wire up, run,
//! and turn a `DomainError` into an exit code.

use clap::Parser;
use non_nobis_solum::infra::cli_adapter::{self, Cli};

fn main() {
    let cli = Cli::parse();
    if let Err(e) = cli_adapter::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
