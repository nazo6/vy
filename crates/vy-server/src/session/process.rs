//! Process management and execution for `vy-server`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vy_proto::{
    ErrorCode, ProcessExecParams, ProcessKillParams, ProcessKillResult, ProcessOutput,
    ProcessSpawnParams, ProcessSpawnResult, ProcessStdinParams, ProcessStdinResult, RpcMessage,
    StreamPayload, decode_value, to_value,
};

use super::{Outcome, Session};

/// State for a running child process.
pub(super) struct RunningProcess {
    pub stdin: Option<ChildStdin>,
    pub child: Arc<Mutex<Option<Child>>>,
}

pub(super) type ProcessTable = HashMap<u32, RunningProcess>;

impl Session {
    pub(super) fn process_exec(&self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<ProcessExecParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid process/exec params",
            ));
        };
        let mut cmd = Command::new(&params.command);
        cmd.args(&params.args);
        if let Some(cwd) = &params.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = &params.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        match cmd.output() {
            Ok(output) => {
                let res = ProcessOutput {
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                };
                Outcome::one(RpcMessage::ok(id, to_value(&res)))
            }
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("failed to execute {}: {err}", params.command),
            )),
        }
    }

    pub(super) fn process_spawn(&mut self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<ProcessSpawnParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid process/spawn params",
            ));
        };
        let mut cmd = Command::new(&params.command);
        cmd.args(&params.args);
        if let Some(cwd) = &params.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = &params.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::Internal,
                    format!("failed to spawn {}: {err}", params.command),
                ));
            }
        };

        let pid = child.id();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child_arc = Arc::new(Mutex::new(Some(child)));

        if let Some(sender) = self.outgoing.clone() {
            let seq = Arc::new(AtomicU64::new(0));

            // Stdout reader thread
            if let Some(stdout) = stdout {
                let sender = sender.clone();
                let seq = Arc::clone(&seq);
                std::thread::Builder::new()
                    .name(format!("vy-proc-{pid}-stdout"))
                    .spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            let Ok(line) = line else { break };
                            let chunk_seq = seq.fetch_add(1, Ordering::SeqCst);
                            let _ = sender.send(RpcMessage::Stream {
                                stream_id: pid as u64,
                                seq: chunk_seq,
                                payload: StreamPayload::Chunk(to_value(&serde_json::json!({
                                    "channel": "stdout",
                                    "data": format!("{line}\n")
                                }))),
                            });
                        }
                    })
                    .ok();
            }

            // Stderr reader thread
            if let Some(stderr) = stderr {
                let sender = sender.clone();
                let seq = Arc::clone(&seq);
                std::thread::Builder::new()
                    .name(format!("vy-proc-{pid}-stderr"))
                    .spawn(move || {
                        let reader = BufReader::new(stderr);
                        for line in reader.lines() {
                            let Ok(line) = line else { break };
                            let chunk_seq = seq.fetch_add(1, Ordering::SeqCst);
                            let _ = sender.send(RpcMessage::Stream {
                                stream_id: pid as u64,
                                seq: chunk_seq,
                                payload: StreamPayload::Chunk(to_value(&serde_json::json!({
                                    "channel": "stderr",
                                    "data": format!("{line}\n")
                                }))),
                            });
                        }
                    })
                    .ok();
            }

            // Exit waiter thread
            let waiter_child = Arc::clone(&child_arc);
            std::thread::Builder::new()
                .name(format!("vy-proc-{pid}-wait"))
                .spawn(move || {
                    let mut wait_child = None;
                    if let Ok(mut lock) = waiter_child.lock() {
                        wait_child = lock.take();
                    }
                    if let Some(mut child) = wait_child {
                        let exit_code = child.wait().ok().and_then(|st| st.code()).unwrap_or(-1);
                        let end_seq = seq.fetch_add(1, Ordering::SeqCst);
                        let _ = sender.send(RpcMessage::Stream {
                            stream_id: pid as u64,
                            seq: end_seq,
                            payload: StreamPayload::End {
                                summary: Some(to_value(&serde_json::json!({
                                    "exit_code": exit_code
                                }))),
                            },
                        });
                    }
                })
                .ok();
        }

        self.processes.insert(
            pid,
            RunningProcess {
                stdin,
                child: child_arc,
            },
        );

        Outcome::one(RpcMessage::ok(id, to_value(&ProcessSpawnResult { pid })))
    }

    pub(super) fn process_kill(&mut self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<ProcessKillParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid process/kill params",
            ));
        };
        let Some(proc) = self.processes.remove(&params.pid) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("process {} not found", params.pid),
            ));
        };
        if let Ok(mut lock) = proc.child.lock()
            && let Some(mut child) = lock.take()
        {
            let _ = child.kill();
        }
        Outcome::one(RpcMessage::ok(
            id,
            to_value(&ProcessKillResult { success: true }),
        ))
    }

    pub(super) fn process_stdin(&mut self, id: u64, params: rmpv::Value) -> Outcome {
        let Some(params) = decode_value::<ProcessStdinParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "invalid process/stdin params",
            ));
        };
        let Some(proc) = self.processes.get_mut(&params.pid) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("process {} not found", params.pid),
            ));
        };
        let Some(stdin) = proc.stdin.as_mut() else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidState,
                format!("stdin closed for process {}", params.pid),
            ));
        };
        match stdin
            .write_all(params.data.as_bytes())
            .and_then(|()| stdin.flush())
        {
            Ok(()) => Outcome::one(RpcMessage::ok(
                id,
                to_value(&ProcessStdinResult { success: true }),
            )),
            Err(err) => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("failed to write to stdin: {err}"),
            )),
        }
    }
}
