//! Locations of server-side state.
//!
//! The server resolves these on the machine where it runs, so a remote session
//! keeps its layout on the remote machine. `VY_DATA_HOME` overrides the default
//! for tests and development builds.

use std::path::PathBuf;

const APP_DIR: &str = "vy";

pub fn data_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("VY_DATA_HOME") {
        return PathBuf::from(value);
    }
    dirs::data_dir()
        .map(|base| base.join(APP_DIR))
        .unwrap_or_else(|| PathBuf::from(".vy"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_the_app_name_or_an_override() {
        assert!(data_dir().ends_with("vy") || std::env::var_os("VY_DATA_HOME").is_some());
    }
}
