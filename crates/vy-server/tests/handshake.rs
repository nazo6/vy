//! Starts the built `vy-server` as a child process and performs the handshake over stdio.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use rmpv::Value;
use vy_proto::{
    FrameDecoder, InitializeParams, InitializeResult, InitializedNotification, PROTOCOL_VERSION,
    RpcMessage, RpcResult, decode_message, decode_value, encode_frame, encode_message, to_value,
};

struct Server {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    decoder: FrameDecoder,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vy-server"))
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start vy-server");

        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");

        Self {
            child,
            stdin,
            stdout,
            decoder: FrameDecoder::new(),
        }
    }

    fn send(&mut self, message: &RpcMessage) {
        let payload = encode_message(message).expect("encode");
        let frame = encode_frame(&payload).expect("frame");
        self.stdin.write_all(&frame).expect("write");
        self.stdin.flush().expect("flush");
    }

    fn receive(&mut self) -> RpcMessage {
        let mut chunk = [0u8; 4096];

        loop {
            if let Some(payload) = self.decoder.next_frame().expect("frame") {
                return decode_message(&payload).expect("decode");
            }

            let read = self.stdout.read(&mut chunk).expect("read");
            assert!(read > 0, "the server exited before responding");
            self.decoder.push(&chunk[..read]);
        }
    }
}

fn initialize(version: u64) -> RpcMessage {
    RpcMessage::request(
        1,
        0,
        "initialize",
        to_value(&InitializeParams {
            protocol_version: version as u32,
            client_version: "0.1.0".into(),
            client_capabilities: Vec::new(),
            workspace: Some("file:///tmp".into()),
            resume_session: None,
        }),
    )
}

fn ok_value(message: &RpcMessage) -> &Value {
    match message {
        RpcMessage::Response {
            result: RpcResult::Ok { value },
            ..
        } => value,
        other => panic!("expected Ok: {other:?}"),
    }
}

#[test]
fn completes_the_handshake_over_stdio() {
    let mut server = Server::start();
    server.send(&initialize(PROTOCOL_VERSION as u64));

    let response = server.receive();
    let init_res: InitializeResult = decode_value(ok_value(&response)).expect("InitializeResult");
    assert_eq!(init_res.session_id, 1);

    match server.receive() {
        RpcMessage::Notification { method, params } => {
            assert_eq!(method, "initialized");
            let notif: InitializedNotification =
                decode_value(&params).expect("InitializedNotification");
            assert_eq!(notif.protocol_version, PROTOCOL_VERSION);
            assert_eq!(notif.server_version, env!("CARGO_PKG_VERSION"));
            assert_eq!(notif.platform.os, std::env::consts::OS);
            assert_eq!(
                notif.platform.path_separator,
                std::path::MAIN_SEPARATOR.to_string()
            );
        }
        other => panic!("expected a notification: {other:?}"),
    }

    server.send(&RpcMessage::request(
        2,
        1,
        "shutdown",
        Value::Map(Vec::new()),
    ));
    let response = server.receive();
    assert!(matches!(
        response,
        RpcMessage::Response {
            result: RpcResult::Ok { .. },
            ..
        }
    ));

    drop(server.stdin);
    let status = server.child.wait().expect("wait");
    assert!(status.success());
}

#[test]
fn closes_when_the_protocol_version_does_not_match() {
    let mut server = Server::start();
    server.send(&initialize((PROTOCOL_VERSION + 1) as u64));

    match server.receive() {
        RpcMessage::Response {
            result: RpcResult::Err { code, .. },
            ..
        } => assert_eq!(code, vy_proto::ErrorCode::UnsupportedProtocolVersion),
        other => panic!("expected an error response: {other:?}"),
    }

    drop(server.stdin);
    let status = server.child.wait().expect("wait");
    assert!(status.success());
}
