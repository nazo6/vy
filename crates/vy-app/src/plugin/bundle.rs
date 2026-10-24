//! Validation for bundled plugin entry points.
//!
//! Bundle validation is separate from discovery so the same checks run when a
//! plugin is activated or reloaded. See ADR 0021.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use vy_js::BundleSyntaxError;
use vy_js::bundle::validate_esm;

/// Initial size limit for a bundle. Revisit it when remote transfer is added.
pub const MAX_BUNDLE_BYTES: u64 = 4 * 1024 * 1024;

/// Size and content hash of a validated bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleInfo {
    pub size: u64,
    pub sha256: String,
}

/// Why a bundle could not be loaded.
#[derive(Debug)]
pub enum BundleError {
    Io {
        path: PathBuf,
        message: String,
    },
    TooLarge {
        size: u64,
        limit: u64,
    },
    NotUtf8 {
        path: PathBuf,
    },
    Syntax {
        path: PathBuf,
        error: BundleSyntaxError,
    },
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(formatter, "cannot read {}: {message}", path.display())
            }
            Self::TooLarge { size, limit } => write!(
                formatter,
                "bundle is too large: {size} bytes (limit {limit} bytes)"
            ),
            Self::NotUtf8 { path } => {
                write!(formatter, "{} is not UTF-8 JavaScript", path.display())
            }
            Self::Syntax { path, error } => write!(formatter, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for BundleError {}

/// Reads and validates a bundle, returning its size and SHA-256.
pub fn validate_bundle(path: &Path, max_bytes: u64) -> Result<BundleInfo, BundleError> {
    let metadata = std::fs::metadata(path).map_err(|error| BundleError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if metadata.len() > max_bytes {
        return Err(BundleError::TooLarge {
            size: metadata.len(),
            limit: max_bytes,
        });
    }

    let bytes = std::fs::read(path).map_err(|error| BundleError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if bytes.len() as u64 > max_bytes {
        return Err(BundleError::TooLarge {
            size: bytes.len() as u64,
            limit: max_bytes,
        });
    }

    let source = std::str::from_utf8(&bytes).map_err(|_| BundleError::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    validate_esm(source).map_err(|error| BundleError::Syntax {
        path: path.to_path_buf(),
        error,
    })?;

    Ok(BundleInfo {
        size: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("vy-bundle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create temp directory");
        directory
    }

    #[test]
    fn validates_and_hashes_a_bundle() {
        let directory = directory("hash");
        let path = directory.join("frontend.js");
        let source = "export const answer = 42;\n";
        std::fs::write(&path, source).expect("write bundle");

        let info = validate_bundle(&path, 1024).expect("valid bundle");
        assert_eq!(info.size, source.len() as u64);
        assert_eq!(
            info.sha256,
            "a2098bd92b10bf8b816d24b7556b1ce8c49a879d130489065ef1051c17e042f6"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn rejects_remaining_imports() {
        let directory = directory("import");
        let path = directory.join("frontend.js");
        std::fs::write(&path, "import \"zod\";\n").expect("write bundle");

        let error = validate_bundle(&path, 1024).expect_err("import should be rejected");
        match error {
            BundleError::Syntax { error, .. } => {
                assert!(error.message.contains("zod"), "{error}");
            }
            other => panic!("expected a syntax error, got {other}"),
        }

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn rejects_oversized_bundles() {
        let directory = directory("size");
        let path = directory.join("frontend.js");
        std::fs::write(&path, "export const answer = 42;\n").expect("write bundle");

        let error = validate_bundle(&path, 10).expect_err("bundle should be too large");
        assert!(matches!(error, BundleError::TooLarge { .. }));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn rejects_non_utf8_bundles() {
        let directory = directory("utf8");
        let path = directory.join("frontend.js");
        std::fs::write(&path, [0xff, 0xfe]).expect("write bundle");

        let error = validate_bundle(&path, 1024).expect_err("bundle should be rejected");
        assert!(matches!(error, BundleError::NotUtf8 { .. }));

        let _ = std::fs::remove_dir_all(&directory);
    }
}
