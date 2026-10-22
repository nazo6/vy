use rmpv::Value;
use vy_core::Uri;
use vy_proto::value::{get, object};
use vy_proto::{ErrorCode, PROTOCOL_VERSION, RpcMessage};

use super::{Outcome, Session, State};

impl Session {
    pub(super) fn initialize(&mut self, id: u64, params: Value) -> Outcome {
        let protocol_version = get(&params, "protocol_version").and_then(Value::as_u64);

        if protocol_version != Some(PROTOCOL_VERSION as u64) {
            let message = match protocol_version {
                Some(version) => format!(
                    "protocol version mismatch: server {PROTOCOL_VERSION}, client {version}"
                ),
                None => "missing protocol_version".to_string(),
            };
            return Outcome::closing(RpcMessage::err(
                id,
                ErrorCode::UnsupportedProtocolVersion,
                message,
            ));
        }

        let workspace = match get(&params, "workspace").and_then(Value::as_str) {
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

        let session_id = self.next_session_id;
        self.next_session_id += 1;
        self.state = State::Ready { session_id };

        let response = RpcMessage::ok(id, object([("session_id", Value::from(session_id))]));
        let initialized = RpcMessage::notification(
            "initialized",
            object([
                ("protocol_version", Value::from(PROTOCOL_VERSION)),
                ("session_id", Value::from(session_id)),
                ("server_version", Value::from(env!("CARGO_PKG_VERSION"))),
                // Streams, Blob, and multiplexing are not implemented yet, so this is empty.
                ("server_capabilities", Value::Array(Vec::new())),
                (
                    "platform",
                    object([
                        ("os", Value::from(std::env::consts::OS)),
                        (
                            "path_separator",
                            Value::from(std::path::MAIN_SEPARATOR.to_string()),
                        ),
                    ]),
                ),
            ]),
        );

        Outcome {
            messages: vec![response, initialized],
            close: false,
        }
    }
}
