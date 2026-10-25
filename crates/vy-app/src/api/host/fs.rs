//! Client implementation of the `fs` host trait.

use vy_host_api::fs::{
    FsApi, FsCopyParams, FsCopyReply, FsCreateDirParams, FsCreateDirReply, FsExistsParams,
    FsExistsReply, FsReadDirParams, FsReadDirReply, FsReadFileParams, FsReadFileReply,
    FsReadTextFileParams, FsReadTextFileReply, FsRemoveParams, FsRemoveReply, FsRenameParams,
    FsRenameReply, FsStatParams, FsStatReply, FsWriteFileParams, FsWriteFileReply,
};
use vy_proto::RpcCall;

use super::ClientHost;
use crate::api::{CallError, Reply, check_capability};
use crate::plugin_host::HostAction;

impl FsApi for ClientHost<'_, '_, '_> {
    fn read_file(&mut self, params: FsReadFileParams) -> Result<Reply<FsReadFileReply>, CallError> {
        check_capability(self.call.context, "fs:read", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsReadFile>(vy_proto::FsReadFileParams {
                uri: params.uri.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn read_text_file(
        &mut self,
        params: FsReadTextFileParams,
    ) -> Result<Reply<FsReadTextFileReply>, CallError> {
        check_capability(self.call.context, "fs:read", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsReadTextFile>(vy_proto::FsReadTextFileParams {
                uri: params.uri.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn write_file(
        &mut self,
        params: FsWriteFileParams,
    ) -> Result<Reply<FsWriteFileReply>, CallError> {
        check_capability(self.call.context, "fs:write", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsWriteFile>(vy_proto::FsWriteFileParams {
                uri: params.uri.to_string(),
                bytes: params.bytes,
                text: params.text,
                expected_mtime_ms: None,
                expected_size: None,
                force: None,
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn read_dir(&mut self, params: FsReadDirParams) -> Result<Reply<FsReadDirReply>, CallError> {
        check_capability(self.call.context, "fs:read", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsReadDir>(vy_proto::FsReadDirParams {
                uri: params.uri.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn stat(&mut self, params: FsStatParams) -> Result<Reply<FsStatReply>, CallError> {
        check_capability(self.call.context, "fs:read", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsStat>(vy_proto::FsStatParams {
                uri: params.uri.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn exists(&mut self, params: FsExistsParams) -> Result<Reply<FsExistsReply>, CallError> {
        check_capability(self.call.context, "fs:read", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsExists>(vy_proto::FsExistsParams {
                uri: params.uri.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn create_dir(
        &mut self,
        params: FsCreateDirParams,
    ) -> Result<Reply<FsCreateDirReply>, CallError> {
        check_capability(self.call.context, "fs:write", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsCreateDir>(vy_proto::FsCreateDirParams {
                uri: params.uri.to_string(),
                recursive: params.recursive.unwrap_or(false),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn remove(&mut self, params: FsRemoveParams) -> Result<Reply<FsRemoveReply>, CallError> {
        check_capability(self.call.context, "fs:write", Some(params.uri.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsRemove>(vy_proto::FsRemoveParams {
                uri: params.uri.to_string(),
                recursive: params.recursive.unwrap_or(false),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn rename(&mut self, params: FsRenameParams) -> Result<Reply<FsRenameReply>, CallError> {
        check_capability(self.call.context, "fs:write", Some(params.from.as_str()))?;
        check_capability(self.call.context, "fs:write", Some(params.to.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsRename>(vy_proto::FsRenameParams {
                from: params.from.to_string(),
                to: params.to.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    fn copy(&mut self, params: FsCopyParams) -> Result<Reply<FsCopyReply>, CallError> {
        check_capability(self.call.context, "fs:read", Some(params.from.as_str()))?;
        check_capability(self.call.context, "fs:write", Some(params.to.as_str()))?;
        self.call.outcome.action = HostAction::ServerRpc {
            call: RpcCall::new::<vy_proto::FsCopy>(vy_proto::FsCopyParams {
                from: params.from.to_string(),
                to: params.to.to_string(),
            }),
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }
}
