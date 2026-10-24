//! Shared `process` method and event declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};
use vy_proto::{ProcessExecParams, ProcessOutput};

use crate::{event_decl, method_decl};

/// `process.spawn` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSpawnParams {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    #[ts(optional)]
    pub cwd: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub env: Option<std::collections::BTreeMap<String, String>>,
}

/// `process.spawn` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSpawnReply {
    pub pid: u32,
}

/// `process.kill` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessKillParams {
    pub pid: u32,
    #[serde(default)]
    #[ts(optional)]
    pub signal: Option<String>,
}

/// `process.kill` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessKillReply {
    pub success: bool,
}

/// `process.writeStdin` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStdinParams {
    pub pid: u32,
    pub data: String,
}

/// `process.writeStdin` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStdinReply {
    pub success: bool,
}

/// Payload of `process.onStdout` and `process.onStderr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProcessData {
    pub pid: u32,
    pub data: String,
}

/// Payload of `process.onExit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExit {
    pub pid: u32,
    pub exit_code: i32,
}

method_decl!(
    EXEC,
    "process.exec",
    ProcessExecParams,
    ProcessOutput,
    "Executes a process and waits for it to complete."
);
method_decl!(
    SPAWN,
    "process.spawn",
    ProcessSpawnParams,
    ProcessSpawnReply,
    "Spawns an asynchronous child process."
);
method_decl!(
    KILL,
    "process.kill",
    ProcessKillParams,
    ProcessKillReply,
    "Terminates a running child process."
);
method_decl!(
    WRITE_STDIN,
    "process.writeStdin",
    ProcessStdinParams,
    ProcessStdinReply,
    "Writes data to a child process standard input."
);

event_decl!(
    ON_STDOUT,
    "process.onStdout",
    "process",
    "onStdout",
    ProcessData,
    "Fires when a child process writes to standard output."
);
event_decl!(
    ON_STDERR,
    "process.onStderr",
    "process",
    "onStderr",
    ProcessData,
    "Fires when a child process writes to standard error."
);
event_decl!(
    ON_EXIT,
    "process.onExit",
    "process",
    "onExit",
    ProcessExit,
    "Fires when a child process exits."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[EXEC, SPAWN, KILL, WRITE_STDIN];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[ON_STDOUT, ON_STDERR, ON_EXIT];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "process",
    doc: "Runs and manages external processes.",
    sugar: None,
};
