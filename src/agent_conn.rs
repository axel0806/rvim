// SPDX-License-Identifier: Apache-2.0
//! The agent's process, on worker threads (PLAN.md, "Agent experience",
//! sections 2 and 10; milestone 7 step 5b-1).
//!
//! A manager thread takes [`ConnCmd`]s from the editor. `Start` spawns the
//! agent's ACP command as a local child with piped stdin and stdout; a writer
//! thread feeds its stdin and a reader thread reads its stdout and, at its end,
//! reaps it. Events go back as `Event::AgentConn`, tagged with the connection
//! generation so that a late event from an old connection is ignored. Nothing
//! here runs unless the editor sent `Start`, and the editor sends that only
//! after the approval check (`agent_session.rs`).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::agent_approval;
use crate::event::Event;

/// Requests from the editor to the manager thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnCmd {
    /// Spawn `command` (split on spaces) in `cwd`. Replaces any connection.
    Start {
        generation: u64,
        command: String,
        cwd: PathBuf,
    },
    /// One line for the agent's stdin.
    Send { generation: u64, bytes: Vec<u8> },
    /// Kill the process if this connection is the current one.
    Stop { generation: u64 },
    /// Write the approvals file (the text from `Approvals::to_text`).
    SaveApprovals(String),
}

/// What the worker reports back. `generation` 0 is for notes that belong to no
/// connection (a failed save).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// Bytes read from the agent's stdout, in order.
    Bytes(Vec<u8>),
    /// stdout closed and the process was reaped; its exit code if it had one.
    Ended(Option<i32>),
    /// The process could not be started.
    SpawnFailed(String),
    /// A message for the message line.
    Note(String),
}

/// The live connection: its generation, its stdin queue and the child, which
/// the reader thread also waits on.
struct Live {
    generation: u64,
    stdin: mpsc::Sender<Vec<u8>>,
    child: Arc<Mutex<Child>>,
}

impl Live {
    fn stop(self) {
        // Dropping `stdin` ends the writer thread and closes the pipe.
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

/// Starts the manager thread. It ends when `rx` is dropped.
pub fn spawn(rx: mpsc::Receiver<ConnCmd>, tx: mpsc::Sender<Event>, approvals: PathBuf) {
    thread::spawn(move || {
        let mut current: Option<Live> = None;
        while let Ok(cmd) = rx.recv() {
            match cmd {
                ConnCmd::Start {
                    generation,
                    command,
                    cwd,
                } => {
                    if let Some(old) = current.take() {
                        old.stop();
                    }
                    match start_child(&command, &cwd) {
                        Ok((child, stdout, stdin)) => {
                            let child = Arc::new(Mutex::new(child));
                            let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>();
                            thread::spawn(move || write_loop(stdin, stdin_rx));
                            let (child_r, tx_r) = (Arc::clone(&child), tx.clone());
                            thread::spawn(move || read_loop(stdout, generation, child_r, tx_r));
                            current = Some(Live {
                                generation,
                                stdin: stdin_tx,
                                child,
                            });
                        }
                        Err(reason) => send(&tx, generation, AgentEvent::SpawnFailed(reason)),
                    }
                }
                ConnCmd::Send { generation, bytes } => {
                    if let Some(live) = current.as_ref().filter(|l| l.generation == generation) {
                        // A closed pipe means the reader will report the end.
                        let _ = live.stdin.send(bytes);
                    }
                }
                ConnCmd::Stop { generation } => {
                    if let Some(live) = current.take_if(|l| l.generation == generation) {
                        live.stop();
                    }
                }
                ConnCmd::SaveApprovals(text) => {
                    if let Err(e) = agent_approval::write(&approvals, &text) {
                        let note = format!("could not save {}: {e}", agent_approval::FILE_NAME);
                        send(&tx, 0, AgentEvent::Note(note));
                    }
                }
            }
        }
        if let Some(live) = current.take() {
            live.stop();
        }
    });
}

fn send(tx: &mpsc::Sender<Event>, generation: u64, event: AgentEvent) {
    // A send fails only when the editor has ended; the process then dies with
    // its pipes.
    let _ = tx.send(Event::AgentConn { generation, event });
}

/// Spawns the command. On Windows a launcher such as `npx` is a `.cmd` file,
/// which `Command` finds only with its extension, so that is tried next.
fn start_child(command: &str, cwd: &Path) -> Result<(Child, ChildStdout, ChildStdin), String> {
    let mut words = command.split_whitespace();
    let program = words
        .next()
        .ok_or_else(|| "no ACP command in the registry".to_owned())?;
    let args: Vec<&str> = words.collect();
    let attempt = |prog: &str| {
        Command::new(prog)
            .args(&args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    };
    let spawned = match attempt(program) {
        Ok(child) => Ok(child),
        Err(e) if cfg!(windows) && e.kind() == std::io::ErrorKind::NotFound => {
            attempt(&format!("{program}.cmd"))
        }
        Err(e) => Err(e),
    };
    let mut child = spawned.map_err(|e| format!("{program}: {e}"))?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    match (stdin, stdout) {
        (Some(stdin), Some(stdout)) => Ok((child, stdout, stdin)),
        _ => {
            let _ = child.kill();
            Err(format!("{program}: no stdio pipes"))
        }
    }
}

fn write_loop(mut stdin: ChildStdin, rx: mpsc::Receiver<Vec<u8>>) {
    for bytes in rx {
        if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
            break;
        }
    }
}

fn read_loop(
    mut stdout: ChildStdout,
    generation: u64,
    child: Arc<Mutex<Child>>,
    tx: mpsc::Sender<Event>,
) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match stdout.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let event = AgentEvent::Bytes(buf[..n].to_vec());
                if tx.send(Event::AgentConn { generation, event }).is_err() {
                    return;
                }
            }
        }
    }
    let code = child
        .lock()
        .ok()
        .and_then(|mut c| c.wait().ok())
        .and_then(|status| status.code());
    send(&tx, generation, AgentEvent::Ended(code));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_missing_program_is_reported_not_panicked() {
        let (tx, rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        spawn(
            cmd_rx,
            tx,
            std::env::temp_dir().join("rvim-no-such-approvals.toml"),
        );
        let cwd = std::env::temp_dir();
        cmd_tx
            .send(ConnCmd::Start {
                generation: 7,
                command: "rvim-no-such-agent-binary --acp".to_owned(),
                cwd,
            })
            .expect("manager running");
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Event::AgentConn {
                generation: 7,
                event: AgentEvent::SpawnFailed(reason),
            }) => assert!(reason.contains("rvim-no-such-agent-binary")),
            other => panic!("expected SpawnFailed, got {other:?}"),
        }
    }
}
