//! Shared `fs` method declarations.
//!
//! The client forwards these calls to the core over RPC. The core implements
//! them against the workspace on the machine where it runs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};
use vy_core::Uri;
use vy_proto::{DirEntry, FileStat};

use crate::method_decl;

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

method_decl!(
    READ_FILE,
    "fs.readFile",
    FsReadFileParams,
    FsReadFileReply,
    "Reads the binary contents of a file."
);
method_decl!(
    READ_TEXT_FILE,
    "fs.readTextFile",
    FsReadTextFileParams,
    FsReadTextFileReply,
    "Reads the UTF-8 text contents of a file."
);
method_decl!(
    WRITE_FILE,
    "fs.writeFile",
    FsWriteFileParams,
    FsWriteFileReply,
    "Writes binary or text contents to a file."
);
method_decl!(
    READ_DIR,
    "fs.readDir",
    FsReadDirParams,
    FsReadDirReply,
    "Lists entries in a directory."
);
method_decl!(
    STAT,
    "fs.stat",
    FsStatParams,
    FsStatReply,
    "Retrieves metadata about a file or directory."
);
method_decl!(
    EXISTS,
    "fs.exists",
    FsExistsParams,
    FsExistsReply,
    "Checks whether a file or directory exists."
);
method_decl!(
    CREATE_DIR,
    "fs.createDir",
    FsCreateDirParams,
    FsCreateDirReply,
    "Creates a directory."
);
method_decl!(
    REMOVE,
    "fs.remove",
    FsRemoveParams,
    FsRemoveReply,
    "Removes a file or directory."
);
method_decl!(
    RENAME,
    "fs.rename",
    FsRenameParams,
    FsRenameReply,
    "Renames or moves a file or directory."
);
method_decl!(
    COPY,
    "fs.copy",
    FsCopyParams,
    FsCopyReply,
    "Copies a file or directory."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[
    READ_FILE,
    READ_TEXT_FILE,
    WRITE_FILE,
    READ_DIR,
    STAT,
    EXISTS,
    CREATE_DIR,
    REMOVE,
    RENAME,
    COPY,
];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "fs",
    doc: "Reads, writes, and inspects files in the workspace.",
    sugar: None,
};
