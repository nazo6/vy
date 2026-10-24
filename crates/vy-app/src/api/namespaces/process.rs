//! `process` namespace.

/// Runs and manages external processes.
#[vy_api_schema::vy_api_namespace("process", shared = vy_host_api::process)]
mod api {
    use vy_host_api::process::{
        ProcessKillParams, ProcessKillReply, ProcessSpawnParams, ProcessSpawnReply,
        ProcessStdinParams, ProcessStdinReply,
    };
    use vy_proto::{ProcessExecParams, ProcessOutput, to_value};

    use crate::api::{Call, CallError, Reply, check_capability};
    use crate::plugin_host::HostAction;

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
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
