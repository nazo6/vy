//! How the client reaches `vy-server`.
//!
//! Local connections start a child process. Remote connections use the system
//! `ssh` command and the same stdio framing. See `docs/adr/0010-remote-connection.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::paths::Paths;

/// The transport used for one connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionSpec {
    /// A `vy-server` child process on this machine.
    Local,
    /// `vy-server` started through SSH on another machine.
    Ssh {
        host: String,
        user: Option<String>,
        port: Option<u16>,
        /// Path or command name on the remote machine.
        server_path: String,
    },
}

impl ConnectionSpec {
    pub fn local() -> Self {
        Self::Local
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Ssh { .. })
    }

    /// A short name used in status messages and logs.
    pub fn display_name(&self) -> String {
        match self {
            Self::Local => "local".to_string(),
            Self::Ssh { host, user, .. } => match user {
                Some(user) => format!("{user}@{host}"),
                None => host.clone(),
            },
        }
    }

    /// Builds the command that carries the RPC stream.
    pub fn command(&self) -> Command {
        match self {
            Self::Local => local_server_command(),
            Self::Ssh {
                host,
                user,
                port,
                server_path,
            } => {
                let mut command = Command::new("ssh");
                // No pseudo-terminal: stdout must carry only RPC frames.
                command.arg("-T");
                if let Some(port) = port {
                    command.arg("-p").arg(port.to_string());
                }
                let destination = match user {
                    Some(user) => format!("{user}@{host}"),
                    None => host.clone(),
                };
                // `--` ends local option parsing, so it must come before the
                // destination. Everything after the destination is the remote
                // command.
                command.arg("--");
                command.arg(destination);
                command.arg(server_path);
                command.arg("--stdio");
                command
            }
        }
    }
}

/// One remote connection profile from `connections.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    /// Absolute path of the workspace on the remote machine.
    pub workspace: String,
    /// Remote home used to expand `${home}` in capabilities.
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default = "default_server_path")]
    pub server_path: String,
}

fn default_server_path() -> String {
    "vy-server".to_string()
}

impl ConnectionProfile {
    pub fn spec(&self) -> ConnectionSpec {
        ConnectionSpec::Ssh {
            host: self.host.clone(),
            user: self.user.clone(),
            port: self.port,
            server_path: self.server_path.clone(),
        }
    }

    pub fn workspace_uri(&self) -> String {
        format!("file://{}", self.workspace)
    }
}

/// The parsed `connections.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Connections {
    #[serde(default)]
    pub profiles: BTreeMap<String, ConnectionProfile>,
}

impl Connections {
    /// Reads the file. A missing file is an empty set; a broken file produces a
    /// warning and an empty set so startup is not blocked.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str(&text) {
            Ok(connections) => (connections, None),
            Err(error) => (
                Self::default(),
                Some(format!("could not read connections.json: {error}")),
            ),
        }
    }

    pub fn profile(&self, name: &str) -> Option<&ConnectionProfile> {
        self.profiles.get(name)
    }
}

/// Values fixed at startup. The view keeps them for reconnects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    pub connection: ConnectionSpec,
    pub connection_name: String,
    pub workspace_uri: String,
    /// Path used to expand `${workspace}` and to resolve relative paths.
    pub workspace_path: Option<PathBuf>,
    /// Local path used for project settings. `None` for remote connections.
    pub local_workspace: Option<PathBuf>,
    /// Home used to expand `${home}` in capabilities.
    pub home: Option<PathBuf>,
    pub initial_file: Option<String>,
}

impl Startup {
    pub fn local(file: Option<String>) -> Self {
        let workspace = std::env::current_dir().ok();
        let workspace_uri = workspace
            .as_ref()
            .map(|path| format!("file://{}", path.display()))
            .unwrap_or_else(|| "file:///".to_string());

        Self {
            connection: ConnectionSpec::Local,
            connection_name: "local".to_string(),
            workspace_uri,
            workspace_path: workspace.clone(),
            local_workspace: workspace,
            home: dirs::home_dir(),
            initial_file: file,
        }
    }

    pub fn remote(name: String, profile: &ConnectionProfile, file: Option<String>) -> Self {
        Self {
            connection: profile.spec(),
            connection_name: name,
            workspace_uri: profile.workspace_uri(),
            workspace_path: Some(PathBuf::from(&profile.workspace)),
            local_workspace: None,
            home: profile.home.as_ref().map(PathBuf::from),
            initial_file: file,
        }
    }
}

/// Command-line arguments handled before the window opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliOptions {
    pub connect: Option<String>,
    pub file: Option<String>,
}

impl CliOptions {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--connect" | "-c" => {
                    let name = args.next().ok_or("--connect requires a profile name")?;
                    options.connect = Some(name);
                }
                _ if argument.starts_with("--connect=") => {
                    let name = argument["--connect=".len()..].to_string();
                    if name.is_empty() {
                        return Err("--connect requires a profile name".to_string());
                    }
                    options.connect = Some(name);
                }
                _ if argument.starts_with('-') => {
                    return Err(format!("unknown option: {argument}"));
                }
                _ => {
                    if options.file.is_some() {
                        return Err("only one file can be opened at startup".to_string());
                    }
                    options.file = Some(argument);
                }
            }
        }

        Ok(options)
    }
}

/// Resolves the startup values for the command line.
pub fn startup_for(options: &CliOptions, paths: &Paths) -> Result<Startup, String> {
    let Some(name) = options.connect.as_deref() else {
        return Ok(Startup::local(options.file.clone()));
    };

    let (connections, warning) = Connections::load(&paths.connections_json());
    if let Some(warning) = warning {
        return Err(warning);
    }
    let profile = connections
        .profile(name)
        .ok_or_else(|| format!("unknown connection profile: {name}"))?;

    Ok(Startup::remote(
        name.to_string(),
        profile,
        options.file.clone(),
    ))
}

/// Resolves the location of a local `vy-server`.
///
/// Uses `VY_SERVER` when set. Otherwise it looks next to `vy-app` and finally falls
/// back to `PATH`.
fn local_server_command() -> Command {
    if let Some(path) = std::env::var_os("VY_SERVER") {
        return Command::new(path);
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let sibling = directory.join(format!("vy-server{}", std::env::consts::EXE_SUFFIX));
        if sibling.is_file() {
            return Command::new(sibling);
        }
    }

    Command::new("vy-server")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_uses_the_server_binary() {
        let command = ConnectionSpec::Local.command();
        let program = command.get_program().to_string_lossy().into_owned();
        assert!(program.contains("vy-server"));
    }

    #[test]
    fn ssh_disables_the_pty_and_passes_stdio() {
        let command = ConnectionSpec::Ssh {
            host: "example.com".into(),
            user: Some("nazo".into()),
            port: Some(2222),
            server_path: "/usr/local/bin/vy-server".into(),
        }
        .command();

        assert_eq!(command.get_program(), "ssh");
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-T",
                "-p",
                "2222",
                "--",
                "nazo@example.com",
                "/usr/local/bin/vy-server",
                "--stdio",
            ]
        );
    }

    #[test]
    fn ssh_without_a_user_uses_the_host() {
        let command = ConnectionSpec::Ssh {
            host: "example.com".into(),
            user: None,
            port: None,
            server_path: "vy-server".into(),
        }
        .command();

        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec!["-T", "--", "example.com", "vy-server", "--stdio"]
        );
    }

    #[test]
    fn parses_the_connect_option_and_file() {
        let options = CliOptions::parse([
            "--connect".to_string(),
            "dev".to_string(),
            "src/main.rs".to_string(),
        ])
        .expect("parse");
        assert_eq!(options.connect.as_deref(), Some("dev"));
        assert_eq!(options.file.as_deref(), Some("src/main.rs"));

        let options = CliOptions::parse(["--connect=dev".to_string()]).expect("parse equals form");
        assert_eq!(options.connect.as_deref(), Some("dev"));
        assert!(options.file.is_none());
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(CliOptions::parse(["--nope".to_string()]).is_err());
        assert!(CliOptions::parse(["--connect".to_string()]).is_err());
    }

    #[test]
    fn reads_profiles_and_builds_the_startup_values() {
        let directory = std::env::temp_dir().join(format!("vy-connections-{}", std::process::id()));
        let path = directory.join("connections.json");
        std::fs::create_dir_all(&directory).expect("mkdir");
        std::fs::write(
            &path,
            r#"{
                "profiles": {
                    "dev": {
                        "host": "example.com",
                        "user": "nazo",
                        "port": 2222,
                        "workspace": "/home/nazo/project",
                        "home": "/home/nazo",
                        "serverPath": "/usr/local/bin/vy-server"
                    }
                }
            }"#,
        )
        .expect("write");

        let (connections, warning) = Connections::load(&path);
        assert!(warning.is_none());
        let profile = connections.profile("dev").expect("profile");
        assert_eq!(profile.workspace_uri(), "file:///home/nazo/project");
        assert!(matches!(
            profile.spec(),
            ConnectionSpec::Ssh {
                port: Some(2222),
                ..
            }
        ));

        let startup = Startup::remote("dev".to_string(), profile, None);
        assert!(startup.connection.is_remote());
        assert_eq!(startup.workspace_uri, "file:///home/nazo/project");
        assert_eq!(
            startup.workspace_path.as_deref(),
            Some(std::path::Path::new("/home/nazo/project"))
        );
        assert!(startup.local_workspace.is_none());

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_profile_is_an_error() {
        let directory =
            std::env::temp_dir().join(format!("vy-connections-missing-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("mkdir");
        let paths = Paths::under(&directory);
        let options = CliOptions {
            connect: Some("missing".to_string()),
            file: None,
        };

        assert!(startup_for(&options, &paths).is_err());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
