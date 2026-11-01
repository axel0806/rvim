// SPDX-License-Identifier: Apache-2.0
//! `-32000` on `session/prompt` is the auth-required state, as it is on a
//! failing `session/new` (research M2; PLAN.md, "ACP connector, step 5b-2").
//! Driven on the pure state machine, with no editor in between.

use std::time::Instant;

use rvim::acp::{Conn, ConnState, ContentBlock, InEvent, Notice, OutAction};
use serde_json::{Value, json};

fn reply(conn: &mut Conn, v: Value) -> Vec<OutAction> {
    let mut bytes = serde_json::to_vec(&v).expect("json");
    bytes.push(b'\n');
    conn.on_event(InEvent::Bytes {
        bytes,
        now: Instant::now(),
    })
}

#[test]
fn auth_required_on_a_prompt_enters_needs_auth() {
    let mut conn = Conn::new();
    conn.on_event(InEvent::Connect {
        cwd: "/proj".to_owned(),
        now: Instant::now(),
    });
    reply(
        &mut conn,
        json!({"jsonrpc": "2.0", "id": 1, "result": {
        "protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}}),
    );
    reply(
        &mut conn,
        json!({"jsonrpc": "2.0", "id": 2, "result": {"sessionId": "s1"}}),
    );
    assert_eq!(conn.state(), &ConnState::Ready);
    conn.on_event(InEvent::Prompt {
        blocks: vec![ContentBlock::Text {
            text: "hi".to_owned(),
        }],
    });
    assert!(matches!(conn.state(), ConnState::Prompting { .. }));
    let actions = reply(
        &mut conn,
        json!({"jsonrpc": "2.0", "id": 3, "error": {
        "code": -32000, "message": "Authentication required"}}),
    );
    assert_eq!(conn.state(), &ConnState::NeedsAuth);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, OutAction::Notice(Notice::AuthRequired { .. }))),
        "{actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, OutAction::Notice(Notice::RequestFailed { .. })))
    );
}
