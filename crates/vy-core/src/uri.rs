use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 文書を指す値です。`file:` は実在するファイル、`vy:` は実体を持たない文書です。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Uri(String);

impl Uri {
    pub fn parse(value: impl Into<String>) -> Result<Self, UriError> {
        let value = value.into();
        match value.split_once(':') {
            Some(("file" | "vy", _)) => Ok(Self(value)),
            _ => Err(UriError::UnsupportedScheme(value)),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn scheme(&self) -> &str {
        self.0
            .split_once(':')
            .map(|(scheme, _)| scheme)
            .unwrap_or_default()
    }

    /// `file:` の URI をローカルのパスに変換します。`localhost` 以外の権限は扱いません。
    pub fn file_path(&self) -> Option<PathBuf> {
        let rest = self.0.strip_prefix("file:")?;
        let rest = rest.strip_prefix("//").unwrap_or(rest);
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        Some(PathBuf::from(percent_decode(rest)))
    }
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UriError {
    #[error("対応していない scheme です: {0}")]
    UnsupportedScheme(String),
}

/// `%XX` を復号します。不正な並びはそのまま残します。
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_file_and_vy_schemes() {
        assert_eq!(
            Uri::parse("file:///tmp/a.txt").expect("file").scheme(),
            "file"
        );
        assert_eq!(Uri::parse("vy:untitled-1").expect("vy").scheme(), "vy");
    }

    #[test]
    fn rejects_other_schemes() {
        assert_eq!(
            Uri::parse("https://example.com"),
            Err(UriError::UnsupportedScheme("https://example.com".into()))
        );
    }

    #[test]
    fn converts_file_uris_to_paths() {
        assert_eq!(
            Uri::parse("file:///tmp/a%20b.txt")
                .expect("uri")
                .file_path(),
            Some(PathBuf::from("/tmp/a b.txt"))
        );
        assert_eq!(
            Uri::parse("file://localhost/tmp/c.txt")
                .expect("uri")
                .file_path(),
            Some(PathBuf::from("/tmp/c.txt"))
        );
        assert_eq!(Uri::parse("vy:diff").expect("uri").file_path(), None);
    }
}
