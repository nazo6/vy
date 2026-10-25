//! Atomic file writes for `vy-server`.
//!
//! A write goes to a temporary file in the same directory, is flushed to
//! disk, and is then renamed over the destination, so a crash mid-write
//! cannot leave a truncated file behind. The temporary file is removed
//! when any step fails. When the destination already exists, its
//! permissions are copied to the temporary file before the rename, so the
//! saved file keeps its original permissions.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// On-disk state recorded when a document is opened, used to detect
/// external changes before a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileState {
    pub(super) mtime_ms: Option<u64>,
    pub(super) size: u64,
}

/// Returns the current on-disk state of `path`, or `None` when the file
/// does not exist.
pub(super) fn file_state(path: &Path) -> Option<FileState> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime_ms = metadata.modified().ok().and_then(|time| {
        time.duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_millis() as u64)
    });
    Some(FileState {
        mtime_ms,
        size: metadata.len(),
    })
}

/// Writes `contents` to `path` atomically, as described in the module docs.
pub(super) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let original_permissions = std::fs::metadata(path).ok().map(|metadata| {
        metadata.permissions()
    });
    let temp_path = temp_path_for(path);
    let result = (|| -> io::Result<()> {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        if let Some(permissions) = original_permissions {
            temp.set_permissions(permissions)?;
        }
        temp.write_all(contents)?;
        temp.sync_all()?;
        drop(temp);
        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        std::fs::remove_file(&temp_path).ok();
    }
    result
}

fn temp_path_for(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vy".to_string());
    parent.join(format!(
        ".{file_name}.vy-tmp-{}-{counter}-{nanos}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_contents_and_keeps_permissions() {
        let dir = std::env::temp_dir().join(format!(
            "vy-atomic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("file.txt");
        std::fs::write(&path, "old contents").expect("write fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
                .expect("set fixture permissions");
        }

        atomic_write(&path, b"new contents").expect("atomic write");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read back"),
            "new contents"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o640
            );
        }
        assert_eq!(
            std::fs::read_dir(&dir)
                .expect("read dir")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".vy-tmp-"))
                .count(),
            0,
            "no temporary file is left behind"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_state_changes_when_contents_change() {
        let dir = std::env::temp_dir().join(format!(
            "vy-atomic-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("file.txt");
        std::fs::write(&path, "one").expect("write fixture");
        let before = file_state(&path).expect("state exists");
        std::fs::write(&path, "a much longer replacement").expect("overwrite fixture");
        let after = file_state(&path).expect("state exists");
        assert_ne!(before, after);
        std::fs::remove_dir_all(&dir).ok();
    }
}
