//! Generates the TypeScript declarations for the plugin API.
//!
//! The generator is a CLI, not a test, so it does not depend on `vy-app` or
//! GPUI. Client command ids and settings keys are declared in `vy-client-api`;
//! the shared and client-only namespaces come from the API crates. A future
//! `backend` surface can reuse the same `codegen::export_to` entry point with
//! `vy-host-api` declarations.

use std::error::Error;

use vy_api_codegen::{check_client, generate_client};
use vy_api_schema::codegen::Output;

/// Generates the TypeScript declarations for the plugin API
#[derive(usage_rs::Cli)]
#[usage(bin = "vy-api-codegen", version)]
struct Cli {
    /// Compare generated files without writing them
    #[usage(long)]
    check: bool,

    /// Surface to generate
    #[usage(default = "client", choices("client"))]
    surface: String,
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("vy-api-codegen: {error}");
        std::process::exit(2);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.surface.as_str() {
        "client" => {
            let output = Output::packages();
            if cli.check {
                check_client(&output)?;
                eprintln!("vy-api-codegen: client declarations are up to date");
            } else {
                generate_client(&output)?;
                eprintln!("vy-api-codegen: wrote client declarations");
            }
        }
        other => return Err(format!("unknown surface: {other}").into()),
    }

    Ok(())
}
