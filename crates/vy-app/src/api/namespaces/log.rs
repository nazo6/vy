//! `log` namespace.

/// Writes to plugin log channels.
#[vy_api_schema::vy_api_namespace("log")]
mod api {
    use std::path::Path;

    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};

    /// Severity of a log line or notification.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "lowercase")]
    pub enum LogLevel {
        Info,
        Warn,
        Error,
    }

    impl LogLevel {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Info => "info",
                Self::Warn => "warn",
                Self::Error => "error",
            }
        }
    }

    /// `log.write` parameters.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
    pub struct LogWriteParams {
        pub channel: String,
        pub level: LogLevel,
        pub message: String,
        #[serde(default)]
        #[ts(type = "unknown")]
        pub data: Value,
    }

    /// Writes one entry to a log channel.
    #[vy_api_schema::vy_api_method]
    fn write(call: &mut Call<'_, '_>, params: LogWriteParams) -> Result<Reply<()>, CallError> {
        append_log(
            &call.context.paths.logs_dir().join("init.log"),
            params.level.as_str(),
            &params.channel,
            &params.message,
            &params.data,
        );
        Ok(Reply::Value(()))
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
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
