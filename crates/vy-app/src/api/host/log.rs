//! Client implementation of the `log` host trait.

use std::path::Path;

use serde_json::{Value, json};
use vy_host_api::log::{LogApi, LogWriteParams};
use vy_host_api::{CallError, Reply};

use super::ClientHost;

impl LogApi for ClientHost<'_, '_, '_> {
    fn write(&mut self, params: LogWriteParams) -> Result<Reply<()>, CallError> {
        append_log(
            &self.call.context.paths.logs_dir().join("init.log"),
            params.level.as_str(),
            &params.channel,
            &params.message,
            &params.data,
        );
        Ok(Reply::Value(()))
    }
}

fn append_log(path: &Path, level: &str, channel: &str, message: &str, data: &Value) {
    let line = json!({
        "level": level,
        "channel": channel,
        "message": message,
        "data": data,
    })
    .to_string();

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
    eprintln!("vy: [{channel}] {message}");
}
