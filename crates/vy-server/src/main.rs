//! `vy-server` の入口です。
//!
//! 標準入出力で MessagePack RPC を流します。引数は `--stdio` と `--version` だけです。
//! ワークスペースとログの出力先は `initialize` の params で受け取ります
//! （`docs/protocol.md`）。

mod session;
mod stdio;

use std::process::ExitCode;

const HELP: &str = "\
vy-server: Vy のヘッドレスコア

使い方:
  vy-server [--stdio]

オプション:
  --stdio      標準入出力で RPC を流す（既定）
  --version    版を表示する
  -h, --help   この説明を表示する

ログは標準エラーに出す。標準出力は RPC のフレーム専用。
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
                eprintln!("未知の引数です: {other}\n\n{HELP}");
                return ExitCode::from(2);
            }
        }
    }

    init_tracing();

    match stdio::serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "標準入出力の処理に失敗しました");
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
