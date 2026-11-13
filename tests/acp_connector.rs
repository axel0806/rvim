// SPDX-License-Identifier: Apache-2.0
//! Tests for the ACP connector core (`src/acp.rs`, milestone 7 step 5a).
//!
//! The recorded transcripts in `tests/fixtures/acp/*.jsonl` are replayed
//! through `Conn` with a fake clock. Each record is one step: an `event`
//! (what the connector does), `expect` (the actions that must come back, in
//! order, in the normalized form `describe` produces) and optionally `state`.
//! Replay also re-runs every transcript with the agent's bytes split at every
//! position, and at every byte, and requires the same actions. Nothing here
//! spawns a process or sleeps.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rvim::acp::{
    AUTH_REQUIRED, Conn, ConnState, ContentBlock, FrameError, Id, InEvent, LineDecoder,
    MAX_LINE_BYTES, Message, Notice, OutAction, PermissionChoice, PermissionOptionKind,
    PermissionRequest, PlanEntryPriority, PlanEntryStatus, RpcError, SessionUpdate, StopReason,
    ToolCallContent, ToolCallStatus, encode,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Helpers: wire lines and events
// ---------------------------------------------------------------------------

fn line(v: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(v).expect("json");
    bytes.push(b'\n');
    bytes
}

fn wire(v: Value, now: Instant) -> InEvent {
    InEvent::Bytes {
        bytes: line(&v),
        now,
    }
}

fn connect(now: Instant) -> InEvent {
    InEvent::Connect {
        cwd: "/home/user/project".to_owned(),
        now,
    }
}

fn init_ok(id: i64, auth: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": 1, "agentCapabilities": {}, "authMethods": auth}})
}

fn session_ok(id: i64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "s1"}})
}

fn prompt_event() -> InEvent {
    InEvent::Prompt {
        blocks: vec![ContentBlock::Text {
            text: "hello".to_owned(),
        }],
    }
}

fn permission_request(id: i64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {
        "sessionId": "s1",
        "toolCall": {"toolCallId": format!("call_{id}"), "title": "Write a.rs", "kind": "edit", "status": "pending"},
        "options": [
            {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
            {"optionId": "reject", "name": "Reject", "kind": "reject_once"}
        ]
    }})
}

// Preludes bring a Conn to a state, using ids 1 (initialize), 2 (session/new)
// and 3 (the first prompt).

fn started(c: &mut Conn, t: Instant) {
    c.on_event(connect(t));
}

fn initialized(c: &mut Conn, t: Instant) {
    started(c, t);
    c.on_event(wire(init_ok(1, json!([])), t));
}

fn handshaken(c: &mut Conn, t: Instant) {
    initialized(c, t);
    c.on_event(wire(session_ok(2), t));
}

fn prompting(c: &mut Conn, t: Instant) {
    handshaken(c, t);
    c.on_event(prompt_event());
}

fn waiting_permission(c: &mut Conn, t: Instant) {
    prompting(c, t);
    c.on_event(wire(permission_request(100), t));
}

fn cancelling(c: &mut Conn, t: Instant) {
    waiting_permission(c, t);
    c.on_event(InEvent::Cancel);
}

fn needs_auth(c: &mut Conn, t: Instant) {
    started(c, t);
    c.on_event(wire(
        init_ok(
            1,
            json!([
                {"id": "oauth", "name": "Sign in"},
                {"id": "cli", "name": "Log in in a terminal", "type": "terminal"}
            ]),
        ),
        t,
    ));
    c.on_event(wire(
        json!({"jsonrpc": "2.0", "id": 2, "error": {"code": AUTH_REQUIRED, "message": "Authentication required"}}),
        t,
    ));
}

fn closed(c: &mut Conn, t: Instant) {
    started(c, t);
    c.on_event(wire(
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": "boom"}}),
        t,
    ));
}

// ---------------------------------------------------------------------------
// Replay of the transcripts
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Split {
    Whole,
    EveryByte,
    At(usize),
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("acp")
        .join(name)
}

fn load(name: &str) -> Vec<Value> {
    let text = fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("cannot read fixture {name}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| serde_json::from_str(l).unwrap_or_else(|e| panic!("{name}:{}: {e}", i + 1)))
        .collect()
}

fn is_event(step: &Value) -> bool {
    step.get("event").is_some()
}

fn at(step: &Value) -> u64 {
    step.get("at").and_then(Value::as_u64).unwrap_or(0)
}

fn str_field(step: &Value, key: &str) -> String {
    step.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("step needs string field {key}: {step}"))
        .to_owned()
}

/// The bytes an agent step sends (`in` as one JSON line, or `raw` as is).
fn wire_bytes(step: &Value) -> Vec<u8> {
    match (step.get("in"), step.get("raw")) {
        (Some(v), None) => line(v),
        (None, Some(raw)) => raw.as_str().expect("raw is a string").as_bytes().to_vec(),
        _ => panic!("a bytes step needs exactly one of \"in\" or \"raw\": {step}"),
    }
}

fn event_for(step: &Value, now: Instant) -> InEvent {
    match str_field(step, "event").as_str() {
        "connect" => InEvent::Connect {
            cwd: str_field(step, "cwd"),
            now,
        },
        "bytes" => InEvent::Bytes {
            bytes: wire_bytes(step),
            now,
        },
        "tick" => InEvent::Tick { now },
        "prompt" => InEvent::Prompt {
            blocks: vec![ContentBlock::Text {
                text: str_field(step, "text"),
            }],
        },
        "cancel" => InEvent::Cancel,
        "authenticate" => InEvent::Authenticate {
            method_id: str_field(step, "method"),
        },
        "answer" => InEvent::PermissionAnswer(if step.get("cancel").is_some() {
            PermissionChoice::Cancelled
        } else {
            PermissionChoice::Select(str_field(step, "select"))
        }),
        "file_read_answer" => InEvent::FileReadAnswer {
            request: id_field(step),
            result: match step.get("error") {
                Some(e) => Err(serde_json::from_value::<RpcError>(e.clone()).expect("error")),
                None => Ok(str_field(step, "content")),
            },
        },
        "file_write_answer" => InEvent::FileWriteAnswer {
            request: id_field(step),
            result: match step.get("error") {
                Some(e) => Err(serde_json::from_value::<RpcError>(e.clone()).expect("error")),
                None => Ok(()),
            },
        },
        "eof" => InEvent::Eof,
        other => panic!("unknown event {other}"),
    }
}

fn id_field(step: &Value) -> Id {
    Id::from_value(step["request"].clone()).expect("request id")
}

fn state_name(s: &ConnState) -> &'static str {
    match s {
        ConnState::Disconnected => "Disconnected",
        ConnState::Initializing => "Initializing",
        ConnState::NeedsAuth => "NeedsAuth",
        ConnState::Ready => "Ready",
        ConnState::Prompting { .. } => "Prompting",
        ConnState::Cancelling { .. } => "Cancelling",
        ConnState::Closed => "Closed",
    }
}

fn status_name(s: Option<ToolCallStatus>) -> Option<&'static str> {
    s.map(|s| match s {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        ToolCallStatus::Other => "other",
    })
}

fn priority_name(p: PlanEntryPriority) -> &'static str {
    match p {
        PlanEntryPriority::High => "high",
        PlanEntryPriority::Medium => "medium",
        PlanEntryPriority::Low => "low",
        PlanEntryPriority::Other => "other",
    }
}

fn entry_status_name(s: PlanEntryStatus) -> &'static str {
    match s {
        PlanEntryStatus::Pending => "pending",
        PlanEntryStatus::InProgress => "in_progress",
        PlanEntryStatus::Completed => "completed",
        PlanEntryStatus::Other => "other",
    }
}

fn stop_name(s: StopReason) -> &'static str {
    match s {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        StopReason::Other => "other",
    }
}

fn chunk_text(content: &ContentBlock) -> &str {
    match content {
        ContentBlock::Text { text } => text,
        _ => "<non-text>",
    }
}

fn tool_json(kind: &str, t: &rvim::acp::ToolCallInfo) -> Value {
    json!({"update": kind, "id": t.tool_call_id, "title": t.title, "status": status_name(t.status)})
}

/// One action in the normalized form the transcripts expect. `None` for
/// actions that are not part of the transcript format (logs).
fn describe(action: &OutAction) -> Option<Value> {
    Some(match action {
        OutAction::Send(bytes) => {
            let body = bytes.strip_suffix(b"\n").expect("one line, newline-ended");
            let mut v: Value = serde_json::from_slice(body).expect("valid JSON");
            // The client version changes with releases; the fixtures do not pin it.
            if v["method"] == "initialize" {
                v["params"]["clientInfo"]["version"] = json!("<any>");
            }
            json!({"send": v})
        }
        OutAction::Update(update) => match update {
            SessionUpdate::AgentMessageChunk(c) => {
                json!({"update": "agent_message_chunk", "text": chunk_text(&c.content)})
            }
            SessionUpdate::AgentThoughtChunk(c) => {
                json!({"update": "agent_thought_chunk", "text": chunk_text(&c.content)})
            }
            SessionUpdate::UserMessageChunk(c) => {
                json!({"update": "user_message_chunk", "text": chunk_text(&c.content)})
            }
            SessionUpdate::ToolCall(t) => tool_json("tool_call", t),
            SessionUpdate::ToolCallUpdate(t) => tool_json("tool_call_update", t),
            SessionUpdate::Plan(p) => {
                json!({"update": "plan", "entries": p.entries.iter().map(|e| json!({
                "content": e.content,
                "priority": priority_name(e.priority),
                "status": entry_status_name(e.status),
            })).collect::<Vec<_>>()})
            }
            SessionUpdate::Other { kind, .. } => json!({"update": "other", "kind": kind}),
        },
        OutAction::Notice(n) => match n {
            Notice::Connected { session } => json!({"notice": "connected", "session": session}),
            Notice::AuthRequired { methods } => {
                json!({"notice": "auth_required", "methods": methods.iter().map(|m| json!({
                "id": m.id, "name": m.name, "terminal": m.is_terminal(),
            })).collect::<Vec<_>>()})
            }
            Notice::Authenticated => json!({"notice": "authenticated"}),
            Notice::TurnStarted { turn } => json!({"notice": "turn_started", "turn": turn}),
            Notice::CancelSent { turn } => json!({"notice": "cancel_sent", "turn": turn}),
            Notice::TurnEnded { turn, stop_reason } => {
                json!({"notice": "turn_ended", "turn": turn, "stop": stop_name(*stop_reason)})
            }
            Notice::RequestFailed { turn, message } => {
                json!({"notice": "request_failed", "turn": turn, "message": message})
            }
            Notice::PromptRejected { reason } => {
                json!({"notice": "prompt_rejected", "reason": reason})
            }
            Notice::ConnectionLost { turn } => json!({"notice": "connection_lost", "turn": turn}),
            Notice::Failed { reason } => json!({"notice": "failed", "reason": reason}),
        },
        OutAction::AskPermission { request, queued } => json!({"ask_permission": {
            "tool": request.tool_call.title,
            "options": request.options.iter().map(|o| o.option_id.clone()).collect::<Vec<_>>(),
            "queued": queued,
        }}),
        OutAction::ServeRead {
            request,
            path,
            line,
            limit,
            ..
        } => {
            json!({"serve_read": {"request": request.to_value(), "path": path, "line": line, "limit": limit}})
        }
        OutAction::ServeWrite {
            request,
            path,
            content,
            ..
        } => {
            json!({"serve_write": {"request": request.to_value(), "path": path, "content": content}})
        }
        OutAction::Close => json!({"close": true}),
        OutAction::Log(_) => return None,
    })
}

#[derive(Debug, PartialEq)]
struct StepResult {
    actions: Vec<Value>,
    state: String,
}

fn pieces(bytes: Vec<u8>, split: Split) -> Vec<Vec<u8>> {
    match split {
        Split::Whole => vec![bytes],
        Split::EveryByte => bytes.into_iter().map(|b| vec![b]).collect(),
        Split::At(p) if p == 0 || p >= bytes.len() => vec![bytes],
        Split::At(p) => vec![bytes[..p].to_vec(), bytes[p..].to_vec()],
    }
}

/// Runs the event steps of a transcript. Agent bytes are fed as split.
fn run(steps: &[Value], split: Split) -> Vec<StepResult> {
    let base = Instant::now();
    let mut conn = Conn::new();
    let mut results = Vec::new();
    for step in steps.iter().filter(|s| is_event(s)) {
        let now = base + Duration::from_millis(at(step));
        let mut actions = Vec::new();
        if str_field(step, "event") == "bytes" {
            for piece in pieces(wire_bytes(step), split) {
                actions.extend(conn.on_event(InEvent::Bytes { bytes: piece, now }));
            }
        } else {
            actions.extend(conn.on_event(event_for(step, now)));
        }
        results.push(StepResult {
            actions: actions.iter().filter_map(describe).collect(),
            state: state_name(conn.state()).to_owned(),
        });
    }
    results
}

fn replay(name: &str) {
    let steps = load(name);
    let whole = run(&steps, Split::Whole);
    let event_steps: Vec<&Value> = steps.iter().filter(|s| is_event(s)).collect();
    assert_eq!(whole.len(), event_steps.len());
    for (i, (got, step)) in whole.iter().zip(&event_steps).enumerate() {
        let expect = step
            .get("expect")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{name}: step {} has no expect: {step}", i + 1));
        assert_eq!(&got.actions, expect, "{name}: step {} {step}", i + 1);
        if let Some(state) = step.get("state").and_then(Value::as_str) {
            assert_eq!(
                got.state,
                state,
                "{name}: state after step {} {step}",
                i + 1
            );
        }
    }

    // The same transcript with the agent's bytes split in every possible way.
    let longest = steps
        .iter()
        .filter(|s| is_event(s) && str_field(s, "event") == "bytes")
        .map(|s| wire_bytes(s).len())
        .max()
        .unwrap_or(0);
    assert_eq!(
        run(&steps, Split::EveryByte),
        whole,
        "{name}: split at every byte"
    );
    for p in 1..longest {
        assert_eq!(
            run(&steps, Split::At(p)),
            whole,
            "{name}: split at byte {p}"
        );
    }
}

#[test]
fn transcript_successful_session() {
    replay("success.jsonl");
}

#[test]
fn transcript_auth_required_then_login() {
    replay("auth_required.jsonl");
}

#[test]
fn transcript_auth_failed_then_retry() {
    replay("auth_failed_then_retry.jsonl");
}

#[test]
fn only_agent_flow_methods_are_offered() {
    let methods: Vec<rvim::acp::AuthMethod> = serde_json::from_value(json!([
        {"id": "a", "name": "A"},
        {"id": "b", "name": "B", "type": "agent"},
        {"id": "c", "name": "C", "type": "terminal"},
        {"id": "d", "name": "D", "type": "env_var"},
        {"id": "gateway", "name": "G"},
    ]))
    .expect("methods");
    let offered: Vec<&str> = methods
        .iter()
        .filter(|m| m.is_offered())
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(offered, vec!["a", "b"]);
    assert!(!methods[3].is_terminal() && !methods[3].is_agent_flow());
}

#[test]
fn an_unknown_method_type_is_never_sent_to_authenticate() {
    let t = Instant::now();
    let mut c = Conn::new();
    c.on_event(connect(t));
    c.on_event(wire(
        init_ok(1, json!([{"id": "e", "name": "Env", "type": "env_var"}])),
        t,
    ));
    c.on_event(wire(
        json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32000, "message": "auth"}}),
        t,
    ));
    assert_eq!(c.state(), &ConnState::NeedsAuth);
    let out = c.on_event(InEvent::Authenticate {
        method_id: "e".to_owned(),
    });
    assert!(
        !out.iter().any(|a| matches!(a, OutAction::Send(_))),
        "{out:?}"
    );
    assert_eq!(c.state(), &ConnState::NeedsAuth);
}

#[test]
fn transcript_prompt_cancelled() {
    replay("prompt_cancelled.jsonl");
}

#[test]
fn transcript_reply_timeout_with_fake_clock() {
    replay("reply_timeout.jsonl");
}

#[test]
fn transcript_eof_mid_turn() {
    replay("eof_mid_turn.jsonl");
}

#[test]
fn transcript_malformed_lines_do_not_stop_the_turn() {
    replay("malformed_line.jsonl");
}

#[test]
fn transcript_permission_queue_of_two() {
    replay("permission_queue.jsonl");
}

#[test]
fn transcript_unknown_kinds_and_file_calls() {
    replay("unknown_and_fs.jsonl");
}

// ---------------------------------------------------------------------------
// State transitions (table)
// ---------------------------------------------------------------------------

type Prelude = fn(&mut Conn, Instant);
type MakeEvent = fn(Instant) -> InEvent;
type Row = (&'static str, Prelude, MakeEvent, ConnState);

#[test]
fn state_transition_table() {
    let t0 = Instant::now();
    let no_prelude: Prelude = |_, _| {};
    let rows: Vec<Row> = vec![
        (
            "connect from disconnected",
            no_prelude,
            |t| connect(t),
            ConnState::Initializing,
        ),
        (
            "eof while disconnected",
            no_prelude,
            |_| InEvent::Eof,
            ConnState::Disconnected,
        ),
        (
            "prompt while disconnected",
            no_prelude,
            |_| prompt_event(),
            ConnState::Disconnected,
        ),
        (
            "initialize ok",
            started,
            |t| wire(init_ok(1, json!([])), t),
            ConnState::Initializing,
        ),
        (
            "initialize error",
            started,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": "x"}}),
                    t,
                )
            },
            ConnState::Closed,
        ),
        (
            "unknown protocol version",
            started,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": 2}}),
                    t,
                )
            },
            ConnState::Closed,
        ),
        (
            "no initialize reply in 10 s",
            started,
            |t| InEvent::Tick {
                now: t + Duration::from_secs(10),
            },
            ConnState::Closed,
        ),
        (
            "tick before 10 s",
            started,
            |t| InEvent::Tick {
                now: t + Duration::from_millis(9_999),
            },
            ConnState::Initializing,
        ),
        (
            "eof during handshake",
            started,
            |_| InEvent::Eof,
            ConnState::Disconnected,
        ),
        (
            "session/new ok",
            initialized,
            |t| wire(session_ok(2), t),
            ConnState::Ready,
        ),
        (
            "session/new auth required",
            initialized,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 2, "error": {"code": AUTH_REQUIRED, "message": "a"}}),
                    t,
                )
            },
            ConnState::NeedsAuth,
        ),
        (
            "session/new other error",
            initialized,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32603, "message": "a"}}),
                    t,
                )
            },
            ConnState::Closed,
        ),
        (
            "authenticate with an advertised method",
            needs_auth,
            |_| InEvent::Authenticate {
                method_id: "oauth".to_owned(),
            },
            ConnState::Initializing,
        ),
        (
            "authenticate with a terminal method",
            needs_auth,
            |_| InEvent::Authenticate {
                method_id: "cli".to_owned(),
            },
            ConnState::NeedsAuth,
        ),
        (
            "authenticate with an unknown method",
            needs_auth,
            |_| InEvent::Authenticate {
                method_id: "nope".to_owned(),
            },
            ConnState::NeedsAuth,
        ),
        (
            "prompt when ready",
            handshaken,
            |_| prompt_event(),
            ConnState::Prompting { turn: 1 },
        ),
        (
            "prompt while a turn runs",
            prompting,
            |_| prompt_event(),
            ConnState::Prompting { turn: 1 },
        ),
        (
            "cancel while prompting",
            prompting,
            |_| InEvent::Cancel,
            ConnState::Cancelling { turn: 1 },
        ),
        (
            "cancel while ready",
            handshaken,
            |_| InEvent::Cancel,
            ConnState::Ready,
        ),
        (
            "cancel twice",
            cancelling,
            |_| InEvent::Cancel,
            ConnState::Cancelling { turn: 1 },
        ),
        (
            "turn ends end_turn",
            prompting,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 3, "result": {"stopReason": "end_turn"}}),
                    t,
                )
            },
            ConnState::Ready,
        ),
        (
            "turn ends cancelled",
            cancelling,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 3, "result": {"stopReason": "cancelled"}}),
                    t,
                )
            },
            ConnState::Ready,
        ),
        (
            "turn request fails",
            prompting,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 3, "error": {"code": -32603, "message": "net"}}),
                    t,
                )
            },
            ConnState::Ready,
        ),
        (
            "eof while prompting",
            prompting,
            |_| InEvent::Eof,
            ConnState::Disconnected,
        ),
        (
            "eof while ready",
            handshaken,
            |_| InEvent::Eof,
            ConnState::Disconnected,
        ),
        (
            "connect after closed",
            closed,
            |t| connect(t),
            ConnState::Closed,
        ),
        (
            "eof after closed",
            closed,
            |_| InEvent::Eof,
            ConnState::Closed,
        ),
        (
            "ten malformed lines in a row disconnect",
            prompting,
            |_| InEvent::Bytes {
                bytes: b"bad\n".repeat(10),
                now: Instant::now(),
            },
            ConnState::Disconnected,
        ),
        (
            "nine malformed lines keep the connection",
            prompting,
            |_| InEvent::Bytes {
                bytes: b"bad\n".repeat(9),
                now: Instant::now(),
            },
            ConnState::Prompting { turn: 1 },
        ),
        (
            "permission request while prompting",
            prompting,
            |t| wire(permission_request(100), t),
            ConnState::Prompting { turn: 1 },
        ),
        (
            "permission answer with an unknown option",
            waiting_permission,
            |_| InEvent::PermissionAnswer(PermissionChoice::Select("zzz".to_owned())),
            ConnState::Prompting { turn: 1 },
        ),
        (
            "permission answer",
            waiting_permission,
            |_| InEvent::PermissionAnswer(PermissionChoice::Select("allow".to_owned())),
            ConnState::Prompting { turn: 1 },
        ),
        (
            "permission request while cancelling",
            cancelling,
            |t| wire(permission_request(101), t),
            ConnState::Cancelling { turn: 1 },
        ),
        (
            "file read request",
            handshaken,
            |t| {
                wire(
                    json!({"jsonrpc": "2.0", "id": 50, "method": "fs/read_text_file", "params": {"sessionId": "s1", "path": "/a"}}),
                    t,
                )
            },
            ConnState::Ready,
        ),
    ];

    for (name, prelude, event, expected) in rows {
        let mut conn = Conn::new();
        prelude(&mut conn, t0);
        conn.on_event(event(t0));
        assert_eq!(conn.state(), &expected, "transition: {name}");
    }
}

// ---------------------------------------------------------------------------
// Permissions and cancellation
// ---------------------------------------------------------------------------

#[test]
fn permission_is_never_answered_without_an_event() {
    let t0 = Instant::now();
    let mut conn = Conn::new();
    waiting_permission(&mut conn, t0);
    // An hour of ticks produces no answer, only the timers.
    let later = conn.on_event(InEvent::Tick {
        now: t0 + Duration::from_secs(3_600),
    });
    assert!(later.is_empty(), "tick answered a permission: {later:?}");
    assert_eq!(conn.permissions_waiting(), 1);
}

#[test]
fn cancel_answers_every_queued_permission_as_cancelled() {
    let t0 = Instant::now();
    let mut conn = Conn::new();
    waiting_permission(&mut conn, t0);
    conn.on_event(wire(permission_request(101), t0));
    assert_eq!(conn.permissions_waiting(), 2);
    let actions = conn.on_event(InEvent::Cancel);
    let sent: Vec<Value> = actions
        .iter()
        .filter_map(|a| match a {
            OutAction::Send(bytes) => {
                let body = bytes.strip_suffix(b"\n").expect("newline");
                Some(serde_json::from_slice(body).expect("json"))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        vec![
            json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": "s1"}}),
            json!({"jsonrpc": "2.0", "id": 100, "result": {"outcome": {"outcome": "cancelled"}}}),
            json!({"jsonrpc": "2.0", "id": 101, "result": {"outcome": {"outcome": "cancelled"}}}),
        ]
    );
    assert_eq!(conn.permissions_waiting(), 0);
}

#[test]
fn permission_request_carries_its_options_and_queue_length() {
    let t0 = Instant::now();
    let mut conn = Conn::new();
    let actions = {
        waiting_permission(&mut conn, t0);
        conn.on_event(wire(permission_request(101), t0))
    };
    let (request, queued): (PermissionRequest, usize) = match &actions[0] {
        OutAction::AskPermission { request, queued } => (request.clone(), *queued),
        other => panic!("expected AskPermission, got {other:?}"),
    };
    assert_eq!(queued, 2);
    assert_eq!(request.tool_call.tool_call_id, "call_100");
    let allow = request
        .option_of_kind(PermissionOptionKind::AllowOnce)
        .expect("allow_once option");
    assert_eq!(allow.option_id, "allow");
}

#[test]
fn unknown_permission_option_keeps_the_request_queued() {
    let t0 = Instant::now();
    let mut conn = Conn::new();
    waiting_permission(&mut conn, t0);
    let actions = conn.on_event(InEvent::PermissionAnswer(PermissionChoice::Select(
        "nope".to_owned(),
    )));
    assert!(actions.iter().all(|a| !matches!(a, OutAction::Send(_))));
    assert_eq!(conn.permissions_waiting(), 1);
}

#[test]
fn unknown_session_update_kind_is_kept_as_raw_json() {
    let raw = json!({"sessionUpdate": "future_kind", "payload": [1, 2]});
    match SessionUpdate::from_value(raw.clone()) {
        SessionUpdate::Other { kind, raw: got } => {
            assert_eq!(kind, "future_kind");
            assert_eq!(got, raw);
        }
        other => panic!("expected Other, got {other:?}"),
    }
    // A known kind whose body does not parse is kept too, not dropped.
    let broken = json!({"sessionUpdate": "agent_message_chunk"});
    assert!(matches!(
        SessionUpdate::from_value(broken),
        SessionUpdate::Other { .. }
    ));
}

#[test]
fn tool_call_content_and_plan_parse_typed() {
    let v = json!({"sessionUpdate": "tool_call", "toolCallId": "c", "content": [
        {"type": "diff", "path": "/a", "oldText": "x", "newText": "y"},
        {"type": "terminal", "terminalId": "t1"}
    ]});
    match SessionUpdate::from_value(v) {
        SessionUpdate::ToolCall(info) => {
            let content = info.content.expect("content");
            assert!(matches!(content[0], ToolCallContent::Diff { .. }));
            assert!(matches!(content[1], ToolCallContent::Other));
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

fn notification(method: &str) -> Vec<u8> {
    line(&json!({"jsonrpc": "2.0", "method": method, "params": {"sessionId": "s"}}))
}

#[test]
fn huge_line_is_rejected_and_the_stream_continues() {
    let mut huge = vec![b'x'; MAX_LINE_BYTES + 1];
    huge.push(b'\n');
    let mut stream = huge.clone();
    stream.extend(notification("after/huge"));

    let mut whole = LineDecoder::new();
    let results = whole.push(&stream);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(
        results[0],
        Err(FrameError::TooLong {
            limit: MAX_LINE_BYTES
        })
    );
    assert!(matches!(results[1], Ok(Message::Notification { .. })));

    // The same stream in 1 MiB reads gives the same answer.
    let mut chunked = LineDecoder::new();
    let mut got = Vec::new();
    for chunk in stream.chunks(1 << 20) {
        got.extend(chunked.push(chunk));
    }
    assert_eq!(got, results);
}

#[test]
fn line_limit_is_exact() {
    let limit = 64;
    let base = r#"{"jsonrpc":"2.0","method":"a","params":{"p":""}}"#;
    let pad = " ".repeat(limit - base.len());
    let at_limit = format!("{base}{pad}");
    assert_eq!(at_limit.len(), limit);

    let mut ok = LineDecoder::with_limit(limit);
    let results = ok.push(format!("{at_limit}\n").as_bytes());
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok(), "{results:?}");

    let over = format!("{at_limit} \n");
    let mut too_long = LineDecoder::with_limit(limit);
    assert_eq!(
        too_long.push(over.as_bytes()),
        vec![Err(FrameError::TooLong { limit })]
    );
}

#[test]
fn bad_lines_are_reported_and_skipped() {
    let mut d = LineDecoder::new();
    let results = d.push(b"\xff\xfe\n{oops}\n[1]\r\n   \n\n");
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(results[0], Err(FrameError::InvalidUtf8));
    assert!(matches!(results[1], Err(FrameError::InvalidJson(_))));
    assert!(matches!(results[2], Err(FrameError::InvalidMessage(_))));
}

#[test]
fn crlf_lines_are_accepted() {
    let mut d = LineDecoder::new();
    let body = br#"{"jsonrpc":"2.0","method":"a"}"#;
    let mut bytes = body.to_vec();
    bytes.extend_from_slice(b"\r\n");
    let results = d.push(&bytes);
    assert!(results[0].is_ok(), "{results:?}");
}

#[test]
fn partial_reads_at_every_split_point_decode_the_same() {
    let mut stream = Vec::new();
    stream.extend(notification("a/b"));
    stream.extend_from_slice(b"not json\n");
    stream.extend(line(
        &json!({"jsonrpc": "2.0", "id": 9, "result": {"x": "é"}}),
    ));
    stream.extend_from_slice(b"\n");
    stream.extend(line(
        &json!({"jsonrpc": "2.0", "id": 10, "method": "m", "params": {}}),
    ));

    let whole = LineDecoder::new().push(&stream);
    assert_eq!(whole.len(), 4, "{whole:?}");

    for p in 0..=stream.len() {
        let mut d = LineDecoder::new();
        let mut got = d.push(&stream[..p]);
        got.extend(d.push(&stream[p..]));
        assert_eq!(got, whole, "split at {p}");
    }
    let mut d = LineDecoder::new();
    let mut got = Vec::new();
    for b in &stream {
        got.extend(d.push(std::slice::from_ref(b)));
    }
    assert_eq!(got, whole, "byte by byte");
}

#[test]
fn encoded_message_is_one_line() {
    let msg = Message::Notification {
        method: "session/cancel".to_owned(),
        params: json!({"sessionId": "a\nb"}),
    };
    let bytes = encode(&msg);
    assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
    assert_eq!(bytes.last(), Some(&b'\n'));
}

#[test]
fn stop_reason_unknown_value_is_kept_as_other() {
    let v: rvim::acp::PromptResult =
        serde_json::from_value(json!({"stopReason": "some_future_reason"})).expect("parse");
    assert_eq!(v.stop_reason, StopReason::Other);
}

#[test]
fn update_text_helper_matches_fixture_shape() {
    // Guards the normalized form the transcripts rely on.
    let action = OutAction::Notice(Notice::TurnEnded {
        turn: 4,
        stop_reason: StopReason::MaxTokens,
    });
    assert_eq!(
        describe(&action),
        Some(json!({"notice": "turn_ended", "turn": 4, "stop": "max_tokens"}))
    );
    let _ = PermissionChoice::Cancelled;
    let _ = RpcError::new(1, "x");
}
