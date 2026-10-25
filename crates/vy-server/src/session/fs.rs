//! File system operations for `vy-server`.

use std::path::PathBuf;

use vy_core::Uri;
use vy_proto::{
    DirEntry, ErrorCode, FileStat, FsCopy, FsCopyParams, FsCopyResult, FsCreateDir,
    FsCreateDirParams, FsCreateDirResult, FsExists, FsExistsParams, FsExistsResult, FsReadDir,
    FsReadDirParams, FsReadDirResult, FsReadFile, FsReadFileParams, FsReadFileResult,
    FsReadTextFile, FsReadTextFileParams, FsReadTextFileResult, FsRemove, FsRemoveParams,
    FsRemoveResult, FsRename, FsRenameParams, FsRenameResult, FsStat, FsStatParams, FsStatResult,
    FsWriteFile, FsWriteFileParams, FsWriteFileResult, RpcMessage,
};

use super::{Outcome, Session, atomic, ok_response};

/// Converts a URI or file path into a local `PathBuf`.
pub fn uri_to_path(uri_str: &str) -> Result<PathBuf, (ErrorCode, String)> {
    if uri_str.starts_with('/') {
        return Ok(PathBuf::from(uri_str));
    }
    if let Ok(uri) = Uri::parse(uri_str) {
        if uri.scheme() != "file" {
            return Err((
                ErrorCode::InvalidParams,
                format!("expected a file:// URI, found {uri}"),
            ));
        }
        return uri.file_path().ok_or_else(|| {
            (
                ErrorCode::InvalidParams,
                format!("cannot convert URI to path: {uri}"),
            )
        });
    }
    std::env::current_dir()
        .map(|pwd| pwd.join(uri_str))
        .map_err(|e| {
            (
                ErrorCode::Internal,
                format!("failed to get current dir: {e}"),
            )
        })
}

impl Session {
    pub(super) fn fs_read_file(&self, id: u64, params: FsReadFileParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::read(&path) {
            Ok(bytes) => ok_response::<FsReadFile>(id, &FsReadFileResult { bytes }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::NotFound,
                    format!("{}: {err}", path.display()),
                ))
            }
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_read_text_file(&self, id: u64, params: FsReadTextFileParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => ok_response::<FsReadTextFile>(id, &FsReadTextFileResult { text }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::NotFound,
                    format!("{}: {err}", path.display()),
                ))
            }
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_write_file(&self, id: u64, params: FsWriteFileParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        // When the caller supplies an expected state, reject the write if
        // the file on disk no longer matches it, unless forced. With no
        // expected state the write is unconditional, as before.
        if !params.force.unwrap_or(false)
            && (params.expected_mtime_ms.is_some() || params.expected_size.is_some())
        {
            let current = atomic::file_state(&path);
            let changed = match current {
                None => true,
                Some(state) => {
                    params
                        .expected_mtime_ms
                        .is_some_and(|mtime| state.mtime_ms != Some(mtime))
                        || params
                            .expected_size
                            .is_some_and(|size| state.size != size)
                }
            };
            if changed {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::InvalidState,
                    format!(
                        "file changed on disk since it was checked (pass force to overwrite): {}",
                        path.display()
                    ),
                ));
            }
        }
        let bytes_to_write = if let Some(bytes) = params.bytes {
            bytes
        } else if let Some(text) = params.text {
            text.into_bytes()
        } else {
            Vec::new()
        };
        let len = bytes_to_write.len() as u64;
        match atomic::atomic_write(&path, &bytes_to_write) {
            Ok(()) => ok_response::<FsWriteFile>(id, &FsWriteFileResult { bytes_written: len }),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_read_dir(&self, id: u64, params: FsReadDirParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let read_dir = match std::fs::read_dir(&path) {
            Ok(rd) => rd,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::NotFound,
                    format!("{}: {err}", path.display()),
                ));
            }
            Err(err) => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::Internal,
                    format!("{}: {err}", path.display()),
                ));
            }
        };
        let mut entries = Vec::new();
        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let file_type = entry.file_type().ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path();
            let entry_uri = format!("file://{}", entry_path.display());
            entries.push(DirEntry {
                name,
                uri: entry_uri,
                is_file: file_type.as_ref().map(|t| t.is_file()).unwrap_or(false),
                is_directory: file_type.as_ref().map(|t| t.is_dir()).unwrap_or(false),
                is_symlink: file_type.as_ref().map(|t| t.is_symlink()).unwrap_or(false),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        ok_response::<FsReadDir>(id, &FsReadDirResult { entries })
    }

    pub(super) fn fs_stat(&self, id: u64, params: FsStatParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::NotFound,
                    format!("{}: {err}", path.display()),
                ));
            }
            Err(err) => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::Internal,
                    format!("{}: {err}", path.display()),
                ));
            }
        };
        let mtime_ms = metadata.modified().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as u64)
        });
        let file_type = metadata.file_type();
        let stat = FileStat {
            is_file: file_type.is_file(),
            is_directory: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
            size: metadata.len(),
            mtime_ms,
        };
        ok_response::<FsStat>(id, &FsStatResult { stat })
    }

    pub(super) fn fs_exists(&self, id: u64, params: FsExistsParams) -> Outcome {
        let exists = match uri_to_path(&params.uri) {
            Ok(path) => path.exists(),
            Err(_) => false,
        };
        ok_response::<FsExists>(id, &FsExistsResult { exists })
    }

    pub(super) fn fs_create_dir(&self, id: u64, params: FsCreateDirParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let result = if params.recursive {
            std::fs::create_dir_all(&path)
        } else {
            std::fs::create_dir(&path)
        };
        match result {
            Ok(()) => ok_response::<FsCreateDir>(id, &FsCreateDirResult { success: true }),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_remove(&self, id: u64, params: FsRemoveParams) -> Outcome {
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let result = if path.is_dir() {
            if params.recursive {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_dir(&path)
            }
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(()) => ok_response::<FsRemove>(id, &FsRemoveResult { success: true }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::NotFound,
                    format!("{}: {err}", path.display()),
                ))
            }
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_rename(&self, id: u64, params: FsRenameParams) -> Outcome {
        let from = match uri_to_path(&params.from) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let to = match uri_to_path(&params.to) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::rename(&from, &to) {
            Ok(()) => ok_response::<FsRename>(id, &FsRenameResult { success: true }),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", from.display()),
            )),
        }
    }

    pub(super) fn fs_copy(&self, id: u64, params: FsCopyParams) -> Outcome {
        let from = match uri_to_path(&params.from) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let to = match uri_to_path(&params.to) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::copy(&from, &to) {
            Ok(_) => ok_response::<FsCopy>(id, &FsCopyResult { success: true }),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", from.display()),
            )),
        }
    }
}
