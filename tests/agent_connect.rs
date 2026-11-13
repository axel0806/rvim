// SPDX-License-Identifier: Apache-2.0
//! Milestone 7 step 5b-1 (PLAN.md, "Agent experience", sections 1, 2, 5 and
//! 10): `:AgentConnect`, the consent prompt, the approval store, `:AgentCancel`,
//! `:AgentDisconnect`, the lifecycle with backoff, and the status indicator.
//!
//! The fake agent is in-process. The editor's worker requests (`ConnCmd`) are
//! read from `take_agent_conn_commands`, and the agent's replies are sent back
//! as `Event::AgentConn` with the same generation, as the reader thread does.
//! The replies follow the shapes in `src/acp.rs` (ACP v1), which the recorded
//! transcripts in `tests/fixtures/acp/` also use. No process is spawned and no
//! time passes: backoff is driven by `agent_timer` with the retry instant the
//! editor reports.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rvim::agent_approval::{self, Approvals};
use rvim::agent_conn::{AgentEvent, ConnCmd};
use rvim::agent_detect::{How, ScanSummary};
use rvim::event::Event;
use rvim::key::Key;
use rvim::{Editor, view};
use serde_json::{Value, json};

#[cfg(windows)]
const ROOT: &str = "C:\\proj";
#[cfg(not(windows))]
const ROOT: &str = "/proj";

/// An editor with `claude` found by a scan, a project root, and (if asked)
/// the approval for `claude` in that project already stored.
fn editor(approved: bool) -> Editor {
    let mut ed = Editor::new();
    ed.set_screen_size(200, 24);
    ed.set_agent_project(PathBuf::from(ROOT));
    let mut approvals = Approvals::default();
    if approved {
        approvals.approve("claude", ROOT);
    }
    ed.set_agent_approvals(approvals, None);
    ed.handle_event(Event::AgentScan(ScanSummary {
        found: vec![("claude", How::OnPath)],
        config_only: vec![],
        not_found: vec![],
        warnings: vec![],
    }));
    ed
}

/// The bottom row of the frame, as text.
fn status(ed: &Editor) -> String {
    view::frame(ed).status
}

/// One wire line, newline included, as the reader thread delivers it.
fn line(v: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(v).expect("json");
    bytes.push(b'\n');
    bytes
}

fn starts(cmds: &[ConnCmd]) -> Vec<u64> {
    cmds.iter()
        .filter_map(|c| match c {
            ConnCmd::Start { generation, .. } => Some(*generation),
            _ => None,
        })
        .collect()
}

fn stops(cmds: &[ConnCmd]) -> usize {
    cmds.iter()
        .filter(|c| matches!(c, ConnCmd::Stop { .. }))
        .count()
}

/// The requests rvim sent, parsed.
fn sent(cmds: &[ConnCmd]) -> Vec<Value> {
    cmds.iter()
        .filter_map(|c| match c {
            ConnCmd::Send { bytes, .. } => serde_json::from_slice(bytes).ok(),
            _ => None,
        })
        .collect()
}

/// Plays the agent's side of the handshake for generation `generation`: answers each
/// request rvim sends, until none is left. `auth` makes `session/new` fail
/// with auth required. Returns the methods seen and the other commands.
fn serve(ed: &mut Editor, generation: u64, auth: bool) -> (Vec<String>, Vec<ConnCmd>) {
    serve_taken(ed, generation, auth, Vec::new())
}

/// As `serve`, but first handles `taken`: commands the test already took from
/// the editor and did not answer.
fn serve_taken(
    ed: &mut Editor,
    generation: u64,
    auth: bool,
    taken: Vec<ConnCmd>,
) -> (Vec<String>, Vec<ConnCmd>) {
    let mut methods = Vec::new();
    let mut others = Vec::new();
    let mut taken = Some(taken);
    loop {
        let mut cmds = taken.take().unwrap_or_default();
        cmds.extend(ed.take_agent_conn_commands());
        let mut requests = Vec::new();
        for cmd in cmds {
            if matches!(cmd, ConnCmd::Send { .. }) {
                requests.extend(sent(std::slice::from_ref(&cmd)));
            } else {
                others.push(cmd);
            }
        }
        if requests.is_empty() {
            return (methods, others);
        }
        for req in requests {
            let id = req["id"].clone();
            let method = req["method"].as_str().unwrap_or_default().to_owned();
            let reply = match method.as_str() {
                "initialize" => json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}}),
                "session/new" if auth => json!({"jsonrpc": "2.0", "id": id, "error": {
                    "code": -32000, "message": "Authentication required"}}),
                "session/new" => json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "s1"}}),
                _ => continue,
            };
            methods.push(method);
            ed.handle_event(Event::AgentConn {
                generation,
                event: AgentEvent::Bytes(line(&reply)),
            });
        }
    }
}

fn connected(generation: u64) -> Editor {
    let mut ed = editor(true);
    ed.agent_connect(None);
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![generation]);
    serve_taken(&mut ed, generation, false, cmds);
    assert!(status(&ed).contains("ai: claude"));
    ed
}

// ---- the handshake --------------------------------------------------------

#[test]
fn successful_initialize_and_session_new() {
    let mut ed = editor(true);
    ed.agent_connect(None);
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![1]);
    let command = cmds.iter().find_map(|c| match c {
        ConnCmd::Start { command, cwd, .. } => Some((command.clone(), cwd.clone())),
        _ => None,
    });
    let (command, cwd) = command.expect("a Start");
    assert!(command.starts_with("npx "), "registry command: {command}");
    assert_eq!(cwd, PathBuf::from(ROOT));
    // The first request is sent after the Start, in the same queue.
    let first = sent(&cmds);
    assert_eq!(first[0]["method"], "initialize");
    assert_eq!(first[0]["params"]["protocolVersion"], 1);
    assert_eq!(
        first[0]["params"]["clientCapabilities"]["fs"]["readTextFile"],
        true
    );
    assert_eq!(
        first[0]["params"]["clientCapabilities"]["fs"]["writeTextFile"],
        true
    );
    assert_eq!(first[0]["params"]["clientCapabilities"]["terminal"], false);

    let (methods, _) = serve_taken(&mut ed, 1, false, cmds);
    assert_eq!(methods, vec!["initialize", "session/new"]);
    assert!(status(&ed).contains("ai: claude"));
    assert!(
        status(&ed).contains("claude-acp: connected"),
        "{}",
        status(&ed)
    );
    assert_eq!(ed.agent_retry_at(), None);
}

#[test]
fn auth_failure_is_an_error_and_stops_the_process() {
    let mut ed = editor(true);
    ed.agent_connect(None);
    let (_, others) = serve(&mut ed, 1, true);
    assert_eq!(stops(&others), 1, "rvim stops the process it started");
    assert!(status(&ed).contains("ai: claude error"), "{}", status(&ed));
    assert!(status(&ed).contains("not logged in"), "{}", status(&ed));
    assert_eq!(ed.agent_retry_at(), None, "auth failure is not retried");
}

#[test]
fn eof_mid_start_is_an_error_with_the_exit_code() {
    let mut ed = editor(true);
    ed.agent_connect(None);
    ed.take_agent_conn_commands();
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(Some(1)),
    });
    assert!(status(&ed).contains("ai: claude error"), "{}", status(&ed));
    assert!(
        status(&ed).contains("claude-acp: exited 1"),
        "{}",
        status(&ed)
    );
}

#[test]
fn spawn_failure_is_reported_on_the_message_line() {
    let mut ed = editor(true);
    ed.agent_connect(None);
    ed.take_agent_conn_commands();
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::SpawnFailed("npx: not found".to_owned()),
    });
    assert!(
        status(&ed).contains("could not start: npx: not found"),
        "{}",
        status(&ed)
    );
    assert!(status(&ed).contains("ai: claude error"));
}

#[test]
fn malformed_lines_disconnect_after_ten_in_a_row() {
    let mut ed = connected(1);
    for _ in 0..9 {
        ed.handle_event(Event::AgentConn {
            generation: 1,
            event: AgentEvent::Bytes(b"not json\n".to_vec()),
        });
    }
    assert!(
        status(&ed).contains("ai: claude"),
        "still connected after 9: {}",
        status(&ed)
    );
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Bytes(b"not json\n".to_vec()),
    });
    assert!(
        status(&ed).contains("ai: claude retry 1/5"),
        "{}",
        status(&ed)
    );
    assert!(ed.agent_retry_at().is_some());
}

#[test]
fn no_reply_to_initialize_times_out_on_the_fake_clock() {
    let mut ed = editor(true);
    ed.agent_connect(None);
    ed.take_agent_conn_commands();
    let started = Instant::now();
    ed.agent_timer(started + Duration::from_secs(9));
    assert!(
        status(&ed).contains("ai: claude connecting"),
        "{}",
        status(&ed)
    );
    ed.agent_timer(started + Duration::from_secs(11));
    assert!(status(&ed).contains("ai: claude error"), "{}", status(&ed));
    assert!(
        status(&ed).contains("no reply to initialize after 10s"),
        "{}",
        status(&ed)
    );
}

// ---- consent and approvals ------------------------------------------------

#[test]
fn nothing_is_spawned_without_approval() {
    let mut ed = editor(false);
    ed.agent_connect(None);
    let cmds = ed.take_agent_conn_commands();
    assert!(cmds.is_empty(), "no Start, no Send: {cmds:?}");
    assert!(status(&ed).contains("[y]es [n]o"), "{}", status(&ed));
    // The prompt takes every key, and other keys do not start anything.
    ed.handle_key(Key::Char('x'));
    ed.handle_key(Key::Char('j'));
    assert!(ed.take_agent_conn_commands().is_empty());
    // Time passing does not start anything either.
    ed.agent_timer(Instant::now() + Duration::from_secs(60));
    assert!(ed.take_agent_conn_commands().is_empty());
    assert!(ed.next_deadline().is_none());
}

#[test]
fn consent_n_and_escape_do_not_connect() {
    for key in [Key::Char('n'), Key::Esc] {
        let mut ed = editor(false);
        ed.agent_connect(None);
        ed.handle_key(key);
        let cmds = ed.take_agent_conn_commands();
        assert!(cmds.is_empty(), "{cmds:?}");
        assert!(status(&ed).contains("not connected"), "{}", status(&ed));
        // The prompt is gone: a later key is an ordinary key, nothing starts.
        ed.handle_key(Key::Char('y'));
        assert!(ed.take_agent_conn_commands().is_empty());
    }
}

#[test]
fn consent_y_saves_the_approval_and_connects() {
    let mut ed = editor(false);
    ed.agent_connect(Some("claude"));
    ed.handle_key(Key::Char('y'));
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![1]);
    let saved = cmds.iter().find_map(|c| match c {
        ConnCmd::SaveApprovals(text) => Some(text.clone()),
        _ => None,
    });
    let saved = saved.expect("the approval is written by the worker");
    let back = agent_approval::parse(&saved).expect("valid");
    assert!(back.is_approved("claude", ROOT));
    // A second connect in the same project does not ask again or save again.
    ed.agent_disconnect();
    ed.take_agent_conn_commands();
    ed.agent_connect(None);
    let again = ed.take_agent_conn_commands();
    assert_eq!(starts(&again), vec![2]);
    assert!(
        again
            .iter()
            .all(|c| !matches!(c, ConnCmd::SaveApprovals(_)))
    );
}

#[test]
fn connect_without_an_scan_or_registry_command_says_so() {
    let mut ed = Editor::new();
    ed.set_agent_project(PathBuf::from(ROOT));
    ed.agent_connect(None);
    assert!(
        status(&ed).contains("run :AgentScan first"),
        "{}",
        status(&ed)
    );
    ed.agent_connect(Some("nosuchagent"));
    assert!(status(&ed).contains("unknown agent"), "{}", status(&ed));
    assert!(ed.take_agent_conn_commands().is_empty());
}

// ---- approval store ------------------------------------------------------

#[test]
fn approval_store_round_trip_and_unreadable_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(agent_approval::FILE_NAME);

    let (none, warning) = agent_approval::load(&path);
    assert!(warning.is_none(), "a missing file is no warning");
    assert!(!none.is_approved("claude", ROOT));

    let mut stored = none;
    stored.approve("claude", ROOT);
    stored.approve("codex", "C:\\x \"quoted\" dir");
    agent_approval::write(&path, &stored.to_text()).expect("write");
    let (back, warning) = agent_approval::load(&path);
    assert!(warning.is_none());
    assert_eq!(back, stored);

    // Unreadable (invalid TOML): no approvals, a warning, and the file is kept.
    let broken = "[[approval]\nagent = ";
    std::fs::write(&path, broken).expect("write");
    let (loaded, warning) = agent_approval::load(&path);
    let warning = warning.expect("a warning");
    assert!(warning.contains("unreadable"), "{warning}");
    assert!(loaded.is_broken());
    assert!(!loaded.is_approved("claude", ROOT));

    // A wrong type is just as unreadable.
    std::fs::write(&path, "[[approval]]\nagent = 3\nproject = \"/p\"\n").expect("write");
    assert!(agent_approval::load(&path).1.is_some());

    // The editor asks for consent again, connects for this session, and does
    // not overwrite the damaged file.
    std::fs::write(&path, broken).expect("write");
    let (loaded, warning) = agent_approval::load(&path);
    let mut ed = editor(false);
    ed.set_agent_approvals(loaded, warning);
    ed.agent_connect(Some("claude"));
    ed.handle_key(Key::Char('y'));
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![1]);
    assert!(cmds.iter().all(|c| !matches!(c, ConnCmd::SaveApprovals(_))));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), broken);
}

#[test]
fn a_missing_config_directory_leaves_approvals_unsaved() {
    let mut ed = editor(false);
    ed.set_agent_approvals(
        Approvals::broken(),
        Some("no config directory: agent approvals cannot be saved".to_owned()),
    );
    ed.agent_connect(Some("claude"));
    ed.handle_key(Key::Char('y'));
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![1]);
    assert!(cmds.iter().all(|c| !matches!(c, ConnCmd::SaveApprovals(_))));
}

// ---- disconnect, cancel, indicator ---------------------------------------

#[test]
fn disconnect_stops_the_process_and_keeps_the_approval() {
    let mut ed = connected(1);
    ed.take_agent_conn_commands();
    ed.agent_disconnect();
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(stops(&cmds), 1);
    assert!(status(&ed).contains("ai: claude found"), "{}", status(&ed));
    // Still approved: the next connect starts without asking.
    ed.agent_connect(None);
    assert_eq!(starts(&ed.take_agent_conn_commands()), vec![2]);
}

#[test]
fn cancel_without_a_turn_says_so_and_sends_nothing() {
    let mut ed = connected(1);
    ed.take_agent_conn_commands();
    ed.agent_cancel();
    let cmds = ed.take_agent_conn_commands();
    assert!(sent(&cmds).is_empty(), "{cmds:?}");
    assert!(status(&ed).contains("no turn to cancel"), "{}", status(&ed));
}

#[test]
fn cancel_and_disconnect_with_nothing_connected() {
    let mut ed = editor(true);
    ed.agent_cancel();
    assert!(
        status(&ed).contains("no agent connected"),
        "{}",
        status(&ed)
    );
    ed.agent_disconnect();
    assert!(
        status(&ed).contains("no agent connected"),
        "{}",
        status(&ed)
    );
    assert!(ed.take_agent_conn_commands().is_empty());
}

#[test]
fn indicator_states_for_connecting_connected_and_failed() {
    let mut ed = editor(true);
    ed.agent_connect(None);
    assert!(
        status(&ed).contains("ai: claude connecting"),
        "{}",
        status(&ed)
    );
    serve(&mut ed, 1, false);
    assert!(status(&ed).contains("ai: claude "), "{}", status(&ed));
    assert!(!status(&ed).contains("connecting"));
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(None),
    });
    assert!(
        status(&ed).contains("ai: claude retry 1/5"),
        "{}",
        status(&ed)
    );
    let mut failed = editor(true);
    failed.agent_connect(None);
    serve(&mut failed, 1, true);
    assert!(
        status(&failed).contains("ai: claude error"),
        "{}",
        status(&failed)
    );
}

#[test]
fn no_indicator_without_the_layer_and_no_prediction() {
    let ed = Editor::new();
    assert!(!status(&ed).contains("ai:"));
    assert!(!ed.prediction_allowed());
    assert!(!editor(true).prediction_allowed());
}

// ---- backoff with the fake clock ------------------------------------------

#[test]
fn lost_connection_retries_with_backoff_then_gives_up() {
    let mut ed = connected(1);
    ed.take_agent_conn_commands();
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(Some(0)),
    });
    let mut generation = 1;
    // Delays 1, 2, 4, 8 and 16 seconds: five reconnects, each failing.
    for (attempt, delay) in [1u64, 2, 4, 8, 16].into_iter().enumerate() {
        let due = ed.agent_retry_at().expect("a retry is scheduled");
        assert!(
            status(&ed).contains(&format!("retry {}/5", attempt + 1)),
            "{}",
            status(&ed)
        );
        // Not before the due time; at it, a new generation starts.
        ed.agent_timer(due - Duration::from_millis(1));
        assert!(starts(&ed.take_agent_conn_commands()).is_empty());
        assert_eq!(delay, [1u64, 2, 4, 8, 16][attempt]);
        ed.agent_timer(due);
        let cmds = ed.take_agent_conn_commands();
        assert_eq!(starts(&cmds), vec![generation + 1]);
        generation += 1;
        // The reconnect dies before connecting (the process exits).
        ed.handle_event(Event::AgentConn {
            generation,
            event: AgentEvent::Ended(None),
        });
        // Events of the old generation are ignored.
        ed.handle_event(Event::AgentConn {
            generation: generation - 1,
            event: AgentEvent::Ended(None),
        });
    }
    assert_eq!(ed.agent_retry_at(), None);
    assert!(status(&ed).contains("ai: claude error"), "{}", status(&ed));
    assert!(
        status(&ed).contains("gave up after 5 retries"),
        "{}",
        status(&ed)
    );
}

#[test]
fn a_successful_reconnect_resets_the_retry_count() {
    let mut ed = connected(1);
    ed.take_agent_conn_commands();
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(None),
    });
    let due = ed.agent_retry_at().expect("retry");
    ed.agent_timer(due);
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![2]);
    serve_taken(&mut ed, 2, false, cmds);
    assert!(status(&ed).contains("ai: claude "), "{}", status(&ed));
    assert!(!status(&ed).contains("retry"));
    ed.handle_event(Event::AgentConn {
        generation: 2,
        event: AgentEvent::Ended(None),
    });
    assert!(status(&ed).contains("retry 1/5"), "{}", status(&ed));
}

#[test]
fn a_stale_generation_is_ignored() {
    let mut ed = connected(1);
    ed.take_agent_conn_commands();
    ed.agent_disconnect();
    ed.take_agent_conn_commands();
    ed.agent_connect(None);
    ed.take_agent_conn_commands();
    // Gen 1 is gone; its late end must not touch the new connection.
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(None),
    });
    assert!(status(&ed).contains("connecting"), "{}", status(&ed));
    assert_eq!(ed.agent_retry_at(), None);
}

// ---- 5b-1b: revoked approvals, paste, and the prompts' precedence ---------

#[test]
fn a_retry_after_the_approval_is_revoked_does_not_start() {
    let mut ed = connected(1);
    ed.take_agent_conn_commands();
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(Some(0)),
    });
    let due = ed.agent_retry_at().expect("a retry is scheduled");
    // The approval is removed mid-session.
    ed.set_agent_approvals(Approvals::default(), None);
    ed.agent_timer(due);
    let cmds = ed.take_agent_conn_commands();
    assert!(starts(&cmds).is_empty(), "{cmds:?}");
    assert_eq!(ed.agent_retry_at(), None);
    assert!(
        status(&ed).contains("approval removed; not reconnecting"),
        "{}",
        status(&ed)
    );
    // Later ticks start nothing either.
    ed.agent_timer(due + Duration::from_secs(60));
    assert!(starts(&ed.take_agent_conn_commands()).is_empty());
}

#[test]
fn a_pasted_y_does_not_answer_the_consent_prompt() {
    let mut ed = editor(false);
    ed.agent_connect(None);
    ed.handle_event(Event::Paste("y".into()));
    let cmds = ed.take_agent_conn_commands();
    assert!(starts(&cmds).is_empty(), "{cmds:?}");
    assert!(
        cmds.iter().all(|c| !matches!(c, ConnCmd::SaveApprovals(_))),
        "{cmds:?}"
    );
    // The prompt is still up: `n` answers it.
    ed.handle_key(Key::Char('n'));
    assert!(status(&ed).contains("not connected"), "{}", status(&ed));
    assert!(ed.take_agent_conn_commands().is_empty());
}

/// An editor on a modified buffer whose file changed on disk, with the agent
/// opt-in on: the conflict prompt is up.
fn conflicted(dir: &std::path::Path) -> Editor {
    let path = dir.join("notes.txt");
    std::fs::write(&path, "one\ntwo\n").expect("write");
    let mut ed = Editor::open(&path);
    ed.set_screen_size(200, 24);
    ed.set_agent_enabled(true);
    ed.set_agent_project(PathBuf::from(ROOT));
    ed.set_agent_approvals(Approvals::default(), None);
    ed.handle_event(Event::AgentScan(ScanSummary {
        found: vec![("claude", How::OnPath)],
        config_only: vec![],
        not_found: vec![],
        warnings: vec![],
    }));
    ed.feed("x");
    std::fs::write(&path, "changed\n").expect("write");
    ed.feed(":checktime<CR>");
    assert!(
        status(&ed).contains("changed on disk and in rvim"),
        "{}",
        status(&ed)
    );
    ed
}

#[test]
fn the_conflict_prompt_takes_keys_before_consent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut ed = conflicted(dir.path());
    // Consent is asked while the conflict prompt is up.
    ed.agent_connect(Some("claude"));
    assert!(status(&ed).contains("Start claude"), "{}", status(&ed));
    // `y` answers the conflict prompt (a beep), not the consent prompt.
    ed.handle_key(Key::Char('y'));
    let cmds = ed.take_agent_conn_commands();
    assert!(starts(&cmds).is_empty(), "{cmds:?}");
    assert!(
        cmds.iter().all(|c| !matches!(c, ConnCmd::SaveApprovals(_))),
        "{cmds:?}"
    );
    // `k` keeps the buffer and closes the conflict prompt; the consent prompt
    // is still waiting, so `y` now approves and connects.
    ed.handle_key(Key::Char('k'));
    ed.handle_key(Key::Char('y'));
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(starts(&cmds), vec![1]);
    assert!(
        cmds.iter().any(|c| matches!(c, ConnCmd::SaveApprovals(_))),
        "{cmds:?}"
    );
}
