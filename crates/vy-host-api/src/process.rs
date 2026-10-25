//! Shared `process` method and event declarations.

use serde::{Deserialize, Serialize};
use vy_proto::{ProcessExecParams, ProcessOutput};

use crate::{CallError, Reply};

/// `process.spawn` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSpawnParams {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<std::collections::BTreeMap<String, String>>,
}

/// `process.spawn` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSpawnReply {
    pub pid: u32,
}

/// `process.kill` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessKillParams {
    pub pid: u32,
    #[serde(default)]
    pub signal: Option<String>,
}

/// `process.kill` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessKillReply {
    pub success: bool,
}

/// `process.writeStdin` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStdinParams {
    pub pid: u32,
    pub data: String,
}

/// `process.writeStdin` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStdinReply {
    pub success: bool,
}

/// Payload of `process.onStdout` and `process.onStderr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ProcessData {
    pub pid: u32,
    pub data: String,
}

/// Payload of `process.onExit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExit {
    pub pid: u32,
    pub exit_code: i32,
}

/// Runs and manages external processes.
#[vy_api_schema::vy_api_trait("process")]
pub trait ProcessApi {
    /// Executes a process and waits for it to complete.
    #[vy_api_schema::vy_api_method]
    fn exec(&mut self, params: ProcessExecParams) -> Result<Reply<ProcessOutput>, CallError>;

    /// Spawns an asynchronous child process.
    #[vy_api_schema::vy_api_method]
    fn spawn(&mut self, params: ProcessSpawnParams) -> Result<Reply<ProcessSpawnReply>, CallError>;

    /// Terminates a running child process.
    #[vy_api_schema::vy_api_method]
    fn kill(&mut self, params: ProcessKillParams) -> Result<Reply<ProcessKillReply>, CallError>;

    /// Writes data to a child process standard input.
    #[vy_api_schema::vy_api_method]
    fn write_stdin(
        &mut self,
        params: ProcessStdinParams,
    ) -> Result<Reply<ProcessStdinReply>, CallError>;

    /// Fires when a child process writes to standard output.
    #[vy_api_schema::vy_api_event("process.onStdout", payload = ProcessData)]
    const ON_STDOUT: ();

    /// Fires when a child process writes to standard error.
    #[vy_api_schema::vy_api_event("process.onStderr", payload = ProcessData)]
    const ON_STDERR: ();

    /// Fires when a child process exits.
    #[vy_api_schema::vy_api_event("process.onExit", payload = ProcessExit)]
    const ON_EXIT: ();
}
