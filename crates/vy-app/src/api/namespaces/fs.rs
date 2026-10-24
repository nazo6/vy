//! `fs` namespace.

/// Reads, writes, and inspects files in the workspace.
#[vy_api_schema::vy_api_namespace("fs")]
mod api {
    use vy_api_schema::{
        FsCopyParams, FsCopyReply, FsCreateDirParams, FsCreateDirReply, FsExistsParams,
        FsExistsReply, FsReadDirParams, FsReadDirReply, FsReadFileParams, FsReadFileReply,
        FsReadTextFileParams, FsReadTextFileReply, FsRemoveParams, FsRemoveReply, FsRenameParams,
        FsRenameReply, FsStatParams, FsStatReply, FsWriteFileParams, FsWriteFileReply,
    };
    use vy_proto::to_value;

    use crate::api::{Call, CallError, Reply, check_capability};
    use crate::plugin_host::HostAction;

    /// Reads the binary contents of a file.
    #[vy_api_schema::vy_api_method]
    fn read_file(
        call: &mut Call<'_, '_>,
        params: FsReadFileParams,
    ) -> Result<Reply<FsReadFileReply>, CallError> {
        check_capability(call.context, "fs:read", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/read_file".into(),
            params: to_value(&vy_proto::FsReadFileParams {
                uri: params.uri.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Reads the UTF-8 text contents of a file.
    #[vy_api_schema::vy_api_method]
    fn read_text_file(
        call: &mut Call<'_, '_>,
        params: FsReadTextFileParams,
    ) -> Result<Reply<FsReadTextFileReply>, CallError> {
        check_capability(call.context, "fs:read", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/read_text_file".into(),
            params: to_value(&vy_proto::FsReadTextFileParams {
                uri: params.uri.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Writes binary or text contents to a file.
    #[vy_api_schema::vy_api_method]
    fn write_file(
        call: &mut Call<'_, '_>,
        params: FsWriteFileParams,
    ) -> Result<Reply<FsWriteFileReply>, CallError> {
        check_capability(call.context, "fs:write", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/write_file".into(),
            params: to_value(&vy_proto::FsWriteFileParams {
                uri: params.uri.to_string(),
                bytes: params.bytes,
                text: params.text,
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Lists entries in a directory.
    #[vy_api_schema::vy_api_method]
    fn read_dir(
        call: &mut Call<'_, '_>,
        params: FsReadDirParams,
    ) -> Result<Reply<FsReadDirReply>, CallError> {
        check_capability(call.context, "fs:read", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/read_dir".into(),
            params: to_value(&vy_proto::FsReadDirParams {
                uri: params.uri.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Retrieves metadata about a file or directory.
    #[vy_api_schema::vy_api_method]
    fn stat(
        call: &mut Call<'_, '_>,
        params: FsStatParams,
    ) -> Result<Reply<FsStatReply>, CallError> {
        check_capability(call.context, "fs:read", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/stat".into(),
            params: to_value(&vy_proto::FsStatParams {
                uri: params.uri.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Checks whether a file or directory exists.
    #[vy_api_schema::vy_api_method]
    fn exists(
        call: &mut Call<'_, '_>,
        params: FsExistsParams,
    ) -> Result<Reply<FsExistsReply>, CallError> {
        check_capability(call.context, "fs:read", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/exists".into(),
            params: to_value(&vy_proto::FsExistsParams {
                uri: params.uri.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Creates a directory.
    #[vy_api_schema::vy_api_method]
    fn create_dir(
        call: &mut Call<'_, '_>,
        params: FsCreateDirParams,
    ) -> Result<Reply<FsCreateDirReply>, CallError> {
        check_capability(call.context, "fs:write", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/create_dir".into(),
            params: to_value(&vy_proto::FsCreateDirParams {
                uri: params.uri.to_string(),
                recursive: params.recursive.unwrap_or(false),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Removes a file or directory.
    #[vy_api_schema::vy_api_method]
    fn remove(
        call: &mut Call<'_, '_>,
        params: FsRemoveParams,
    ) -> Result<Reply<FsRemoveReply>, CallError> {
        check_capability(call.context, "fs:write", Some(params.uri.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/remove".into(),
            params: to_value(&vy_proto::FsRemoveParams {
                uri: params.uri.to_string(),
                recursive: params.recursive.unwrap_or(false),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Renames or moves a file or directory.
    #[vy_api_schema::vy_api_method]
    fn rename(
        call: &mut Call<'_, '_>,
        params: FsRenameParams,
    ) -> Result<Reply<FsRenameReply>, CallError> {
        check_capability(call.context, "fs:write", Some(params.from.as_str()))?;
        check_capability(call.context, "fs:write", Some(params.to.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/rename".into(),
            params: to_value(&vy_proto::FsRenameParams {
                from: params.from.to_string(),
                to: params.to.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }

    /// Copies a file or directory.
    #[vy_api_schema::vy_api_method]
    fn copy(
        call: &mut Call<'_, '_>,
        params: FsCopyParams,
    ) -> Result<Reply<FsCopyReply>, CallError> {
        check_capability(call.context, "fs:read", Some(params.from.as_str()))?;
        check_capability(call.context, "fs:write", Some(params.to.as_str()))?;
        call.outcome.action = HostAction::ServerRpc {
            method: "fs/copy".into(),
            params: to_value(&vy_proto::FsCopyParams {
                from: params.from.to_string(),
                to: params.to.to_string(),
            }),
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
