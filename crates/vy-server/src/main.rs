//! Entry point for `vy-server`.
//!
//! It carries MessagePack RPC over stdio. The only arguments are `--stdio` and
//! `--version`. The workspace and the log destination come in the `initialize`
//! params (`docs/protocol.md`).

mod paths;
mod session;
mod stdio;

use std::process::ExitCode;

const HELP: &str = "\
vy-server: headless core for Vy

Usage:
  vy-server [--stdio]

Options:
  --stdio      carry RPC over stdio (default)
  --version    print the version
  -h, --help   show this help

Logs go to stderr. Stdout is reserved for RPC frames.
";

#[tokio::main]
async fn main() -> ExitCode {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--stdio" => {}
            "--version" | "-V" => {
                println!("vy-server {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}\n\n{HELP}");
                return ExitCode::from(2);
            }
        }
    }

    init_tracing();

    match stdio::serve(paths::data_dir()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "failed to process stdio");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("VY_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
