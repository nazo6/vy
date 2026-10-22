//! ビルドした `vy-server` を子プロセスで起動し、標準入出力でハンドシェイクします。

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use rmpv::Value;
use vy_proto::{
    FrameDecoder, PROTOCOL_VERSION, RpcMessage, RpcResult, decode_message, encode_frame,
    encode_message, get, object,
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
            .expect("vy-server を起動できません");

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
            assert!(read > 0, "サーバーが応答の前に終了しました");
            self.decoder.push(&chunk[..read]);
        }
    }
}

fn initialize(version: u64) -> RpcMessage {
    RpcMessage::request(
        1,
        0,
        "initialize",
        object([
            ("protocol_version", Value::from(version)),
            ("client_version", Value::from("0.1.0")),
            ("client_capabilities", Value::Array(Vec::new())),
            ("workspace", Value::from("file:///tmp")),
            ("resume_session", Value::Nil),
        ]),
    )
}

fn ok_value(message: &RpcMessage) -> &Value {
    match message {
        RpcMessage::Response {
            result: RpcResult::Ok { value },
            ..
        } => value,
        other => panic!("Ok ではありません: {other:?}"),
    }
}

#[test]
fn completes_the_handshake_over_stdio() {
    let mut server = Server::start();
    server.send(&initialize(PROTOCOL_VERSION as u64));

    let response = server.receive();
    assert_eq!(
        get(ok_value(&response), "session_id").and_then(Value::as_u64),
        Some(1)
    );

    match server.receive() {
        RpcMessage::Notification { method, params } => {
            assert_eq!(method, "initialized");
            assert_eq!(
                get(&params, "protocol_version").and_then(Value::as_u64),
                Some(PROTOCOL_VERSION as u64)
            );
            assert_eq!(
                get(&params, "server_version").and_then(Value::as_str),
                Some(env!("CARGO_PKG_VERSION"))
            );
        }
        other => panic!("通知ではありません: {other:?}"),
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
        other => panic!("エラーの応答ではありません: {other:?}"),
    }

    drop(server.stdin);
    let status = server.child.wait().expect("wait");
    assert!(status.success());
}
