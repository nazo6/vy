//! `process` namespace.

/// Runs and manages external processes.
#[vy_api_schema::vy_api_namespace("process")]
mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;
    use vy_proto::{ProcessExecParams, ProcessOutput, to_value};

    use crate::api::{Call, CallError, Reply, check_capability};
    use crate::plugin_host::HostAction;

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

    /// Executes a process and waits for it to complete.
    #[vy_api_schema::vy_api_method]
    fn exec(
        call: &mut Call<'_, '_>,
        params: ProcessExecParams,
    ) -> Result<Reply<ProcessOutput>, CallError> {
        check_capability(call.context, "process:exec", Some(&params.command))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "process/exec".into(),
            params: to_value(&params),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Spawns an asynchronous child process.
    #[vy_api_schema::vy_api_method]
    fn spawn(
        call: &mut Call<'_, '_>,
        params: ProcessSpawnParams,
    ) -> Result<Reply<ProcessSpawnReply>, CallError> {
        check_capability(call.context, "process:exec", Some(&params.command))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "process/spawn".into(),
            params: to_value(&vy_proto::ProcessSpawnParams {
                command: params.command,
                args: params.args,
                cwd: params.cwd,
                env: params.env,
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Terminates a running child process.
    #[vy_api_schema::vy_api_method]
    fn kill(
        call: &mut Call<'_, '_>,
        params: ProcessKillParams,
    ) -> Result<Reply<ProcessKillReply>, CallError> {
        check_capability(call.context, "process:exec", None)?;
        call.outcome.action = HostAction::ServerRpc {
            method: "process/kill".into(),
            params: to_value(&vy_proto::ProcessKillParams {
                pid: params.pid,
                signal: params.signal,
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Writes data to a child process standard input.
    #[vy_api_schema::vy_api_method]
    fn write_stdin(
        call: &mut Call<'_, '_>,
        params: ProcessStdinParams,
    ) -> Result<Reply<ProcessStdinReply>, CallError> {
        check_capability(call.context, "process:exec", None)?;
        call.outcome.action = HostAction::ServerRpc {
            method: "process/stdin".into(),
            params: to_value(&vy_proto::ProcessStdinParams {
                pid: params.pid,
                data: params.data,
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Fires when a child process writes to standard output.
    #[vy_api_schema::vy_api_event("process.onStdout", payload = ProcessData)]
    const ON_STDOUT: () = ();

    /// Fires when a child process writes to standard error.
    #[vy_api_schema::vy_api_event("process.onStderr", payload = ProcessData)]
    const ON_STDERR: () = ();

    /// Fires when a child process exits.
    #[vy_api_schema::vy_api_event("process.onExit", payload = ProcessExit)]
    const ON_EXIT: () = ();
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
