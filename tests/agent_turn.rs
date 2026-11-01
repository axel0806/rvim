// SPDX-License-Identifier: Apache-2.0
//! Prompt turns, the agent log and the leader keys (PLAN.md, "Agent
//! experience", sections 3 and 4; milestone 7 step 5b-2). The agent is the
//! in-process fake of tests/agent_connect.rs: the editor's events carry the
//! agent's replies, and the requests rvim sends are read back as commands.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rvim::agent_approval::Approvals;
use rvim::agent_conn::{AgentEvent, ConnCmd};
use rvim::agent_detect::{How, ScanSummary};
use rvim::event::Event;
use rvim::{Editor, view};
use serde_json::{Value, json};

#[cfg(windows)]
const ROOT: &str = "C:\\proj";
#[cfg(not(windows))]
const ROOT: &str = "/proj";

fn editor() -> Editor {
    let mut ed = Editor::new();
    ed.set_screen_size(200, 24);
    ed.set_agent_project(PathBuf::from(ROOT));
    let mut approvals = Approvals::default();
    approvals.approve("claude", ROOT);
    ed.set_agent_approvals(approvals, None);
    ed.handle_event(Event::AgentScan(ScanSummary {
        found: vec![("claude", How::OnPath)],
        config_only: vec![],
        not_found: vec![],
        warnings: vec![],
    }));
    ed
}

/// The bottom row of the frame: the message and the indicator.
fn status(ed: &Editor) -> String {
    view::frame(ed).status
}

fn sent(cmds: &[ConnCmd]) -> Vec<Value> {
    cmds.iter()
        .filter_map(|c| match c {
            ConnCmd::Send { bytes, .. } => serde_json::from_slice(bytes).ok(),
            _ => None,
        })
        .collect()
}

fn stops(cmds: &[ConnCmd]) -> usize {
    cmds.iter()
        .filter(|c| matches!(c, ConnCmd::Stop { .. }))
        .count()
}

/// One wire line from the agent, delivered as the reader thread would.
fn agent(ed: &mut Editor, generation: u64, v: Value) {
    let mut bytes = serde_json::to_vec(&v).expect("json");
    bytes.push(b'\n');
    ed.handle_event(Event::AgentConn {
        generation,
        event: AgentEvent::Bytes(bytes),
    });
}

/// Answers `initialize` and `session/new` as an agent does. `taken` are the
/// commands the test already took; returns the spawn and stop commands.
fn handshake(ed: &mut Editor, generation: u64, taken: Vec<ConnCmd>) -> Vec<ConnCmd> {
    let mut others = Vec::new();
    let mut batch = taken;
    loop {
        batch.extend(ed.take_agent_conn_commands());
        let mut requests = Vec::new();
        for cmd in std::mem::take(&mut batch) {
            if matches!(cmd, ConnCmd::Send { .. }) {
                requests.extend(sent(std::slice::from_ref(&cmd)));
            } else {
                others.push(cmd);
            }
        }
        if requests.is_empty() {
            return others;
        }
        for req in requests {
            let id = req["id"].clone();
            let reply = match req["method"].as_str().unwrap_or_default() {
                "initialize" => json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}}),
                "session/new" => json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "s1"}}),
                _ => continue,
            };
            agent(ed, generation, reply);
        }
    }
}

/// An editor connected to the fake agent as generation `generation`.
fn connected(generation: u64) -> Editor {
    let mut ed = editor();
    ed.agent_connect(None);
    let cmds = ed.take_agent_conn_commands();
    handshake(&mut ed, generation, cmds);
    assert!(status(&ed).contains("ai: claude"), "{}", status(&ed));
    ed
}

/// Sends `:Agent {text}` and returns the id of the `session/prompt` request.
fn start_turn(ed: &mut Editor, text: &str) -> Value {
    ed.feed(&format!(":Agent {text}<CR>"));
    let cmds = ed.take_agent_conn_commands();
    sent(&cmds)
        .into_iter()
        .find(|v| v["method"] == "session/prompt")
        .map(|v| v["id"].clone())
        .expect("a session/prompt was sent")
}

fn update(ed: &mut Editor, generation: u64, update: Value) {
    agent(
        ed,
        generation,
        json!({"jsonrpc": "2.0", "method": "session/update",
               "params": {"sessionId": "s1", "update": update}}),
    );
}

fn chunk(text: &str) -> Value {
    json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})
}

/// The prompt's stop reason, as the agent's reply to `session/prompt`.
fn finish(ed: &mut Editor, generation: u64, id: &Value, reason: &str) {
    agent(
        ed,
        generation,
        json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": reason}}),
    );
}

/// The current document's text, one line per line, without a final newline.
fn text(ed: &Editor) -> String {
    ed.buf().lines().join("\n")
}

fn log(ed: &Editor) -> String {
    ed.agent_log_text().expect("the log exists")
}

#[test]
fn a_turn_streams_into_the_log_in_order() {
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "hello");
    assert!(
        status(&ed).contains("ai: claude working"),
        "{}",
        status(&ed)
    );
    update(&mut ed, 1, chunk("Hel"));
    update(&mut ed, 1, chunk("lo\nwo"));
    update(
        &mut ed,
        1,
        json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "hmm"}}),
    );
    update(
        &mut ed,
        1,
        json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "src/a.rs",
               "kind": "edit", "status": "completed"}),
    );
    update(
        &mut ed,
        1,
        json!({"sessionUpdate": "plan", "entries": [
            {"content": "a", "priority": "high", "status": "completed"},
            {"content": "b", "priority": "low", "status": "pending"}]}),
    );
    finish(&mut ed, 1, &id, "end_turn");
    assert_eq!(
        log(&ed),
        "> hello\nHello\nwo\n[thought] hmm\n[tool] edit src/a.rs: completed\n[plan] 1/2 done\n[stop] end_turn"
    );
    assert!(!status(&ed).contains("working"), "{}", status(&ed));
}

#[test]
fn a_second_prompt_during_a_turn_says_so() {
    let mut ed = connected(1);
    let _first = start_turn(&mut ed, "one");
    ed.feed(":Agent two<CR>");
    assert!(
        status(&ed).contains("a turn is already running"),
        "{}",
        status(&ed)
    );
    let cmds = ed.take_agent_conn_commands();
    assert!(
        sent(&cmds).iter().all(|v| v["method"] != "session/prompt"),
        "{cmds:?}"
    );
}

#[test]
fn the_prompt_is_the_rest_of_the_line_as_typed() {
    let mut ed = connected(1);
    ed.feed(":Agent a | b<CR>");
    let cmds = ed.take_agent_conn_commands();
    let prompt = sent(&cmds)
        .into_iter()
        .find(|v| v["method"] == "session/prompt")
        .expect("a prompt");
    assert_eq!(prompt["params"]["prompt"][0]["text"], "a | b");
}

#[test]
fn agent_with_no_text_opens_the_prompt_line() {
    let mut ed = connected(1);
    ed.feed(":Agent<CR>");
    assert_eq!(ed.cmdline(), "Agent ");
}

#[test]
fn cancel_mid_turn_sends_cancel_and_logs_the_stop_reason() {
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "long");
    ed.feed(":AgentCancel<CR>");
    let cmds = ed.take_agent_conn_commands();
    assert!(
        sent(&cmds).iter().any(|v| v["method"] == "session/cancel"),
        "{cmds:?}"
    );
    assert!(status(&ed).contains("cancelling"), "{}", status(&ed));
    finish(&mut ed, 1, &id, "cancelled");
    assert_eq!(
        log(&ed),
        "> long\n[cancel] session/cancel sent\n[stop] cancelled"
    );
}

#[test]
fn auth_required_on_a_prompt_is_the_auth_state() {
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "hi");
    agent(
        &mut ed,
        1,
        json!({"jsonrpc": "2.0", "id": id, "error": {
            "code": -32000, "message": "Authentication required"}}),
    );
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(stops(&cmds), 1, "rvim stops the process it started");
    assert!(status(&ed).contains("ai: claude error"), "{}", status(&ed));
    assert!(status(&ed).contains("not logged in"), "{}", status(&ed));
    assert!(log(&ed).contains("[error] not logged in"), "{}", log(&ed));
}

#[test]
fn a_permission_request_waits_and_cancel_answers_it_as_cancelled() {
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "edit");
    agent(
        &mut ed,
        1,
        json!({"jsonrpc": "2.0", "id": 50, "method": "session/request_permission",
               "params": {"sessionId": "s1",
                          "toolCall": {"toolCallId": "t1", "title": "edit a.rs"},
                          "options": [
                              {"optionId": "ok", "name": "Allow", "kind": "allow_once"},
                              {"optionId": "no", "name": "Reject", "kind": "reject_once"}]}}),
    );
    assert!(
        status(&ed).contains("waiting for a permission decision"),
        "{}",
        status(&ed)
    );
    assert!(status(&ed).contains("PERMISSION 1"), "{}", status(&ed));
    ed.feed(":AgentCancel<CR>");
    let cmds = ed.take_agent_conn_commands();
    let answer = sent(&cmds)
        .into_iter()
        .find(|v| v["id"] == 50)
        .expect("the permission request is answered");
    assert_eq!(answer["result"]["outcome"]["outcome"], "cancelled");
    finish(&mut ed, 1, &id, "cancelled");
    assert!(
        log(&ed).contains("[permission] edit a.rs: waiting"),
        "{}",
        log(&ed)
    );
}

#[test]
fn the_agent_log_is_not_modifiable_and_not_written_by_w() {
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "hi");
    finish(&mut ed, 1, &id, "end_turn");
    ed.feed(":AgentLog<CR>");
    let before = text(&ed);
    assert!(before.contains("> hi"), "{before:?}");
    ed.feed("dd");
    assert!(status(&ed).contains("E21"), "{}", status(&ed));
    ed.feed("ix<Esc>");
    assert_eq!(text(&ed), before);
    ed.feed("ox<Esc>");
    assert_eq!(text(&ed), before);
    ed.feed(":w<CR>");
    assert!(status(&ed).contains("E382"), "{}", status(&ed));
    assert!(!ed.buf().modified, "the log is not modified");
    ed.feed(":AgentLog!<CR>");
    assert_ne!(text(&ed), before, "back to the document it replaced");
    assert_eq!(log(&ed), before);
}

#[test]
fn leader_keys_work_once_opted_in() {
    let mut ed = connected(1);
    ed.set_agent_enabled(true);
    // `\a` waits for the next key (`l` and `c` are its own keys); alone, it
    // opens the prompt when the timeout runs out.
    ed.feed("\\a");
    assert_eq!(ed.cmdline(), "");
    assert!(ed.next_deadline().is_some(), "the timeout is a deadline");
    ed.agent_timer(Instant::now() + Duration::from_secs(2));
    assert_eq!(ed.cmdline(), "Agent ");
    ed.feed("hi<CR>");
    let cmds = ed.take_agent_conn_commands();
    let prompt = sent(&cmds)
        .into_iter()
        .find(|v| v["method"] == "session/prompt")
        .expect("a prompt");
    assert_eq!(prompt["params"]["prompt"][0]["text"], "hi");
    ed.feed("\\al");
    assert_eq!(text(&ed), log(&ed));
    ed.feed("\\al");
    assert_ne!(text(&ed), log(&ed));
    // The prompt above is running, so `\ac` cancels it.
    ed.feed("\\ac");
    assert!(status(&ed).contains("cancel sent"), "{}", status(&ed));
    let cmds = ed.take_agent_conn_commands();
    assert!(sent(&cmds).iter().any(|v| v["method"] == "session/cancel"));
}

#[test]
fn leader_keys_are_unbound_with_the_layer_off() {
    // `\` does nothing in Vim, so `\a` is `a`: the text after it is typed.
    let mut ed = connected(1);
    ed.feed("\\aZ<Esc>");
    assert_eq!(text(&ed), "Z");
    assert_eq!(ed.cmdline(), "");
}
