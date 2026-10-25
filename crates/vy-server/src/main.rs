//! Entry point for `vy-server`.
//!
//! It carries MessagePack RPC over stdio. The only arguments are `--stdio` and
//! `--version`. The workspace and the log destination come in the `initialize`
//! params (`docs/specs/protocol.md`).

mod paths;
mod session;
mod stdio;

use std::process::ExitCode;

/// Headless core for Vy
#[derive(usage_rs::Cli)]
#[usage(
    bin = "vy-server",
    version,
    after_help = "Logs go to stderr. Stdout is reserved for RPC frames."
)]
struct Cli {
    /// Carry RPC over stdio (the only transport)
    #[usage(long)]
    stdio: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing();
    tracing::debug!(stdio = cli.stdio, "starting vy-server");

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
