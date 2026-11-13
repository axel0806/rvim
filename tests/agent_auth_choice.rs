// SPDX-License-Identifier: Apache-2.0
//! Subscription sign-in (PLAN.md, "Agent SDKs and subscriptions"): when an
//! agent answers `session/new` with auth required, rvim offers the methods the
//! agent advertised on the message line, `1` to `9`, and sends `authenticate`
//! only after the user presses a number. rvim never reads, stores or sends a
//! credential: the agent signs in with its own stored login or its own browser
//! flow.
//!
//! The fake agent is in-process, as in `tests/agent_connect.rs`: the editor's
//! worker requests are read from `take_agent_conn_commands`, and the agent's
//! replies come back as `Event::AgentConn`. The advertised methods are the ones
//! the adapters' sources list (see the registry notes in `src/agent_detect.rs`).

use std::path::PathBuf;

use rvim::agent_approval::Approvals;
use rvim::agent_conn::{AgentEvent, ConnCmd};
use rvim::agent_detect::{AGENTS, How, ScanSummary};
use rvim::event::Event;
use rvim::key::Key;
use rvim::{Editor, view};
use serde_json::{Value, json};

#[cfg(windows)]
const ROOT: &str = "C:\\proj";
#[cfg(not(windows))]
const ROOT: &str = "/proj";

/// An editor with `id` found by a scan, a project root and the approval for
/// `id` stored.
fn editor(id: &'static str) -> Editor {
    let mut ed = Editor::new();
    ed.set_screen_size(200, 24);
    ed.set_agent_project(PathBuf::from(ROOT));
    let mut approvals = Approvals::default();
    approvals.approve(id, ROOT);
    ed.set_agent_approvals(approvals, None);
    ed.handle_event(Event::AgentScan(ScanSummary {
        found: vec![(id, How::OnPath)],
        config_only: vec![],
        not_found: vec![],
        warnings: vec![],
    }));
    ed
}

fn status(ed: &Editor) -> String {
    view::frame(ed).status
}

fn line(v: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(v).expect("json");
    bytes.push(b'\n');
    bytes
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

fn reply(ed: &mut Editor, v: Value) {
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Bytes(line(&v)),
    });
}

/// Takes the commands, answers `initialize` with `methods` and `session/new`
/// with auth required, and returns every command taken on the way.
fn until_auth_required(ed: &mut Editor, methods: Value) -> Vec<ConnCmd> {
    let mut all = Vec::new();
    for _ in 0..2 {
        let cmds = ed.take_agent_conn_commands();
        for req in sent(&cmds) {
            let id = req["id"].clone();
            match req["method"].as_str() {
                Some("initialize") => reply(
                    ed,
                    json!({"jsonrpc": "2.0", "id": id, "result": {
                        "protocolVersion": 1, "agentCapabilities": {},
                        "authMethods": methods.clone()}}),
                ),
                Some("session/new") => reply(
                    ed,
                    json!({"jsonrpc": "2.0", "id": id, "error": {
                        "code": -32000, "message": "Authentication required"}}),
                ),
                _ => {}
            }
        }
        all.extend(cmds);
    }
    // What the last reply caused (a stop, say).
    all.extend(ed.take_agent_conn_commands());
    all
}

/// The `authenticate` requests among `cmds`.
fn authenticates(cmds: &[ConnCmd]) -> Vec<Value> {
    sent(cmds)
        .into_iter()
        .filter(|r| r["method"] == "authenticate")
        .collect()
}

/// What `codex-acp` advertises when the client has no gateway or URL
/// elicitation capability, plus a device-code method as a client with elicitation
/// would see it (src/CodexAuthMethod.ts).
fn codex_methods() -> Value {
    json!([
        {"id": "api-key", "name": "API Key", "description": "Use an API key to authenticate"},
        {"id": "chat-gpt", "name": "ChatGPT", "description": "Use ChatGPT to authenticate"},
        {"id": "chat-gpt-device-code", "name": "ChatGPT (device code)"},
        {"id": "gateway", "name": "Custom model gateway"},
    ])
}

#[test]
fn a_choice_is_offered_and_nothing_is_sent_without_a_key() {
    let mut ed = editor("codex");
    ed.agent_connect(None);
    let cmds = until_auth_required(&mut ed, codex_methods());
    assert!(
        authenticates(&cmds).is_empty(),
        "no authenticate before the user chooses"
    );
    assert_eq!(
        stops(&cmds),
        0,
        "the process stays up while the user chooses"
    );
    let msg = status(&ed);
    // The subscription login first, then the others; the gateway is never offered.
    assert!(
        msg.contains("[1] ChatGPT [2] ChatGPT (device code) [3] API Key [n]ot now"),
        "{msg}"
    );
    assert!(!msg.contains("gateway"), "{msg}");
    assert!(msg.contains("ai: codex login?"), "{msg}");
    // Other keys do nothing and the choice stays up.
    ed.handle_key(Key::Char('x'));
    ed.handle_key(Key::Char('9'));
    ed.handle_key(Key::Enter);
    let cmds = ed.take_agent_conn_commands();
    assert!(authenticates(&cmds).is_empty());
    assert_eq!(stops(&cmds), 0);
    assert!(status(&ed).contains("ai: codex login?"), "{}", status(&ed));
}

#[test]
fn choosing_a_method_signs_in_through_the_agent_and_connects() {
    let mut ed = editor("codex");
    ed.agent_connect(None);
    until_auth_required(&mut ed, codex_methods());
    ed.handle_key(Key::Char('1'));
    let cmds = ed.take_agent_conn_commands();
    let auth = authenticates(&cmds);
    assert_eq!(auth.len(), 1, "{auth:?}");
    assert_eq!(auth[0]["params"], json!({"methodId": "chat-gpt"}));
    assert!(
        status(&ed).contains("signing in with ChatGPT"),
        "{}",
        status(&ed)
    );
    // The agent finishes its own sign-in, then the session is created again.
    reply(
        &mut ed,
        json!({"jsonrpc": "2.0", "id": auth[0]["id"], "result": {}}),
    );
    let cmds = ed.take_agent_conn_commands();
    let again = sent(&cmds);
    assert_eq!(again.len(), 1, "{again:?}");
    assert_eq!(again[0]["method"], "session/new");
    reply(
        &mut ed,
        json!({"jsonrpc": "2.0", "id": again[0]["id"], "result": {"sessionId": "s1"}}),
    );
    assert!(
        status(&ed).contains("codex-acp: connected"),
        "{}",
        status(&ed)
    );
    assert!(status(&ed).contains("ai: codex"), "{}", status(&ed));
    assert!(!status(&ed).contains("login?"), "{}", status(&ed));
    assert_eq!(stops(&ed.take_agent_conn_commands()), 0);
}

#[test]
fn not_now_sends_nothing_stops_the_process_and_shows_the_login_hint() {
    for key in [Key::Char('n'), Key::Esc, Key::Ctrl('c')] {
        let mut ed = editor("codex");
        ed.agent_connect(None);
        until_auth_required(&mut ed, codex_methods());
        ed.handle_key(key);
        let cmds = ed.take_agent_conn_commands();
        assert!(authenticates(&cmds).is_empty(), "{key:?}");
        assert_eq!(stops(&cmds), 1, "{key:?}");
        let msg = status(&ed);
        assert!(msg.contains("ai: codex error"), "{msg}");
        assert!(msg.contains("not logged in; run `codex login`"), "{msg}");
        // The choice is gone: keys are Vim's again.
        ed.handle_key(Key::Char('1'));
        assert!(authenticates(&ed.take_agent_conn_commands()).is_empty());
    }
}

#[test]
fn a_failed_sign_in_offers_the_choice_again_with_the_reason() {
    let mut ed = editor("codex");
    ed.agent_connect(None);
    until_auth_required(&mut ed, codex_methods());
    ed.handle_key(Key::Char('1'));
    let auth = authenticates(&ed.take_agent_conn_commands());
    reply(
        &mut ed,
        json!({"jsonrpc": "2.0", "id": auth[0]["id"], "error": {
            "code": -32000, "message": "Sign-in did not complete"}}),
    );
    let msg = status(&ed);
    assert!(
        msg.contains("authenticate failed: Sign-in did not"),
        "{msg}"
    );
    assert!(msg.contains("[1] ChatGPT"), "{msg}");
    assert!(msg.contains("ai: codex login?"), "{msg}");
    // A second try works the same way.
    ed.handle_key(Key::Char('3'));
    let auth = authenticates(&ed.take_agent_conn_commands());
    assert_eq!(auth[0]["params"], json!({"methodId": "api-key"}));
}

#[test]
fn terminal_only_methods_are_never_offered_or_sent() {
    // claude-agent-acp advertises only terminal methods (src/auth-methods.ts).
    let mut ed = editor("claude");
    ed.agent_connect(None);
    let cmds = until_auth_required(
        &mut ed,
        json!([
            {"id": "claude-ai-login", "name": "Claude Subscription", "type": "terminal",
             "args": ["--cli", "auth", "login", "--claudeai"]},
            {"id": "console-login", "name": "Anthropic Console", "type": "terminal"},
        ]),
    );
    assert!(authenticates(&cmds).is_empty());
    assert_eq!(stops(&cmds), 1, "nothing to offer: the process is stopped");
    let msg = status(&ed);
    assert!(msg.contains("ai: claude error"), "{msg}");
    assert!(
        msg.contains("not logged in; run `claude auth login`"),
        "{msg}"
    );
    assert!(!msg.contains("[1]"), "{msg}");
    ed.handle_key(Key::Char('1'));
    assert!(authenticates(&ed.take_agent_conn_commands()).is_empty());
}

#[test]
fn another_connect_shows_the_choice_again_and_disconnect_drops_it() {
    let mut ed = editor("codex");
    ed.agent_connect(None);
    until_auth_required(&mut ed, codex_methods());
    ed.handle_key(Key::Char('n'));
    // A new connection after the choice was declined starts afresh.
    ed.agent_connect(None);
    let cmds = ed.take_agent_conn_commands();
    assert!(
        cmds.iter()
            .any(|c| matches!(c, ConnCmd::Start { generation: 2, .. })),
        "{cmds:?}"
    );

    let mut ed = editor("codex");
    ed.agent_connect(None);
    until_auth_required(&mut ed, codex_methods());
    ed.agent_connect(None);
    assert!(status(&ed).contains("[1] ChatGPT"), "{}", status(&ed));
    assert!(
        ed.take_agent_conn_commands().is_empty(),
        "no second process"
    );
    ed.agent_disconnect();
    assert_eq!(stops(&ed.take_agent_conn_commands()), 1);
    ed.handle_key(Key::Char('1'));
    assert!(authenticates(&ed.take_agent_conn_commands()).is_empty());
}

#[test]
fn control_characters_in_a_method_name_do_not_reach_the_message_line() {
    let mut ed = editor("codex");
    ed.agent_connect(None);
    until_auth_required(&mut ed, json!([{"id": "x", "name": "Evil\u{1b}[2J\nname"}]));
    let msg = status(&ed);
    assert!(!msg.contains('\u{1b}') && !msg.contains('\n'), "{msg:?}");
    assert!(msg.contains("[1] Evil [2J name"), "{msg}");
}

#[test]
fn registry_commands_launch_the_subscription_login_route() {
    for a in AGENTS {
        assert!(!a.login.is_empty(), "{} has a login hint", a.id);
        assert!(!a.acp.is_empty(), "{} has an ACP command", a.id);
        if a.acp.starts_with("npx ") {
            assert!(
                a.acp.starts_with("npx -y "),
                "{}: npx must not stop to ask: {}",
                a.id,
                a.acp
            );
            // The package is pinned to a version.
            let pkg = a.acp.split_whitespace().nth(2).expect("package");
            let version = pkg.rsplit_once('@').expect("pinned").1;
            assert!(
                version.chars().next().is_some_and(|c| c.is_ascii_digit()),
                "{}: {}",
                a.id,
                a.acp
            );
        }
        // Method ids named as login methods are never terminal or gateway ids.
        for m in a.login_methods {
            assert_ne!(*m, "gateway", "{}", a.id);
        }
    }
    let by_id = |id: &str| AGENTS.iter().find(|a| a.id == id).expect(id);
    assert_eq!(
        by_id("claude").acp,
        "npx -y @agentclientprotocol/claude-agent-acp@0.107.0"
    );
    assert_eq!(
        by_id("codex").acp,
        "npx -y @agentclientprotocol/codex-acp@2.6.0"
    );
    assert_eq!(by_id("antigravity").acp, "agy acp");
    assert_eq!(
        by_id("gemini").acp,
        "npx -y @google/gemini-cli@0.68.0 --acp"
    );
    assert_eq!(
        by_id("github-copilot-cli").acp,
        "npx -y @github/copilot@1.0.110 --acp"
    );
    assert!(by_id("codex").login_methods.contains(&"chat-gpt"));
    assert!(by_id("cursor").login_methods.contains(&"cursor_login"));
}
