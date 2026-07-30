mod core;
mod infra;

use clap::Parser;
use infra::cli_adapter::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(e) = infra::cli_adapter::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
