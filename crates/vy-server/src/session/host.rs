//! Core-side implementation of the shared host traits.
//!
//! This is a prototype used by the validation tests. It shows that `HostApi`
//! can be implemented against the session and core filesystem without going
//! through the client RPC path.

use std::path::PathBuf;

use vy_capabilities::{Capability, CapabilitySet, Roots};
use vy_core::{Position, Uri};
use vy_host_api::fs::{
    FsApi, FsCopyParams, FsCopyReply, FsCreateDirParams, FsCreateDirReply, FsExistsParams,
    FsExistsReply, FsReadDirParams, FsReadDirReply, FsReadFileParams, FsReadFileReply,
    FsReadTextFileParams, FsReadTextFileReply, FsRemoveParams, FsRemoveReply, FsRenameParams,
    FsRenameReply, FsStatParams, FsStatReply, FsWriteFileParams, FsWriteFileReply,
};
use vy_host_api::workspace::{WorkspaceApi, WorkspaceInfoReply};
use vy_host_api::{CallError, DocumentInfoReply, HostApi, Reply};
use vy_proto::{DirEntry, ErrorCode, FileStat};

use super::Session;
use super::fs::uri_to_path;

/// A core host used while validating the trait-based API.
pub struct CoreHost<'a> {
    pub session: &'a mut Session,
    pub capabilities: CapabilitySet,
    pub roots: Roots,
}

impl CoreHost<'_> {
    fn path(&self, uri: &str) -> Result<PathBuf, CallError> {
        uri_to_path(uri).map_err(|(code, message)| CallError::new(code, message))
    }

    fn check(&self, kind: &str, target: &str) -> Result<(), CallError> {
        // Capability patterns use paths (`${workspace}/**`), while the API
        // passes file URIs. Normalize before matching.
        let target = if target.starts_with("file:") {
            Uri::parse(target)
                .ok()
                .and_then(|uri| uri.file_path())
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| target.to_string())
        } else {
            target.to_string()
        };
        let requested = Capability::parse(&format!("{kind}:{target}"))
            .map_err(|error| CallError::invalid_params(error.to_string()))?;
        if self.capabilities.has(&requested, &self.roots) {
            Ok(())
        } else {
            Err(CallError::new(
                ErrorCode::CapabilityDenied,
                format!("capability denied: {kind}:{target}"),
            ))
        }
    }
}

fn io_error(path: &std::path::Path, error: std::io::Error) -> CallError {
    let code = if error.kind() == std::io::ErrorKind::NotFound {
        ErrorCode::NotFound
    } else {
        ErrorCode::Internal
    };
    CallError::new(code, format!("{}: {error}", path.display()))
}

impl vy_host_api::config::ConfigReadApi for CoreHost<'_> {
    fn get(
        &mut self,
        _params: vy_host_api::config::ConfigGetParams,
    ) -> Result<Reply<vy_host_api::Json>, CallError> {
        Ok(Reply::Value(vy_host_api::Json(serde_json::Value::Null)))
    }

    fn inspect(
        &mut self,
        _params: vy_host_api::config::ConfigInspectParams,
    ) -> Result<Reply<vy_host_api::config::ConfigInspectReply>, CallError> {
        Ok(Reply::Value(vy_host_api::config::ConfigInspectReply {
            default: None,
            user: None,
            project: None,
            memory: None,
            effective: None,
        }))
    }
}

impl WorkspaceApi for CoreHost<'_> {
    fn info(&mut self) -> Result<Reply<WorkspaceInfoReply>, CallError> {
        let root_uri = self.session.workspace.clone();
        let name = root_uri
            .as_ref()
            .and_then(|uri| uri.file_path())
            .and_then(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            });
        Ok(Reply::Value(WorkspaceInfoReply {
            root_uri,
            name,
            folders: self.session.workspace.iter().cloned().collect::<Vec<_>>(),
            trusted: true,
        }))
    }

    fn text_documents(&mut self) -> Result<Reply<Vec<DocumentInfoReply>>, CallError> {
        let documents = self
            .session
            .documents
            .values()
            .map(|buffer| DocumentInfoReply {
                id: buffer.id(),
                uri: buffer.uri().clone(),
                language_id: String::new(),
                version: buffer.version(),
                line_count: buffer.line_count(),
                end: buffer
                    .char_to_position(buffer.len_chars())
                    .unwrap_or(Position::new(0, 0)),
            })
            .collect();
        Ok(Reply::Value(documents))
    }
}

impl FsApi for CoreHost<'_> {
    fn read_file(&mut self, params: FsReadFileParams) -> Result<Reply<FsReadFileReply>, CallError> {
        self.check("fs:read", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let bytes = std::fs::read(&path).map_err(|error| io_error(&path, error))?;
        Ok(Reply::Value(FsReadFileReply { bytes }))
    }

    fn read_text_file(
        &mut self,
        params: FsReadTextFileParams,
    ) -> Result<Reply<FsReadTextFileReply>, CallError> {
        self.check("fs:read", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let text = std::fs::read_to_string(&path).map_err(|error| io_error(&path, error))?;
        Ok(Reply::Value(FsReadTextFileReply { text }))
    }

    fn write_file(
        &mut self,
        params: FsWriteFileParams,
    ) -> Result<Reply<FsWriteFileReply>, CallError> {
        self.check("fs:write", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let bytes = params
            .bytes
            .unwrap_or_else(|| params.text.unwrap_or_default().into_bytes());
        let bytes_written = bytes.len() as u64;
        std::fs::write(&path, bytes).map_err(|error| io_error(&path, error))?;
        Ok(Reply::Value(FsWriteFileReply { bytes_written }))
    }

    fn read_dir(&mut self, params: FsReadDirParams) -> Result<Reply<FsReadDirReply>, CallError> {
        self.check("fs:read", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path).map_err(|error| io_error(&path, error))? {
            let Ok(entry) = entry else { continue };
            let file_type = entry.file_type().ok();
            let entry_path = entry.path();
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                uri: format!("file://{}", entry_path.display()),
                is_file: file_type.as_ref().is_some_and(|kind| kind.is_file()),
                is_directory: file_type.as_ref().is_some_and(|kind| kind.is_dir()),
                is_symlink: file_type.as_ref().is_some_and(|kind| kind.is_symlink()),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Reply::Value(FsReadDirReply { entries }))
    }

    fn stat(&mut self, params: FsStatParams) -> Result<Reply<FsStatReply>, CallError> {
        self.check("fs:read", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
        let file_type = metadata.file_type();
        Ok(Reply::Value(FsStatReply {
            stat: FileStat {
                is_file: file_type.is_file(),
                is_directory: file_type.is_dir(),
                is_symlink: file_type.is_symlink(),
                size: metadata.len(),
                mtime_ms: metadata.modified().ok().and_then(|time| {
                    time.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_millis() as u64)
                }),
            },
        }))
    }

    fn exists(&mut self, params: FsExistsParams) -> Result<Reply<FsExistsReply>, CallError> {
        self.check("fs:read", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        Ok(Reply::Value(FsExistsReply {
            exists: path.exists(),
        }))
    }

    fn create_dir(
        &mut self,
        params: FsCreateDirParams,
    ) -> Result<Reply<FsCreateDirReply>, CallError> {
        self.check("fs:write", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let result = if params.recursive.unwrap_or(false) {
            std::fs::create_dir_all(&path)
        } else {
            std::fs::create_dir(&path)
        };
        result.map_err(|error| io_error(&path, error))?;
        Ok(Reply::Value(FsCreateDirReply { success: true }))
    }

    fn remove(&mut self, params: FsRemoveParams) -> Result<Reply<FsRemoveReply>, CallError> {
        self.check("fs:write", params.uri.as_str())?;
        let path = self.path(params.uri.as_str())?;
        let result = if path.is_dir() {
            if params.recursive.unwrap_or(false) {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_dir(&path)
            }
        } else {
            std::fs::remove_file(&path)
        };
        result.map_err(|error| io_error(&path, error))?;
        Ok(Reply::Value(FsRemoveReply { success: true }))
    }

    fn rename(&mut self, params: FsRenameParams) -> Result<Reply<FsRenameReply>, CallError> {
        self.check("fs:write", params.from.as_str())?;
        self.check("fs:write", params.to.as_str())?;
        let from = self.path(params.from.as_str())?;
        let to = self.path(params.to.as_str())?;
        std::fs::rename(&from, &to).map_err(|error| io_error(&from, error))?;
        Ok(Reply::Value(FsRenameReply { success: true }))
    }

    fn copy(&mut self, params: FsCopyParams) -> Result<Reply<FsCopyReply>, CallError> {
        self.check("fs:read", params.from.as_str())?;
        self.check("fs:write", params.to.as_str())?;
        let from = self.path(params.from.as_str())?;
        let to = self.path(params.to.as_str())?;
        std::fs::copy(&from, &to).map_err(|error| io_error(&from, error))?;
        Ok(Reply::Value(FsCopyReply { success: true }))
    }
}

impl HostApi for CoreHost<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn temp_workspace(name: &str) -> (PathBuf, String, String) {
        let directory =
            std::env::temp_dir().join(format!("vy-core-host-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("workspace");
        let file = directory.join("note.txt");
        std::fs::write(&file, "hello").expect("file");
        let workspace_uri = format!("file://{}", directory.display());
        let file_uri = format!("file://{}", file.display());
        (directory, workspace_uri, file_uri)
    }

    #[test]
    fn dispatches_workspace_and_fs_through_host_api() {
        let (directory, workspace_uri, file_uri) = temp_workspace("dispatch");
        let workspace = Uri::parse(workspace_uri).expect("workspace uri");
        let mut session = Session::new();
        session.workspace = Some(workspace.clone());
        session
            .document_for_uri(Uri::parse(file_uri.clone()).expect("file uri"))
            .expect("open document");

        let capabilities =
            CapabilitySet::from_declarations(&["fs:read:${workspace}/**".into()]).expect("caps");
        let roots = Roots::new(Some(directory.clone()), None);
        let mut host = CoreHost {
            session: &mut session,
            capabilities,
            roots,
        };

        let result = vy_host_api::HostApi::dispatch(&mut host, "workspace.info", &Value::Null)
            .expect("workspace.info");
        let Ok(Reply::Value(value)) = result else {
            panic!("workspace.info should return a value");
        };
        assert_eq!(
            value.get("name").and_then(|value| value.as_str()),
            directory.file_name().and_then(|name| name.to_str())
        );

        let params = json!({ "uri": file_uri });
        let result =
            vy_host_api::HostApi::dispatch(&mut host, "fs.readFile", &params).expect("fs.readFile");
        let Ok(Reply::Value(value)) = result else {
            panic!("fs.readFile should return a value");
        };
        assert_eq!(
            value
                .get("bytes")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(5)
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn denies_fs_capabilities() {
        let (directory, workspace_uri, file_uri) = temp_workspace("denied");
        let workspace = Uri::parse(workspace_uri).expect("workspace uri");
        let mut session = Session::new();
        session.workspace = Some(workspace);
        session
            .document_for_uri(Uri::parse(file_uri.clone()).expect("file uri"))
            .expect("open document");

        let mut host = CoreHost {
            session: &mut session,
            capabilities: CapabilitySet::default(),
            roots: Roots::new(Some(directory.clone()), None),
        };
        let params = json!({ "uri": file_uri });
        let result =
            vy_host_api::HostApi::dispatch(&mut host, "fs.readFile", &params).expect("fs.readFile");
        let Err(error) = result else {
            panic!("fs.readFile should be denied");
        };
        assert_eq!(error.code, ErrorCode::CapabilityDenied);

        let _ = std::fs::remove_dir_all(&directory);
    }
}
