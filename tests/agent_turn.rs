// SPDX-License-Identifier: Apache-2.0
//! Prompt turns, the agent log and the leader keys (PLAN.md, "Agent
//! experience", sections 3 and 4; milestone 7 step 5b-2). The agent is the
//! in-process fake of tests/agent_connect.rs: the editor's events carry the
//! agent's replies, and the requests rvim sends are read back as commands.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rvim::acp::ConnState;
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
    editor_with("")
}

fn editor_with(text: &str) -> Editor {
    let mut ed = Editor::from_text(text);
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
    connected_with(generation, "")
}

/// As `connected`, with `text` in the document.
fn connected_with(generation: u64, text: &str) -> Editor {
    let mut ed = editor_with(text);
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
    assert_eq!(ed.agent_conn_state(), Some(ConnState::NeedsAuth));
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
        status(&ed).contains("claude-acp asks for permission: edit a.rs. [y]es [n]o (1/1)"),
        "{}",
        status(&ed)
    );
    assert!(status(&ed).contains("PERMISSION 1"), "{}", status(&ed));
    // The prompt takes typed keys, so the cancel comes from the API (as
    // the leader key would, were it typeable while the prompt is up).
    ed.agent_cancel();
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
fn the_log_refuses_every_change_and_still_takes_output() {
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "hello");
    update(&mut ed, 1, chunk("Hi\nthere"));
    finish(&mut ed, 1, &id, "end_turn");
    ed.feed(":AgentLog<CR>");
    let before = text(&ed);
    assert!(before.contains("there"), "{before:?}");
    let keys = [
        // Visual operators (Body::VisOp), Normal changes and dot.
        "vd<Esc>",
        "vx<Esc>",
        "vc<Esc>",
        "vs<Esc>",
        "vC<Esc>",
        "vX<Esc>",
        "vR<Esc>",
        "vJ<Esc>",
        "vu<Esc>",
        "vU<Esc>",
        "v~<Esc>",
        "dd",
        "x",
        "p",
        "J",
        "ox<Esc>",
        "Ox<Esc>",
        ".",
        // Ex commands that change text.
        ":s/>/X/<CR>",
        ":normal dd<CR>",
        ":g/>/d<CR>",
        ":m0<CR>",
        ":t0<CR>",
        ":pu<CR>",
        ":j<CR>",
        ":><CR>",
        ":retab<CR>",
        ":sort<CR>",
        ":d<CR>",
    ];
    for keys in keys {
        ed.feed(keys);
        assert_eq!(text(&ed), before, "{keys} changed the log");
        assert!(status(&ed).contains("E21"), "{keys}: {}", status(&ed));
    }
    // The editor's own output still goes into the locked log.
    let id = start_turn(&mut ed, "again");
    finish(&mut ed, 1, &id, "end_turn");
    assert!(text(&ed).contains("> again"), "{}", text(&ed));
    assert!(!ed.buf().modified, "the log is not modified");
}

#[test]
fn leader_keys_are_unbound_with_the_layer_off() {
    // `\` does nothing in Vim, so `\a` is `a`: the text after it is typed.
    let mut ed = connected(1);
    ed.feed("\\aZ<Esc>");
    assert_eq!(text(&ed), "Z");
    assert_eq!(ed.cmdline(), "");
}

// ---- permission requests (milestone 7 step 6a) -----------------------------

/// The absolute path of `rel` in the project, as an agent sends it.
fn project_path(rel: &str) -> String {
    PathBuf::from(ROOT).join(rel).to_string_lossy().into_owned()
}

/// A tool call of `kind` on the project file `rel`.
fn file_tool(kind: &str, rel: &str) -> Value {
    json!({"toolCallId": format!("t-{rel}"), "title": format!("{kind} {rel}"),
           "kind": kind, "locations": [{"path": project_path(rel)}]})
}

/// `session/request_permission` with request id `id`, offering the option
/// kinds `kinds`; each option's id is its kind.
fn ask(ed: &mut Editor, id: i64, tool: Value, kinds: &[&str]) {
    let options: Vec<Value> = kinds
        .iter()
        .map(|k| json!({"optionId": k, "name": k, "kind": k}))
        .collect();
    agent(
        ed,
        1,
        json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission",
               "params": {"sessionId": "s1", "toolCall": tool, "options": options}}),
    );
}

const ALL: [&str; 4] = ["allow_once", "allow_always", "reject_once", "reject_always"];

/// The answers to permission requests in `cmds`: (request id, outcome),
/// where the outcome is the selected option id or `cancelled`.
fn answers(cmds: &[ConnCmd]) -> Vec<(i64, String)> {
    sent(cmds)
        .into_iter()
        .filter(|v| v.get("result").is_some_and(|r| r.get("outcome").is_some()))
        .map(|v| {
            let outcome = &v["result"]["outcome"];
            let what = match outcome["outcome"].as_str() {
                Some("selected") => outcome["optionId"].as_str().unwrap_or("?").to_owned(),
                other => other.unwrap_or("?").to_owned(),
            };
            (v["id"].as_i64().unwrap_or(-1), what)
        })
        .collect()
}

#[test]
fn a_queue_of_two_is_answered_only_by_keys_in_order() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "edit");
    let diff = json!([{"type": "diff", "path": project_path("src/a.rs"),
                       "oldText": "a\nb\nc\n", "newText": "a\nB\nc\nd\ne\n"}]);
    let mut tool = file_tool("edit", "src/a.rs");
    tool["content"] = diff;
    ask(&mut ed, 50, tool, &ALL);
    ask(
        &mut ed,
        51,
        json!({"toolCallId": "t2", "title": "cargo test", "kind": "execute"}),
        &["allow_once", "reject_once"],
    );
    assert_eq!(
        status(&ed)
            .split("  ")
            .next()
            .unwrap_or_default()
            .trim_end(),
        "claude-acp wants to edit src/a.rs (+3 -1). [y]es [a]lways for edit in src/ this session [n]o [N]ever (1/2)",
    );
    assert!(status(&ed).contains("PERMISSION 2"), "{}", status(&ed));
    // No answer without a key, however long the prompt waits.
    ed.agent_timer(Instant::now() + Duration::from_secs(3600));
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    ed.feed("y");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_once".to_owned())]
    );
    assert!(
        status(&ed).contains("claude-acp wants to run: cargo test. [y]es [n]o (1/1)"),
        "{}",
        status(&ed)
    );
    ed.feed("n");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(51, "reject_once".to_owned())]
    );
    assert!(!ed.permission_prompt_up());
    assert!(!status(&ed).contains("PERMISSION"), "{}", status(&ed));
    let log = log(&ed);
    assert!(log.contains("[permission] edit src/a.rs: waiting"), "{log}");
    assert!(log.contains("[permission] edit src/a.rs: allowed"), "{log}");
    assert!(
        log.contains("[permission] execute cargo test: rejected"),
        "{log}"
    );
}

#[test]
fn each_key_selects_its_option_kind() {
    for (keys, want) in [
        ("y", "allow_once"),
        ("a", "allow_always"),
        ("n", "reject_once"),
        ("N", "reject_always"),
        ("<Esc>", "reject_once"),
        ("<C-c>", "reject_once"),
    ] {
        let mut ed = connected(1);
        let _turn = start_turn(&mut ed, "edit");
        ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
        ed.feed(keys);
        assert_eq!(
            answers(&ed.take_agent_conn_commands()),
            [(50, want.to_owned())],
            "{keys}"
        );
    }
}

#[test]
fn esc_without_a_reject_option_cancels_and_unoffered_keys_do_nothing() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "edit");
    ask(
        &mut ed,
        50,
        file_tool("edit", "src/a.rs"),
        &["allow_once", "reject_always"],
    );
    assert!(
        status(&ed).contains("[y]es [N]ever for edit in src/ this session (1/1)"),
        "{}",
        status(&ed)
    );
    // Not offered (`a`, `n`) or not a prompt key (`x`): the prompt stays.
    ed.feed("anxq:");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());
    assert_eq!(ed.cmdline(), "");
    ed.feed("<Esc>");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "cancelled".to_owned())]
    );
    assert!(
        log(&ed).contains("[permission] edit src/a.rs: cancelled"),
        "{}",
        log(&ed)
    );
}

#[test]
fn a_grant_answers_only_matching_requests_and_is_not_saved() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    ed.feed("a");
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(answers(&cmds), [(50, "allow_always".to_owned())]);
    assert!(cmds.iter().all(|c| !matches!(c, ConnCmd::SaveApprovals(_))));
    assert!(
        status(&ed).contains("ai: claude working (1 grant)"),
        "{}",
        status(&ed)
    );
    // Same kind and directory: answered from the grant, with no prompt.
    ask(&mut ed, 51, file_tool("edit", "src/b.rs"), &ALL);
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(51, "allow_always".to_owned())]
    );
    assert!(!ed.permission_prompt_up());
    assert!(
        log(&ed).contains("grant used: edit src/ allow"),
        "{}",
        log(&ed)
    );
    // Only allow_once offered: the grant picks it.
    ask(
        &mut ed,
        52,
        file_tool("edit", "src/c.rs"),
        &["allow_once", "reject_once"],
    );
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(52, "allow_once".to_owned())]
    );
    // No allow option at all: the grant cannot answer, so it prompts.
    ask(&mut ed, 53, file_tool("edit", "src/d.rs"), &["reject_once"]);
    assert!(ed.permission_prompt_up());
    ed.feed("n");
    // Another directory, a subdirectory, another kind: they prompt.
    for (id, tool) in [
        (54, file_tool("edit", "lib/a.rs")),
        (55, file_tool("edit", "src/sub/a.rs")),
        (56, file_tool("delete", "src/a.rs")),
    ] {
        ask(&mut ed, id, tool, &ALL);
        assert!(ed.permission_prompt_up(), "{id} prompts");
        assert!(
            answers(&ed.take_agent_conn_commands())
                .iter()
                .all(|(i, _)| *i != id)
        );
        ed.feed("y");
        assert_eq!(
            answers(&ed.take_agent_conn_commands()),
            [(id, "allow_once".to_owned())]
        );
    }
    // A grant is never written to agent-approvals.toml.
    let cmds = ed.take_agent_conn_commands();
    assert!(cmds.iter().all(|c| !matches!(c, ConnCmd::SaveApprovals(_))));
    ed.feed(":AgentGrants<CR>");
    assert!(
        status(&ed).contains("claude: allow edit src/"),
        "{}",
        status(&ed)
    );
}

#[test]
fn a_reject_grant_for_a_command_answers_the_same_command_only() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "run");
    let run = |cmd: &str| json!({"toolCallId": cmd, "title": cmd, "kind": "execute"});
    ask(&mut ed, 50, run("rm -rf target"), &ALL);
    assert!(
        status(&ed).contains("[a]lways for this command this session"),
        "{}",
        status(&ed)
    );
    ed.feed("N");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "reject_always".to_owned())]
    );
    ask(
        &mut ed,
        51,
        run("rm -rf target"),
        &["allow_once", "reject_once"],
    );
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(51, "reject_once".to_owned())]
    );
    assert!(
        log(&ed).contains("grant used: execute rm -rf target reject"),
        "{}",
        log(&ed)
    );
    ask(&mut ed, 52, run("cargo test"), &ALL);
    assert!(ed.permission_prompt_up());
}

#[test]
fn a_request_without_a_scope_makes_no_grant() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "x");
    ask(
        &mut ed,
        50,
        json!({"toolCallId": "t1", "title": "think hard", "kind": "think"}),
        &ALL,
    );
    ed.feed("a");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_always".to_owned())]
    );
    ed.feed(":AgentGrants<CR>");
    assert!(status(&ed).contains("No grants"), "{}", status(&ed));
    ask(
        &mut ed,
        51,
        json!({"toolCallId": "t1", "title": "think hard", "kind": "think"}),
        &ALL,
    );
    assert!(ed.permission_prompt_up());
}

#[test]
fn agent_forget_removes_the_grants_and_the_approval() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    ed.feed("a");
    ed.take_agent_conn_commands();
    ed.feed(":AgentForget<CR>");
    let cmds = ed.take_agent_conn_commands();
    assert_eq!(stops(&cmds), 1, "the connection is closed");
    let saved: Vec<&String> = cmds
        .iter()
        .filter_map(|c| match c {
            ConnCmd::SaveApprovals(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(saved.len(), 1, "{cmds:?}");
    assert!(!saved[0].contains("[[approval]]"), "{}", saved[0]);
    assert!(
        status(&ed).contains("forgot 1 grant and 1 approval for this project"),
        "{}",
        status(&ed)
    );
    ed.feed(":AgentGrants<CR>");
    assert!(status(&ed).contains("No grants"), "{}", status(&ed));
    // Connecting again asks for consent.
    ed.agent_connect(None);
    assert!(status(&ed).contains("[y]es [n]o"), "{}", status(&ed));
    assert!(
        ed.take_agent_conn_commands()
            .iter()
            .all(|c| !matches!(c, ConnCmd::Start { .. }))
    );
}

#[test]
fn agent_forget_bang_removes_every_project() {
    let mut ed = editor();
    let mut approvals = Approvals::default();
    approvals.approve("claude", ROOT);
    approvals.approve("claude", "/elsewhere");
    ed.set_agent_approvals(approvals, None);
    ed.feed(":AgentForget<CR>");
    let cmds = ed.take_agent_conn_commands();
    let text = cmds
        .iter()
        .find_map(|c| match c {
            ConnCmd::SaveApprovals(text) => Some(text.clone()),
            _ => None,
        })
        .expect("saved");
    assert!(
        text.contains("/elsewhere") && !text.contains(&format!("{:?}", ROOT)),
        "{text}"
    );
    ed.feed(":AgentForget!<CR>");
    let cmds = ed.take_agent_conn_commands();
    let text = cmds
        .iter()
        .find_map(|c| match c {
            ConnCmd::SaveApprovals(text) => Some(text.clone()),
            _ => None,
        })
        .expect("saved");
    assert!(!text.contains("[[approval]]"), "{text}");
    assert!(status(&ed).contains("every project"), "{}", status(&ed));
}

#[test]
fn the_prompt_goes_when_the_queue_empties() {
    // :AgentCancel answers the queue as cancelled.
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    ask(&mut ed, 51, file_tool("edit", "src/b.rs"), &ALL);
    // The prompt takes typed keys, so the cancel comes from the API (as
    // the leader key would, were it typeable while the prompt is up).
    ed.agent_cancel();
    assert!(!ed.permission_prompt_up());
    let got = answers(&ed.take_agent_conn_commands());
    assert_eq!(
        got,
        [(50, "cancelled".to_owned()), (51, "cancelled".to_owned())]
    );
    assert!(
        log(&ed).contains("[permission] edit src/a.rs: cancelled"),
        "{}",
        log(&ed)
    );
    assert!(
        log(&ed).contains("[permission] 1 more cancelled"),
        "{}",
        log(&ed)
    );
    finish(&mut ed, 1, &id, "cancelled");
    // A `y` now is Vim's `y` again: no answer.
    ed.feed("y<Esc>");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());

    // The turn ends with a request queued.
    let mut ed = connected(1);
    let id = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    finish(&mut ed, 1, &id, "end_turn");
    assert!(!ed.permission_prompt_up());

    // The connection is lost.
    let mut ed = connected(1);
    let _id = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(Some(1)),
    });
    assert!(!ed.permission_prompt_up());
    assert!(ed.next_deadline().is_some(), "the retry timer");

    // :AgentDisconnect.
    let mut ed = connected(1);
    let _id = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    ed.feed(":AgentDisconnect<CR>");
    assert!(!ed.permission_prompt_up());
    assert_eq!(ed.next_deadline(), None);
}

#[test]
fn the_prompt_reminds_every_five_minutes_and_never_answers() {
    let mut ed = connected(1);
    let _turn = start_turn(&mut ed, "edit");
    let t0 = Instant::now();
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    let deadline = ed.next_deadline().expect("the reminder is a deadline");
    assert!(deadline >= t0 + Duration::from_secs(299), "{deadline:?}");
    ed.agent_timer(t0 + Duration::from_secs(240));
    assert!(status(&ed).contains("wants to edit"), "{}", status(&ed));
    ed.agent_timer(deadline);
    assert!(
        status(&ed).contains("claude-acp still waiting for y or n"),
        "{}",
        status(&ed)
    );
    let next = ed.next_deadline().expect("repeats");
    assert_eq!(next, deadline + Duration::from_secs(300));
    ed.feed(":"); // taken by the prompt; clears nothing
    ed.agent_timer(next);
    assert!(status(&ed).contains("still waiting"), "{}", status(&ed));
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());
    ed.feed("y");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_once".to_owned())]
    );
    assert_eq!(ed.next_deadline(), None, "no reminder once answered");
}

#[test]
fn prompt_keys_do_not_reach_normal_or_insert_mode() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ask(
        &mut ed,
        50,
        file_tool("edit", "src/a.rs"),
        &["allow_once", "reject_once"],
    );
    ed.feed("xddiZp");
    assert_eq!(text(&ed), "abc");
    assert_eq!(ed.mode(), rvim::editor::Mode::Normal);
    ed.paste("y");
    assert!(ed.permission_prompt_up(), "a paste is not an answer");
    assert_eq!(text(&ed), "abc");
    ed.feed("y");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_once".to_owned())]
    );
    // Insert mode takes its own keys again once the prompt is answered.
    ed.feed("AZ<Esc>");
    assert_eq!(text(&ed), "abcZ");
}

// ---- the prompt takes keys in plain Normal mode only (2026-11-13) ----------

fn request_one(ed: &mut Editor, id: i64) {
    ask(
        ed,
        id,
        file_tool("edit", "src/a.rs"),
        &["allow_once", "reject_once"],
    );
}

#[test]
fn a_request_in_insert_mode_waits_for_normal_mode() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("A");
    request_one(&mut ed, 50);
    // Queued, shown by the indicator, and no key is taken.
    assert!(ed.permission_queued());
    assert!(!ed.permission_prompt_up());
    assert!(status(&ed).contains("PERMISSION 1"), "{}", status(&ed));
    ed.feed("yn");
    ed.paste("y");
    assert_eq!(text(&ed), "abcyny");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    // A second request in Insert mode only raises the count.
    request_one(&mut ed, 51);
    assert!(status(&ed).contains("PERMISSION 2"), "{}", status(&ed));
    ed.feed("<Esc>");
    // The `<Esc>` left Insert mode; it did not answer.
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());
    assert!(
        status(&ed).contains("claude-acp wants to edit src/a.rs. [y]es [n]o (1/2)"),
        "{}",
        status(&ed)
    );
    ed.feed("y");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_once".to_owned())]
    );
    assert_eq!(text(&ed), "abcyny");
    ed.feed("n");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(51, "reject_once".to_owned())]
    );
    assert!(!ed.permission_queued());
}

#[test]
fn a_request_in_visual_mode_or_with_an_operator_pending_waits() {
    // Visual mode: `y` yanks; the prompt appears when Visual mode ends.
    let mut ed = connected_with(1, "abc def");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("v");
    request_one(&mut ed, 50);
    assert!(!ed.permission_prompt_up());
    ed.feed("ly");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());
    ed.feed("y");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_once".to_owned())]
    );
    ed.feed("$p");
    assert_eq!(text(&ed), "abc defab", "the yank was Vim's");

    // An operator pending: `d` then `w` is Vim's `dw`, then the prompt.
    let mut ed = connected_with(1, "abc def");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("d");
    request_one(&mut ed, 51);
    assert!(!ed.permission_prompt_up());
    assert!(!ed.pending_keys().is_empty());
    ed.feed("w");
    assert_eq!(text(&ed), "def");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());

    // `y` after a `d` is not an answer: it ends the operator (Vim beeps).
    let mut ed = connected_with(1, "abc def");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("d");
    request_one(&mut ed, 52);
    ed.feed("y");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert_eq!(text(&ed), "abc def");
    assert!(ed.permission_prompt_up(), "active once nothing is pending");
    ed.feed("n");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(52, "reject_once".to_owned())]
    );

    // A count pending, then `<Esc>`.
    let mut ed = connected_with(1, "abc def");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("2");
    request_one(&mut ed, 53);
    assert!(!ed.permission_prompt_up());
    ed.feed("<Esc>");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());
}

#[test]
fn agent_cancel_can_be_typed_while_a_request_is_queued() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed(":AgentCan");
    request_one(&mut ed, 50);
    request_one(&mut ed, 51);
    assert!(ed.permission_queued() && !ed.permission_prompt_up());
    assert_eq!(ed.cmdline(), "AgentCan", "the command line kept its keys");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    ed.feed("cel<CR>");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "cancelled".to_owned()), (51, "cancelled".to_owned())]
    );
    assert!(!ed.permission_queued());
    assert!(!status(&ed).contains("PERMISSION"), "{}", status(&ed));
}

#[test]
fn esc_in_the_command_line_leaves_it_and_the_next_esc_rejects() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed(":");
    request_one(&mut ed, 50);
    ed.feed("<Esc>");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    assert!(ed.permission_prompt_up());
    ed.feed("<Esc>");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "reject_once".to_owned())]
    );
}

#[test]
fn grants_answer_at_once_in_any_mode() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    ed.feed("a");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_always".to_owned())]
    );
    // Insert, Visual, command line and a pending operator.
    for (i, keys) in ["A", "v", ":", "d"].into_iter().enumerate() {
        ed.feed(keys);
        let id = 60 + i as i64;
        ask(&mut ed, id, file_tool("edit", "src/b.rs"), &ALL);
        assert_eq!(
            answers(&ed.take_agent_conn_commands()),
            [(id, "allow_always".to_owned())],
            "{keys}"
        );
        assert!(!ed.permission_queued(), "{keys}");
        ed.feed("<Esc><Esc>");
        assert_eq!(text(&ed), "abc", "{keys}");
    }
}

#[test]
fn the_reminder_works_for_a_queued_request() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    let t0 = Instant::now();
    ed.feed("v");
    request_one(&mut ed, 50);
    let deadline = ed.next_deadline().expect("the reminder is a deadline");
    assert!(deadline >= t0 + Duration::from_secs(299), "{deadline:?}");
    ed.agent_timer(deadline);
    assert!(
        status(&ed).contains("claude-acp waits for permission; <Esc> to answer"),
        "{}",
        status(&ed)
    );
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
    let next = ed.next_deadline().expect("repeats");
    assert_eq!(next, deadline + Duration::from_secs(300));
    ed.feed("<Esc>");
    assert!(ed.permission_prompt_up());
    ed.agent_timer(next);
    assert!(
        status(&ed).contains("claude-acp still waiting for y or n"),
        "{}",
        status(&ed)
    );
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
}

#[test]
fn a_queued_request_says_it_waits_on_the_message_line() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("v");
    request_one(&mut ed, 50);
    assert!(
        status(&ed).contains("claude-acp waits for permission; <Esc> to answer"),
        "{}",
        status(&ed)
    );
    assert!(status(&ed).contains("PERMISSION 1"), "{}", status(&ed));
}

#[test]
fn a_paste_is_refused_only_while_the_prompt_is_active() {
    let mut ed = connected_with(1, "abc");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("A");
    request_one(&mut ed, 50);
    ed.paste("y");
    assert_eq!(text(&ed), "abcy", "typed text, not an answer");
    ed.feed("<Esc>");
    ed.paste("y");
    assert!(ed.permission_prompt_up(), "refused while active");
    assert_eq!(text(&ed), "abcy");
    assert!(answers(&ed.take_agent_conn_commands()).is_empty());
}

#[test]
fn queued_requests_keep_the_desired_column() {
    let mut ed = connected_with(1, "abcdef\nab\nabcdef");
    let _turn = start_turn(&mut ed, "edit");
    // (Leaving Visual mode resets the desired column in Vim itself, so it is
    // not one of the states here.)
    for before in [":", "2", "d"] {
        ed.feed("gg0llllj");
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1));
        ed.feed(before);
        request_one(&mut ed, 50);
        assert!(!ed.permission_prompt_up(), "{before}");
        // Leaving that state activates the prompt; the desired column stays.
        ed.feed("<Esc>");
        assert!(ed.permission_prompt_up(), "{before}");
        ed.feed("y");
        assert_eq!(
            answers(&ed.take_agent_conn_commands()),
            [(50, "allow_once".to_owned())],
            "{before}"
        );
        ed.feed("j");
        assert_eq!((ed.cursor().line, ed.cursor().col), (2, 4), "{before}");
    }
}

#[test]
fn prompt_keys_keep_the_desired_column() {
    let mut ed = connected_with(1, "abcdef\nab\nabcdef");
    let _turn = start_turn(&mut ed, "edit");
    ed.feed("gg0llllj");
    assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1));
    ask(&mut ed, 50, file_tool("edit", "src/a.rs"), &ALL);
    // An ignored key and an answer: neither moves the cursor or resets the
    // desired column, so `j` goes back to column 4.
    ed.feed("xy");
    assert_eq!(
        answers(&ed.take_agent_conn_commands()),
        [(50, "allow_once".to_owned())]
    );
    assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1));
    ed.feed("j");
    assert_eq!((ed.cursor().line, ed.cursor().col), (2, 4));
    // A grant answer (no key) keeps it too.
    ed.feed("k");
    ask(&mut ed, 51, file_tool("edit", "src/a.rs"), &ALL);
    ed.feed("a");
    ask(&mut ed, 52, file_tool("edit", "src/b.rs"), &ALL);
    assert!(!ed.permission_prompt_up());
    ed.feed("j");
    assert_eq!((ed.cursor().line, ed.cursor().col), (2, 4));
}
