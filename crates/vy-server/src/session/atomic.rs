//! Atomic file writes using temporary files and rename.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `contents` to `path` atomically.
///
/// A temporary file is written in the same directory, flushed with `sync_all`,
/// given the target file's permissions if it already exists, and renamed over
/// `path`. If an error occurs, the temporary file is removed.
pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_name = format!(".{file_name}.tmp-{timestamp}-{count}");
    let temp_path = parent.join(&temp_name);

    // Capture permissions from existing file if present.
    let existing_permissions = std::fs::metadata(path).ok().map(|m| m.permissions());

    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;

        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);

        if let Some(perms) = existing_permissions {
            let _ = std::fs::set_permissions(&temp_path, perms);
        }

        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }

    write_result
}

/// Last known file metadata (modification time and file size) for external change detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMetadata {
    pub mtime: SystemTime,
    pub size: u64,
}

impl FileMetadata {
    pub fn from_path(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let mtime = meta.modified().ok()?;
        Some(Self {
            mtime,
            size: meta.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vy-atomic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_atomic_write_new_file() {
        let dir = test_dir("new");
        let target = dir.join("new_file.txt");

        atomic_write(&target, b"hello atomic world").unwrap();

        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "hello atomic world"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let dir = test_dir("overwrite");
        let target = dir.join("existing.txt");
        std::fs::write(&target, "initial content").unwrap();

        atomic_write(&target, b"updated content").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "updated content");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_atomic_write_preserves_permissions() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = test_dir("perms");
            let target = dir.join("executable.sh");
            std::fs::write(&target, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();

            atomic_write(&target, b"#!/bin/sh\necho hello\n").unwrap();

            let perms = std::fs::metadata(&target).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o755);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn test_atomic_write_fails_nonexistent_dir() {
        let nonexistent = Path::new("/path/that/definitely/does/not/exist/file.txt");
        let result = atomic_write(nonexistent, b"content");
        assert!(result.is_err());
    }
}
