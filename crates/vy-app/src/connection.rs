//! How the client reaches `vy-server`.
//!
//! Local connections start a child process. Remote connections use the system
//! `ssh` command and the same stdio framing. See `docs/adr/0010-remote-connection.md`.

use std::process::Command;

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
                command.arg(destination);
                command.arg("--");
                command.arg(server_path);
                command.arg("--stdio");
                command
            }
        }
    }
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
                "nazo@example.com",
                "--",
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
        assert_eq!(args, vec!["-T", "example.com", "--", "vy-server", "--stdio"]);
    }
}
