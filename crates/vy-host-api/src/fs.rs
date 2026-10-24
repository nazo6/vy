//! Shared `fs` method declarations.
//!
//! The client forwards these calls to the core over RPC. The core implements
//! them against the workspace on the machine where it runs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_core::Uri;
use vy_proto::{DirEntry, FileStat};

use crate::{CallError, Reply};

/// `fs.readFile` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsReadFileParams {
    pub uri: Uri,
}

/// `fs.readFile` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsReadFileReply {
    pub bytes: Vec<u8>,
}

/// `fs.readTextFile` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsReadTextFileParams {
    pub uri: Uri,
}

/// `fs.readTextFile` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsReadTextFileReply {
    pub text: String,
}

/// `fs.writeFile` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteFileParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(optional)]
    pub bytes: Option<Vec<u8>>,
    #[serde(default)]
    #[ts(optional)]
    pub text: Option<String>,
}

/// `fs.writeFile` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteFileReply {
    pub bytes_written: u64,
}

/// `fs.readDir` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsReadDirParams {
    pub uri: Uri,
}

/// `fs.readDir` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsReadDirReply {
    pub entries: Vec<DirEntry>,
}

/// `fs.stat` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsStatParams {
    pub uri: Uri,
}

/// `fs.stat` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsStatReply {
    pub stat: FileStat,
}

/// `fs.exists` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsExistsParams {
    pub uri: Uri,
}

/// `fs.exists` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsExistsReply {
    pub exists: bool,
}

/// `fs.createDir` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsCreateDirParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(optional)]
    pub recursive: Option<bool>,
}

/// `fs.createDir` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsCreateDirReply {
    pub success: bool,
}

/// `fs.remove` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsRemoveParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(optional)]
    pub recursive: Option<bool>,
}

/// `fs.remove` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsRemoveReply {
    pub success: bool,
}

/// `fs.rename` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsRenameParams {
    pub from: Uri,
    pub to: Uri,
}

/// `fs.rename` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsRenameReply {
    pub success: bool,
}

/// `fs.copy` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsCopyParams {
    pub from: Uri,
    pub to: Uri,
}

/// `fs.copy` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FsCopyReply {
    pub success: bool,
}

/// Reads, writes, and inspects files in the workspace.
#[vy_api_schema::vy_api_trait("fs")]
pub trait FsApi {
    /// Reads the binary contents of a file.
    #[vy_api_schema::vy_api_method]
    fn read_file(&mut self, params: FsReadFileParams) -> Result<Reply<FsReadFileReply>, CallError>;

    /// Reads the UTF-8 text contents of a file.
    #[vy_api_schema::vy_api_method]
    fn read_text_file(
        &mut self,
        params: FsReadTextFileParams,
    ) -> Result<Reply<FsReadTextFileReply>, CallError>;

    /// Writes binary or text contents to a file.
    #[vy_api_schema::vy_api_method]
    fn write_file(
        &mut self,
        params: FsWriteFileParams,
    ) -> Result<Reply<FsWriteFileReply>, CallError>;

    /// Lists entries in a directory.
    #[vy_api_schema::vy_api_method]
    fn read_dir(&mut self, params: FsReadDirParams) -> Result<Reply<FsReadDirReply>, CallError>;

    /// Retrieves metadata about a file or directory.
    #[vy_api_schema::vy_api_method]
    fn stat(&mut self, params: FsStatParams) -> Result<Reply<FsStatReply>, CallError>;

    /// Checks whether a file or directory exists.
    #[vy_api_schema::vy_api_method]
    fn exists(&mut self, params: FsExistsParams) -> Result<Reply<FsExistsReply>, CallError>;

    /// Creates a directory.
    #[vy_api_schema::vy_api_method]
    fn create_dir(
        &mut self,
        params: FsCreateDirParams,
    ) -> Result<Reply<FsCreateDirReply>, CallError>;

    /// Removes a file or directory.
    #[vy_api_schema::vy_api_method]
    fn remove(&mut self, params: FsRemoveParams) -> Result<Reply<FsRemoveReply>, CallError>;

    /// Renames or moves a file or directory.
    #[vy_api_schema::vy_api_method]
    fn rename(&mut self, params: FsRenameParams) -> Result<Reply<FsRenameReply>, CallError>;

    /// Copies a file or directory.
    #[vy_api_schema::vy_api_method]
    fn copy(&mut self, params: FsCopyParams) -> Result<Reply<FsCopyReply>, CallError>;
}
