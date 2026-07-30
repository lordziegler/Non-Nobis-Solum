use clap::Parser;
use non_nobis_solum::infra::cli_adapter::{self, Cli};

fn main() {
    let cli = Cli::parse();
    if let Err(e) = cli_adapter::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
