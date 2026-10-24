use vy_core::Uri;
use vy_proto::{
    ErrorCode, InitializeParams, InitializeResult, Initialized, InitializedNotification,
    PROTOCOL_VERSION, PlatformInfo, RpcMessage,
};

use super::{Outcome, Session, State, notification, ok_message};

impl Session {
    pub(super) fn initialize(&mut self, id: u64, params: InitializeParams) -> Outcome {
        if params.protocol_version != PROTOCOL_VERSION {
            return Outcome::closing(RpcMessage::err(
                id,
                ErrorCode::UnsupportedProtocolVersion,
                format!(
                    "protocol version mismatch: server {PROTOCOL_VERSION}, client {}",
                    params.protocol_version
                ),
            ));
        }

        let workspace = match params.workspace.as_deref() {
            Some(value) => match Uri::parse(value) {
                Ok(uri) => Some(uri),
                Err(error) => {
                    return Outcome::one(RpcMessage::err(
                        id,
                        ErrorCode::InvalidRequest,
                        error.to_string(),
                    ));
                }
            },
            None => None,
        };
        self.workspace = workspace;
        self.state_path = self
            .data_dir
            .as_ref()
            .zip(self.workspace.as_ref())
            .map(|(data_dir, workspace)| super::state::workspace_state_path(data_dir, workspace));
        self.restore_session();

        let session_id = self.next_session_id;
        self.next_session_id += 1;
        self.state = State::Ready { session_id };

        let response = ok_message::<vy_proto::Initialize>(id, &InitializeResult { session_id });
        let initialized = notification::<Initialized>(&InitializedNotification {
            protocol_version: PROTOCOL_VERSION,
            session_id,
            server_version: env!("CARGO_PKG_VERSION").into(),
            server_capabilities: vec!["stream".into(), "session-restore".into()],
            platform: PlatformInfo {
                os: std::env::consts::OS.into(),
                path_separator: std::path::MAIN_SEPARATOR.to_string(),
            },
        });

        Outcome {
            messages: vec![response, initialized],
            close: false,
        }
    }
}
