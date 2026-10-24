//! Client implementation of the `process` host trait.

use vy_host_api::process::{
    ProcessApi, ProcessKillParams, ProcessKillReply, ProcessSpawnParams, ProcessSpawnReply,
    ProcessStdinParams, ProcessStdinReply,
};
use vy_host_api::{CallError, Reply};
use vy_proto::{ProcessExecParams, ProcessOutput, RpcCall};

use super::ClientHost;
use crate::api::check_capability;
use crate::plugin_host::HostAction;

impl ProcessApi for ClientHost<'_, '_, '_> {
    fn exec(&mut self, params: ProcessExecParams) -> Result<Reply<ProcessOutput>, CallError> {
        check_capability(self.call.context, "process:exec", Some(&params.command))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::ProcessExec>(params),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn spawn(&mut self, params: ProcessSpawnParams) -> Result<Reply<ProcessSpawnReply>, CallError> {
        check_capability(self.call.context, "process:exec", Some(&params.command))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::ProcessSpawn>(vy_proto::ProcessSpawnParams {
                command: params.command,
                args: params.args,
                cwd: params.cwd,
                env: params.env,
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn kill(&mut self, params: ProcessKillParams) -> Result<Reply<ProcessKillReply>, CallError> {
        check_capability(self.call.context, "process:exec", None)?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::ProcessKill>(vy_proto::ProcessKillParams {
                pid: params.pid,
                signal: params.signal,
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn write_stdin(
        &mut self,
        params: ProcessStdinParams,
    ) -> Result<Reply<ProcessStdinReply>, CallError> {
        check_capability(self.call.context, "process:exec", None)?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::ProcessStdin>(vy_proto::ProcessStdinParams {
                pid: params.pid,
                data: params.data,
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }
}
