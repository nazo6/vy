//! `vy-server` を子プロセスとして起動し、標準入出力で RPC をやり取りします。
//!
//! ADR 0002 により、ローカル接続でもコアは別プロセスです。読み取りは専用の
//! スレッドで行い、UI のスレッドを止めません。

use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use rmpv::Value;
use vy_proto::{FrameDecoder, RpcMessage, decode_message, encode_frame, encode_message};

/// 受信したメッセージがあるか、まだか、接続が切れたかです。
pub enum Poll {
    Message(RpcMessage),
    Empty,
    Closed,
}

pub struct Client {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<RpcMessage>,
    next_id: u64,
    session_id: u64,
}

impl Client {
    pub fn spawn() -> std::io::Result<Self> {
        let mut child = server_command()
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // サーバーのログはクライアントの標準エラーへ流す。
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().expect("標準入力を取れません");
        let stdout = child.stdout.take().expect("標準出力を取れません");

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("vy-rpc-reader".into())
            .spawn(move || read_loop(stdout, sender))?;

        Ok(Self {
            child,
            stdin,
            receiver,
            next_id: 1,
            session_id: 0,
        })
    }

    /// 要求を送り、対応する id を返します。
    pub fn request(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let message = RpcMessage::request(id, self.session_id, method, params);
        if let Err(error) = self.write(&message) {
            eprintln!("vy: 要求を送れません: {error}");
        }
        id
    }

    pub fn set_session_id(&mut self, session_id: u64) {
        self.session_id = session_id;
    }

    pub fn poll(&self) -> Poll {
        match self.receiver.try_recv() {
            Ok(message) => Poll::Message(message),
            Err(TryRecvError::Empty) => Poll::Empty,
            Err(TryRecvError::Disconnected) => Poll::Closed,
        }
    }

    fn write(&mut self, message: &RpcMessage) -> std::io::Result<()> {
        let payload = encode_message(message).map_err(invalid_data)?;
        let frame = encode_frame(&payload).map_err(invalid_data)?;
        self.stdin.write_all(&frame)?;
        self.stdin.flush()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // クライアントが閉じたらサーバーも終わらせる。孤児を残さない。
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_loop(stdout: std::process::ChildStdout, sender: Sender<RpcMessage>) {
    let mut reader = BufReader::new(stdout);
    let mut decoder = FrameDecoder::new();
    let mut chunk = [0u8; 8 * 1024];

    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => return,
            Ok(read) => read,
            Err(error) => {
                eprintln!("vy: 標準出力を読めません: {error}");
                return;
            }
        };

        decoder.push(&chunk[..read]);

        loop {
            match decoder.next_frame() {
                Ok(Some(payload)) => match decode_message(&payload) {
                    Ok(message) => {
                        if sender.send(message).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        eprintln!("vy: メッセージを復号できません: {error}");
                        return;
                    }
                },
                Ok(None) => break,
                Err(error) => {
                    eprintln!("vy: フレームを読めません: {error}");
                    return;
                }
            }
        }
    }
}

/// `vy-server` の場所を決めます。
///
/// `VY_SERVER` があればそれを使います。なければ `vy-app` と同じディレクトリを探し、
/// 最後に `PATH` から起動します。
fn server_command() -> Command {
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

fn invalid_data(error: vy_proto::ProtoError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
}
