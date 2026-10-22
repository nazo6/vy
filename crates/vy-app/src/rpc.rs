//! Starts `vy-server` as a child process and exchanges RPC over stdio.
//!
//! Per ADR 0002, the core is a separate process even for local connections. Both
//! reads and writes happen on dedicated threads so the UI thread never blocks.

use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use rmpv::Value;
use vy_proto::{FrameDecoder, RpcMessage, decode_message, encode_frame, encode_message};

pub struct Client {
    child: Child,
    writer: Option<Sender<RpcMessage>>,
    receiver: async_channel::Receiver<RpcMessage>,
    next_id: u64,
    session_id: u64,
}

impl Client {
    pub fn spawn() -> std::io::Result<Self> {
        let mut child = server_command()
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Forward the server's logs to the client's stderr.
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().expect("could not take stdin");
        let stdout = child.stdout.take().expect("could not take stdout");

        // Writes go through a queue so a full pipe never blocks the UI thread. The
        // queue is unbounded: dropping messages would desynchronize the document.
        let (writer, queue) = channel();
        std::thread::Builder::new()
            .name("vy-rpc-writer".into())
            .spawn(move || write_loop(stdin, queue))?;

        let (sender, receiver) = async_channel::unbounded();
        std::thread::Builder::new()
            .name("vy-rpc-reader".into())
            .spawn(move || read_loop(stdout, sender))?;

        Ok(Self {
            child,
            writer: Some(writer),
            receiver,
            next_id: 1,
            session_id: 0,
        })
    }

    /// Sends a request and returns its id.
    pub fn request(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let message = RpcMessage::request(id, self.session_id, method, params);
        if let Err(error) = self.write(message) {
            eprintln!("vy: could not send the request: {error}");
        }
        id
    }

    /// Sends a notification that expects no response.
    pub fn notify(&mut self, method: &str, params: Value) {
        let message = RpcMessage::notification(method, params);
        if let Err(error) = self.write(message) {
            eprintln!("vy: could not send the notification: {error}");
        }
    }

    pub fn set_session_id(&mut self, session_id: u64) {
        self.session_id = session_id;
    }

    /// Returns a handle for receiving messages without blocking the UI thread.
    ///
    /// The channel closes when the reader thread stops, which is how the client
    /// detects that the server closed the connection.
    pub fn receiver(&self) -> async_channel::Receiver<RpcMessage> {
        self.receiver.clone()
    }

    fn write(&mut self, message: RpcMessage) -> std::io::Result<()> {
        let Some(writer) = self.writer.as_ref() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the writer thread stopped",
            ));
        };
        writer.send(message).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the writer thread stopped")
        })
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Closing the queue lets the writer flush what is left. The kill stops a
        // server that did not exit on its own.
        self.writer.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Encodes and writes messages in order. Dropping `stdin` closes the server's input.
fn write_loop(mut stdin: ChildStdin, queue: Receiver<RpcMessage>) {
    for message in queue {
        let payload = match encode_message(&message) {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!("vy: could not encode a message: {error}");
                continue;
            }
        };
        let frame = match encode_frame(&payload) {
            Ok(frame) => frame,
            Err(error) => {
                eprintln!("vy: could not frame a message: {error}");
                continue;
            }
        };
        if let Err(error) = stdin.write_all(&frame).and_then(|()| stdin.flush()) {
            eprintln!("vy: could not write to vy-server: {error}");
            return;
        }
    }
}

fn read_loop(stdout: std::process::ChildStdout, sender: async_channel::Sender<RpcMessage>) {
    let mut reader = BufReader::new(stdout);
    let mut decoder = FrameDecoder::new();
    let mut chunk = [0u8; 8 * 1024];

    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => return,
            Ok(read) => read,
            Err(error) => {
                eprintln!("vy: could not read stdout: {error}");
                return;
            }
        };

        decoder.push(&chunk[..read]);

        loop {
            match decoder.next_frame() {
                Ok(Some(payload)) => match decode_message(&payload) {
                    Ok(message) => {
                        if sender.send_blocking(message).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        eprintln!("vy: could not decode the message: {error}");
                        return;
                    }
                },
                Ok(None) => break,
                Err(error) => {
                    eprintln!("vy: could not read a frame: {error}");
                    return;
                }
            }
        }
    }
}

/// Resolves the location of `vy-server`.
///
/// Uses `VY_SERVER` when set. Otherwise it looks next to `vy-app` and finally falls
/// back to `PATH`.
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
