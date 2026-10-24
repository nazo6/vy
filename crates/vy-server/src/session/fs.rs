//! File system operations for `vy-server`.

use std::path::PathBuf;

use vy_core::Uri;
use vy_proto::{
    DirEntry, ErrorCode, FileStat, FsCopyParams, FsCopyResult, FsCreateDirParams,
    FsCreateDirResult, FsExistsParams, FsExistsResult, FsReadDirParams, FsReadDirResult,
    FsReadFileParams, FsReadFileResult, FsReadTextFileParams, FsReadTextFileResult, FsRemoveParams,
    FsRemoveResult, FsRenameParams, FsRenameResult, FsStatParams, FsStatResult, FsWriteFileParams,
    FsWriteFileResult, RpcMessage, decode_value, to_value,
};

use super::{Outcome, Session};

/// Converts a URI or absolute file path into a local `PathBuf`.
pub fn uri_to_path(uri_str: &str) -> Result<PathBuf, (ErrorCode, String)> {
    if uri_str.starts_with('/') {
        return Ok(PathBuf::from(uri_str));
    }
    let uri =
        Uri::parse(uri_str).map_err(|e| (ErrorCode::InvalidParams, format!("invalid URI: {e}")))?;
    if uri.scheme() != "file" {
        return Err((
            ErrorCode::InvalidParams,
            format!("expected a file:// URI, found {uri}"),
        ));
    }
    uri.file_path().ok_or_else(|| {
        (
            ErrorCode::InvalidParams,
            format!("cannot convert URI to path: {uri}"),
        )
    })
}

impl Session {
    pub(super) fn fs_read_file(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsReadFileParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/read_file params",
            ));
        };
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::read(&path) {
            Ok(bytes) => Outcome::one(RpcMessage::ok(id, to_value(&FsReadFileResult { bytes }))),
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

    pub(super) fn fs_read_text_file(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsReadTextFileParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/read_text_file params",
            ));
        };
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Outcome::one(RpcMessage::ok(id, to_value(&FsReadTextFileResult { text }))),
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

    pub(super) fn fs_write_file(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsWriteFileParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/write_file params",
            ));
        };
        let path = match uri_to_path(&params.uri) {
            Ok(path) => path,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let bytes_to_write = if let Some(bytes) = params.bytes {
            bytes
        } else if let Some(text) = params.text {
            text.into_bytes()
        } else {
            Vec::new()
        };
        let len = bytes_to_write.len() as u64;
        match std::fs::write(&path, bytes_to_write) {
            Ok(()) => Outcome::one(RpcMessage::ok(
                id,
                to_value(&FsWriteFileResult { bytes_written: len }),
            )),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_read_dir(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsReadDirParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/read_dir params",
            ));
        };
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
        Outcome::one(RpcMessage::ok(id, to_value(&FsReadDirResult { entries })))
    }

    pub(super) fn fs_stat(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsStatParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/stat params",
            ));
        };
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
        Outcome::one(RpcMessage::ok(id, to_value(&FsStatResult { stat })))
    }

    pub(super) fn fs_exists(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsExistsParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/exists params",
            ));
        };
        let exists = match uri_to_path(&params.uri) {
            Ok(path) => path.exists(),
            Err(_) => false,
        };
        Outcome::one(RpcMessage::ok(id, to_value(&FsExistsResult { exists })))
    }

    pub(super) fn fs_create_dir(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsCreateDirParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/create_dir params",
            ));
        };
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
            Ok(()) => Outcome::one(RpcMessage::ok(
                id,
                to_value(&FsCreateDirResult { success: true }),
            )),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", path.display()),
            )),
        }
    }

    pub(super) fn fs_remove(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsRemoveParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/remove params",
            ));
        };
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
            Ok(()) => Outcome::one(RpcMessage::ok(
                id,
                to_value(&FsRemoveResult { success: true }),
            )),
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

    pub(super) fn fs_rename(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsRenameParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/rename params",
            ));
        };
        let from = match uri_to_path(&params.from) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let to = match uri_to_path(&params.to) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::rename(&from, &to) {
            Ok(()) => Outcome::one(RpcMessage::ok(
                id,
                to_value(&FsRenameResult { success: true }),
            )),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", from.display()),
            )),
        }
    }

    pub(super) fn fs_copy(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<FsCopyParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid fs/copy params",
            ));
        };
        let from = match uri_to_path(&params.from) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        let to = match uri_to_path(&params.to) {
            Ok(p) => p,
            Err((code, msg)) => return Outcome::one(RpcMessage::err(id, code, msg)),
        };
        match std::fs::copy(&from, &to) {
            Ok(_) => Outcome::one(RpcMessage::ok(
                id,
                to_value(&FsCopyResult { success: true }),
            )),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {err}", from.display()),
            )),
        }
    }
}
