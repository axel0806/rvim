// SPDX-License-Identifier: Apache-2.0
//! Agent Client Protocol (ACP) client core, milestone 7 step 5a.
//!
//! Pure: no sockets, processes, threads or clock reads. The connector feeds
//! [`InEvent`]s (bytes read from the agent, user actions, timer ticks, end of
//! input) to [`Conn::on_event`] and performs the [`OutAction`]s that come
//! back. Spawning the agent, the consent prompt, the approval store and the
//! path sandbox for file calls are later steps; this module only decides what
//! the protocol requires.
//!
//! Spec read on 2026-11-01: ACP protocol v1 (`ProtocolVersion` 1), schema
//! package 1.28.0, from agentclientprotocol.com/protocol/{overview, transports,
//! initialization, session-setup, prompt-turn, file-system} and the v1 schema
//! in the upstream repository (commit b4c72a8, schema/v1/schema.json blob
//! 776c231). PLAN.md, "Agent experience", section 2 and the ACP section list
//! what is UNVERIFIED.

use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The protocol major version this client speaks (`ProtocolVersion`).
pub const PROTOCOL_VERSION: u16 = 1;
/// Longest line accepted from the agent, in bytes before its newline.
pub const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
/// How long `initialize` and `session/new` may take to get a reply. Turns
/// (`session/prompt`) and `authenticate` have no timeout.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Malformed lines in a row before the connection is dropped.
pub const MAX_MALFORMED_IN_A_ROW: u32 = 10;

/// JSON-RPC error codes (schema `ErrorCode`).
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const RESOURCE_NOT_FOUND: i64 = -32002;
/// "Authentication required" (schema `ErrorCode`, ACP range).
pub const AUTH_REQUIRED: i64 = -32000;

// ---------------------------------------------------------------------------
// Framing: newline-delimited JSON-RPC 2.0 (transports page: messages are
// delimited by `\n` and MUST NOT contain embedded newlines; UTF-8).
// ---------------------------------------------------------------------------

/// Why a line from the agent was not turned into a message. The line is
/// skipped; the connection itself is unaffected (see [`Conn`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The line is longer than the limit. Its remaining bytes, up to the
    /// newline, are discarded.
    TooLong {
        limit: usize,
    },
    InvalidUtf8,
    InvalidJson(String),
    /// Valid JSON that is not a JSON-RPC 2.0 message.
    InvalidMessage(String),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::TooLong { limit } => write!(f, "line longer than {limit} bytes"),
            FrameError::InvalidUtf8 => write!(f, "line is not valid UTF-8"),
            FrameError::InvalidJson(e) => write!(f, "line is not valid JSON ({e})"),
            FrameError::InvalidMessage(e) => write!(f, "not a JSON-RPC 2.0 message ({e})"),
        }
    }
}

/// Splits a byte stream into messages. Partial lines are kept until their
/// newline arrives, so the result does not depend on how the bytes were read.
#[derive(Debug)]
pub struct LineDecoder {
    limit: usize,
    line: Vec<u8>,
    overflowed: bool,
}

impl Default for LineDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl LineDecoder {
    pub fn new() -> Self {
        Self::with_limit(MAX_LINE_BYTES)
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            line: Vec::new(),
            overflowed: false,
        }
    }

    /// Feeds bytes read from the agent and returns the messages (or errors)
    /// that are now complete. Empty and whitespace-only lines produce nothing.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<Message, FrameError>> {
        let mut out = Vec::new();
        let mut rest = bytes;
        while let Some(pos) = rest.iter().position(|&b| b == b'\n') {
            self.append(&rest[..pos], &mut out);
            self.finish(&mut out);
            rest = &rest[pos + 1..];
        }
        self.append(rest, &mut out);
        out
    }

    fn append(&mut self, part: &[u8], out: &mut Vec<Result<Message, FrameError>>) {
        if self.overflowed || part.is_empty() {
            return;
        }
        if self.line.len() + part.len() > self.limit {
            // Reported once, as soon as the limit is crossed, so the result
            // is the same however the bytes were split.
            self.overflowed = true;
            self.line = Vec::new();
            out.push(Err(FrameError::TooLong { limit: self.limit }));
        } else {
            self.line.extend_from_slice(part);
        }
    }

    fn finish(&mut self, out: &mut Vec<Result<Message, FrameError>>) {
        let overflowed = std::mem::take(&mut self.overflowed);
        let mut line = std::mem::take(&mut self.line);
        if overflowed {
            return;
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        out.push(decode_line(&line));
    }
}

fn decode_line(line: &[u8]) -> Result<Message, FrameError> {
    let text = std::str::from_utf8(line).map_err(|_| FrameError::InvalidUtf8)?;
    let value: Value =
        serde_json::from_str(text).map_err(|e| FrameError::InvalidJson(e.to_string()))?;
    Message::from_value(value).map_err(FrameError::InvalidMessage)
}

/// One message as a single line, with its trailing newline.
pub fn encode(msg: &Message) -> Vec<u8> {
    let mut line = serde_json::to_vec(&msg.to_value()).unwrap_or_else(|_| b"null".to_vec());
    line.push(b'\n');
    line
}

// ---------------------------------------------------------------------------
// JSON-RPC messages
// ---------------------------------------------------------------------------

/// A JSON-RPC request id. Ids this client sends are integers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Id {
    Num(i64),
    Str(String),
    Null,
}

impl Id {
    pub fn from_value(v: Value) -> Result<Self, String> {
        match v {
            Value::Null => Ok(Id::Null),
            Value::String(s) => Ok(Id::Str(s)),
            Value::Number(n) => n
                .as_i64()
                .map(Id::Num)
                .ok_or_else(|| "request id is not an integer".to_owned()),
            _ => Err("request id must be a string, a number or null".to_owned()),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Id::Num(n) => json!(n),
            Id::Str(s) => json!(s),
            Id::Null => Value::Null,
        }
    }
}

/// JSON-RPC error object (schema `Error`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// A JSON-RPC 2.0 message in either direction. `params` and `result` stay raw
/// here; the typed structs below are parsed from them where they are used.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request {
        id: Id,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: Id,
        result: Result<Value, RpcError>,
    },
}

impl Message {
    pub fn from_value(v: Value) -> Result<Self, String> {
        let Value::Object(mut obj) = v else {
            return Err("not a JSON object".to_owned());
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err("\"jsonrpc\" is not \"2.0\"".to_owned());
        }
        let id = match obj.remove("id") {
            None => None,
            Some(v) => Some(Id::from_value(v)?),
        };
        if let Some(method) = obj.remove("method") {
            let method = method
                .as_str()
                .ok_or_else(|| "\"method\" is not a string".to_owned())?
                .to_owned();
            let params = obj.remove("params").unwrap_or(Value::Null);
            return Ok(match id {
                Some(id) if id != Id::Null => Message::Request { id, method, params },
                _ => Message::Notification { method, params },
            });
        }
        let id = id.ok_or_else(|| "neither a method nor an id".to_owned())?;
        if let Some(error) = obj.remove("error") {
            let error: RpcError =
                serde_json::from_value(error).map_err(|e| format!("bad error object: {e}"))?;
            return Ok(Message::Response {
                id,
                result: Err(error),
            });
        }
        let result = obj
            .remove("result")
            .ok_or_else(|| "response has neither \"result\" nor \"error\"".to_owned())?;
        Ok(Message::Response {
            id,
            result: Ok(result),
        })
    }

    pub fn to_value(&self) -> Value {
        let mut obj = Map::new();
        obj.insert("jsonrpc".to_owned(), json!("2.0"));
        match self {
            Message::Request { id, method, params } => {
                obj.insert("id".to_owned(), id.to_value());
                obj.insert("method".to_owned(), json!(method));
                insert_params(&mut obj, params);
            }
            Message::Notification { method, params } => {
                obj.insert("method".to_owned(), json!(method));
                insert_params(&mut obj, params);
            }
            Message::Response { id, result } => {
                obj.insert("id".to_owned(), id.to_value());
                match result {
                    Ok(v) => obj.insert("result".to_owned(), v.clone()),
                    Err(e) => obj.insert("error".to_owned(), to_value(e)),
                };
            }
        }
        Value::Object(obj)
    }
}

fn insert_params(obj: &mut Map<String, Value>, params: &Value) {
    if !params.is_null() {
        obj.insert("params".to_owned(), params.clone());
    }
}

fn to_value<T: Serialize>(v: &T) -> Value {
    // Every type serialized here is a plain struct or enum, so this does not fail.
    serde_json::to_value(v).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------------------
// Typed messages (schema v1). Unknown fields are ignored by serde.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsCapabilities {
    pub read_text_file: bool,
    pub write_text_file: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    pub fs: FsCapabilities,
    pub terminal: bool,
}

/// `Implementation` (clientInfo / agentInfo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: u16,
    pub client_capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default)]
    pub load_session: bool,
}

/// An authentication method. `type` is absent (or `"agent"`) when the agent
/// handles the flow itself through `authenticate`; `"terminal"` methods are
/// run by the client as a separate process and must never be passed to
/// `authenticate`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMethod {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

impl AuthMethod {
    pub fn is_terminal(&self) -> bool {
        self.kind.as_deref() == Some("terminal")
    }

    /// The agent handles the whole flow itself after `authenticate`: the
    /// `type` is absent or `"agent"`. Any other type (`terminal`, and types a
    /// later protocol version adds) needs the client to do something first,
    /// and is never passed to `authenticate`.
    pub fn is_agent_flow(&self) -> bool {
        matches!(self.kind.as_deref(), None | Some("agent"))
    }

    /// Whether rvim offers this method to the user and may send it. The agent
    /// must run the flow itself, and the method must not need data that only
    /// a client supplies: a custom gateway takes its base URL and headers
    /// from the client, which rvim never provides.
    pub fn is_offered(&self) -> bool {
        self.is_agent_flow() && self.id != "gateway"
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: u16,
    #[serde(default)]
    pub agent_capabilities: AgentCapabilities,
    #[serde(default)]
    pub auth_methods: Vec<AuthMethod>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticateParams {
    pub method_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionParams {
    pub cwd: String,
    /// rvim configures no MCP servers, so this is always empty. Typed as raw
    /// JSON because the MCP server shape is not used here.
    pub mcp_servers: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResult {
    pub session_id: String,
}

/// `session/load`. Types only: rvim does not resume sessions in step 5a.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionParams {
    pub session_id: String,
    pub cwd: String,
    pub mcp_servers: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    pub uri: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// A content block. Blocks of other types (images, audio, ...) parse as
/// `Other`, which keeps their message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ResourceLink {
        uri: String,
        name: String,
    },
    Resource {
        resource: EmbeddedResource,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptParams {
    pub session_id: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResult {
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelParams {
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateParams {
    pub session_id: String,
    pub update: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentChunk {
    pub content: ContentBlock,
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    #[serde(rename = "move")]
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallLocation {
    pub path: String,
    #[serde(default)]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ToolCallContent {
    Content {
        content: ContentBlock,
    },
    /// Whole old and new text of one file; rvim computes hunks itself.
    Diff {
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    #[serde(other)]
    Other,
}

/// A tool call as created (`tool_call`), updated (`tool_call_update`) or
/// shown in a permission request (`toolCall`). Only the id is required by
/// the schema for updates, so every other field is optional here.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallInfo {
    pub tool_call_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub kind: Option<ToolKind>,
    #[serde(default)]
    pub status: Option<ToolCallStatus>,
    #[serde(default)]
    pub content: Option<Vec<ToolCallContent>>,
    #[serde(default)]
    pub locations: Option<Vec<ToolCallLocation>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanEntryPriority {
    High,
    Medium,
    Low,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEntryStatus {
    Pending,
    InProgress,
    Completed,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub priority: PlanEntryPriority,
    pub status: PlanEntryStatus,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Plan {
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionParams {
    pub session_id: String,
    pub tool_call: ToolCallInfo,
    pub options: Vec<PermissionOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "outcome",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PermissionOutcome {
    Cancelled,
    Selected { option_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionResponse {
    pub outcome: PermissionOutcome,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileParams {
    pub session_id: String,
    /// Absolute path, as the agent sent it. Not checked here: the caller
    /// validates it (later step).
    pub path: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadTextFileResponse {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTextFileParams {
    pub session_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WriteTextFileResponse {}

// ---------------------------------------------------------------------------
// Session updates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SessionUpdate {
    AgentMessageChunk(ContentChunk),
    AgentThoughtChunk(ContentChunk),
    UserMessageChunk(ContentChunk),
    ToolCall(ToolCallInfo),
    ToolCallUpdate(ToolCallInfo),
    Plan(Plan),
    /// An update kind this client does not know, or one whose known kind
    /// failed to parse. The raw object is kept.
    Other {
        kind: String,
        raw: Value,
    },
}

impl SessionUpdate {
    /// Dispatches on `sessionUpdate`. Never fails: anything unparsed becomes
    /// `Other` with its raw JSON.
    pub fn from_value(v: Value) -> Self {
        let kind = v
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let parsed = match kind.as_str() {
            "agent_message_chunk" => {
                serde_json::from_value(v.clone()).map(SessionUpdate::AgentMessageChunk)
            }
            "agent_thought_chunk" => {
                serde_json::from_value(v.clone()).map(SessionUpdate::AgentThoughtChunk)
            }
            "user_message_chunk" => {
                serde_json::from_value(v.clone()).map(SessionUpdate::UserMessageChunk)
            }
            "tool_call" => serde_json::from_value(v.clone()).map(SessionUpdate::ToolCall),
            "tool_call_update" => {
                serde_json::from_value(v.clone()).map(SessionUpdate::ToolCallUpdate)
            }
            "plan" => serde_json::from_value(v.clone()).map(SessionUpdate::Plan),
            _ => return SessionUpdate::Other { kind, raw: v },
        };
        parsed.unwrap_or(SessionUpdate::Other { kind, raw: v })
    }
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// Connection states. A new `Conn` is made for each connection attempt;
/// `Closed` is final for a `Conn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    /// Not connected: before `Connect`, after a lost connection.
    Disconnected,
    /// `initialize` or `session/new` (or `authenticate`) is in flight.
    Initializing,
    /// `session/new` failed with auth required; waiting for `Authenticate`.
    NeedsAuth,
    Ready,
    Prompting {
        turn: u64,
    },
    /// `session/cancel` sent; waiting for the prompt's stop reason.
    Cancelling {
        turn: u64,
    },
    /// Handshake failed or timed out, or the agent spoke a protocol version
    /// this client does not know. The caller closes the connection.
    Closed,
}

/// What the connector tells the state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum InEvent {
    /// Start the handshake. `cwd` must be absolute.
    Connect { cwd: String, now: Instant },
    /// Bytes read from the agent's stdout.
    Bytes { bytes: Vec<u8>, now: Instant },
    /// The clock moved (timeouts).
    Tick { now: Instant },
    /// The user sent a prompt.
    Prompt { blocks: Vec<ContentBlock> },
    /// The user cancelled the turn (`\ac`, `:AgentCancel`).
    Cancel,
    /// The user chose an advertised, non-terminal auth method.
    Authenticate { method_id: String },
    /// The user answered the head of the permission queue. Only this event
    /// produces a permission answer.
    PermissionAnswer(PermissionChoice),
    /// The caller's answer to a `ServeRead`.
    FileReadAnswer {
        request: Id,
        result: Result<String, RpcError>,
    },
    /// The caller's answer to a `ServeWrite`.
    FileWriteAnswer {
        request: Id,
        result: Result<(), RpcError>,
    },
    /// The agent's output ended (EOF, process exit).
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionChoice {
    /// Select the option with this `optionId`.
    Select(String),
    /// Answer `cancelled` (for example `<Esc>` when no reject option exists).
    Cancelled,
}

/// A permission request as shown to the user.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub session_id: String,
    pub tool_call: ToolCallInfo,
    pub options: Vec<PermissionOption>,
}

impl PermissionRequest {
    /// The first option of a kind, for keys such as `y` (allow once).
    pub fn option_of_kind(&self, kind: PermissionOptionKind) -> Option<&PermissionOption> {
        self.options.iter().find(|o| o.kind == kind)
    }
}

/// Events for the UI and the agent log.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Connected {
        session: String,
    },
    AuthRequired {
        methods: Vec<AuthMethod>,
    },
    Authenticated,
    TurnStarted {
        turn: u64,
    },
    CancelSent {
        turn: u64,
    },
    TurnEnded {
        turn: u64,
        stop_reason: StopReason,
    },
    /// A request failed. `turn` is set for a prompt.
    RequestFailed {
        turn: Option<u64>,
        message: String,
    },
    PromptRejected {
        reason: String,
    },
    /// The connection ended. `turn` is the turn that was running, if any.
    ConnectionLost {
        turn: Option<u64>,
    },
    /// The handshake failed; the connection is `Closed`.
    Failed {
        reason: String,
    },
}

/// What the caller must do after an event.
#[derive(Debug, Clone, PartialEq)]
pub enum OutAction {
    /// Write these bytes (one line, newline included) to the agent's stdin.
    Send(Vec<u8>),
    /// Show a session update.
    Update(SessionUpdate),
    Notice(Notice),
    /// Ask the user. Shown for the head of the queue; `queued` counts all
    /// waiting requests. The caller answers with `InEvent::PermissionAnswer`.
    AskPermission {
        request: PermissionRequest,
        queued: usize,
    },
    /// The agent asks to read a file. The caller validates `path` (absolute,
    /// shareable) and answers with `InEvent::FileReadAnswer`.
    ServeRead {
        request: Id,
        session: String,
        path: String,
        line: Option<u32>,
        limit: Option<u32>,
    },
    /// The agent asks to write a file. The caller asks the user, writes, and
    /// answers with `InEvent::FileWriteAnswer`.
    ServeWrite {
        request: Id,
        session: String,
        path: String,
        content: String,
    },
    Log(String),
    /// Close the connection (stop the process if rvim started it).
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    NewSession,
    Authenticate,
    Prompt { turn: u64 },
}

#[derive(Debug)]
struct Outstanding {
    id: i64,
    kind: Pending,
    deadline: Option<Instant>,
}

#[derive(Debug)]
struct QueuedPermission {
    id: Id,
    request: PermissionRequest,
}

/// The session state machine. Pure: `on_event` only reads the events it is
/// given, so tests drive it with a fake clock.
#[derive(Debug)]
pub struct Conn {
    state: ConnState,
    decoder: LineDecoder,
    next_id: i64,
    outstanding: Vec<Outstanding>,
    cwd: Option<String>,
    auth_methods: Vec<AuthMethod>,
    session: Option<String>,
    turn: u64,
    permissions: VecDeque<QueuedPermission>,
    serves: Vec<Id>,
    malformed: u32,
}

impl Default for Conn {
    fn default() -> Self {
        Self::new()
    }
}

impl Conn {
    pub fn new() -> Self {
        Self {
            state: ConnState::Disconnected,
            decoder: LineDecoder::new(),
            next_id: 1,
            outstanding: Vec::new(),
            cwd: None,
            auth_methods: Vec::new(),
            session: None,
            turn: 0,
            permissions: VecDeque::new(),
            serves: Vec::new(),
            malformed: 0,
        }
    }

    pub fn state(&self) -> &ConnState {
        &self.state
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// The earliest handshake timeout still outstanding, for the caller's
    /// timer. None when no request is waiting on the clock.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.outstanding.iter().filter_map(|o| o.deadline).min()
    }

    /// Permission requests waiting for the user, including the head.
    pub fn permissions_waiting(&self) -> usize {
        self.permissions.len()
    }

    pub fn on_event(&mut self, event: InEvent) -> Vec<OutAction> {
        let mut out = Vec::new();
        match event {
            InEvent::Connect { cwd, now } => self.connect(cwd, now, &mut out),
            InEvent::Bytes { bytes, now } => self.receive(&bytes, now, &mut out),
            InEvent::Tick { now } => self.tick(now, &mut out),
            InEvent::Prompt { blocks } => self.prompt(blocks, &mut out),
            InEvent::Cancel => self.cancel(&mut out),
            InEvent::Authenticate { method_id } => self.authenticate(method_id, &mut out),
            InEvent::PermissionAnswer(choice) => self.answer_permission(choice, &mut out),
            InEvent::FileReadAnswer { request, result } => {
                self.answer_read(request, result, &mut out)
            }
            InEvent::FileWriteAnswer { request, result } => {
                self.answer_write(request, result, &mut out)
            }
            InEvent::Eof => {
                if !self.is_down() {
                    self.lost(&mut out);
                }
            }
        }
        out
    }

    fn is_down(&self) -> bool {
        matches!(self.state, ConnState::Disconnected | ConnState::Closed)
    }

    fn is_session(&self, session_id: &str) -> bool {
        self.session.as_deref() == Some(session_id)
    }

    fn connect(&mut self, cwd: String, now: Instant, out: &mut Vec<OutAction>) {
        if self.state != ConnState::Disconnected {
            out.push(OutAction::Log(
                "connect ignored: this connection is not disconnected".to_owned(),
            ));
            return;
        }
        self.cwd = Some(cwd);
        self.state = ConnState::Initializing;
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            // rvim does not serve file calls or terminals yet (step 6), so
            // it advertises neither.
            client_capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "rvim".to_owned(),
                title: None,
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
        };
        self.request(
            Pending::Initialize,
            "initialize",
            to_value(&params),
            Some(now + HANDSHAKE_TIMEOUT),
            out,
        );
    }

    /// Sends a request and records it so the reply can be matched.
    fn request(
        &mut self,
        kind: Pending,
        method: &str,
        params: Value,
        deadline: Option<Instant>,
        out: &mut Vec<OutAction>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        self.outstanding.push(Outstanding { id, kind, deadline });
        out.push(OutAction::Send(encode(&Message::Request {
            id: Id::Num(id),
            method: method.to_owned(),
            params,
        })));
    }

    fn respond(&mut self, id: Id, result: Result<Value, RpcError>, out: &mut Vec<OutAction>) {
        out.push(OutAction::Send(encode(&Message::Response { id, result })));
    }

    fn receive(&mut self, bytes: &[u8], now: Instant, out: &mut Vec<OutAction>) {
        if self.is_down() {
            return;
        }
        for item in self.decoder.push(bytes) {
            match item {
                Ok(msg) => {
                    self.malformed = 0;
                    self.dispatch(msg, now, out);
                }
                Err(e) => {
                    // Skipped and logged; the connection continues until
                    // MAX_MALFORMED_IN_A_ROW lines in a row have failed.
                    self.malformed += 1;
                    out.push(OutAction::Log(format!(
                        "skipped a line from the agent: {e}"
                    )));
                    if self.malformed >= MAX_MALFORMED_IN_A_ROW {
                        out.push(OutAction::Log(
                            "too many malformed lines in a row; disconnecting".to_owned(),
                        ));
                        self.lost(out);
                        out.push(OutAction::Close);
                        return;
                    }
                }
            }
            if self.is_down() {
                return;
            }
        }
    }

    fn dispatch(&mut self, msg: Message, now: Instant, out: &mut Vec<OutAction>) {
        match msg {
            Message::Response { id, result } => self.on_response(id, result, now, out),
            Message::Notification { method, params } => self.on_notification(&method, params, out),
            Message::Request { id, method, params } => self.on_request(id, &method, params, out),
        }
    }

    fn on_response(
        &mut self,
        id: Id,
        result: Result<Value, RpcError>,
        now: Instant,
        out: &mut Vec<OutAction>,
    ) {
        let pending = match id {
            Id::Num(n) => self
                .outstanding
                .iter()
                .position(|o| o.id == n)
                .map(|i| self.outstanding.remove(i)),
            _ => None,
        };
        let Some(pending) = pending else {
            out.push(OutAction::Log(format!(
                "ignored a response with no matching request: {}",
                id.to_value()
            )));
            return;
        };
        match pending.kind {
            Pending::Initialize => self.on_initialized(result, now, out),
            Pending::NewSession => self.on_session(result, out),
            Pending::Authenticate => self.on_authenticated(result, now, out),
            Pending::Prompt { turn } => self.on_turn_result(turn, result, out),
        }
    }

    fn on_initialized(
        &mut self,
        result: Result<Value, RpcError>,
        now: Instant,
        out: &mut Vec<OutAction>,
    ) {
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                self.fail(format!("initialize failed: {}", e.message), out);
                return;
            }
        };
        let init: InitializeResult = match serde_json::from_value(value) {
            Ok(init) => init,
            Err(e) => {
                self.fail(format!("invalid initialize response: {e}"), out);
                return;
            }
        };
        if init.protocol_version != PROTOCOL_VERSION {
            self.fail(
                format!(
                    "agent speaks protocol version {}; rvim speaks {PROTOCOL_VERSION}",
                    init.protocol_version
                ),
                out,
            );
            return;
        }
        self.auth_methods = init.auth_methods;
        self.new_session(now, out);
    }

    fn new_session(&mut self, now: Instant, out: &mut Vec<OutAction>) {
        let params = NewSessionParams {
            cwd: self.cwd.clone().unwrap_or_default(),
            mcp_servers: Vec::new(),
        };
        self.request(
            Pending::NewSession,
            "session/new",
            to_value(&params),
            Some(now + HANDSHAKE_TIMEOUT),
            out,
        );
    }

    fn on_session(&mut self, result: Result<Value, RpcError>, out: &mut Vec<OutAction>) {
        match result {
            Ok(v) => match serde_json::from_value::<NewSessionResult>(v) {
                Ok(r) => {
                    self.session = Some(r.session_id.clone());
                    self.state = ConnState::Ready;
                    out.push(OutAction::Notice(Notice::Connected {
                        session: r.session_id,
                    }));
                }
                Err(e) => self.fail(format!("invalid session/new response: {e}"), out),
            },
            Err(e) if e.code == AUTH_REQUIRED => {
                self.state = ConnState::NeedsAuth;
                out.push(OutAction::Notice(Notice::AuthRequired {
                    methods: self.auth_methods.clone(),
                }));
            }
            Err(e) => self.fail(format!("session/new failed: {}", e.message), out),
        }
    }

    fn authenticate(&mut self, method_id: String, out: &mut Vec<OutAction>) {
        if self.state != ConnState::NeedsAuth {
            out.push(OutAction::Log(
                "authenticate ignored: no authentication is needed".to_owned(),
            ));
            return;
        }
        let Some(method) = self
            .auth_methods
            .iter()
            .find(|m| m.id == method_id)
            .cloned()
        else {
            out.push(OutAction::Notice(Notice::RequestFailed {
                turn: None,
                message: format!("unknown authentication method {method_id}"),
            }));
            return;
        };
        if method.is_terminal() {
            // Never sent to the agent: the client runs terminal methods itself.
            out.push(OutAction::Notice(Notice::RequestFailed {
                turn: None,
                message: "terminal authentication is not supported yet".to_owned(),
            }));
            return;
        }
        if !method.is_agent_flow() {
            // A type this client does not know needs the client's part of the
            // flow, which the protocol says must come before `authenticate`.
            out.push(OutAction::Notice(Notice::RequestFailed {
                turn: None,
                message: format!("authentication method {method_id} is not supported"),
            }));
            return;
        }
        if !method.is_offered() {
            out.push(OutAction::Notice(Notice::RequestFailed {
                turn: None,
                message: format!(
                    "authentication method {method_id} needs data this client does not provide"
                ),
            }));
            return;
        }
        self.state = ConnState::Initializing;
        self.request(
            Pending::Authenticate,
            "authenticate",
            to_value(&AuthenticateParams { method_id }),
            None,
            out,
        );
    }

    fn on_authenticated(
        &mut self,
        result: Result<Value, RpcError>,
        now: Instant,
        out: &mut Vec<OutAction>,
    ) {
        match result {
            Ok(_) => {
                out.push(OutAction::Notice(Notice::Authenticated));
                self.new_session(now, out);
            }
            Err(e) => {
                self.state = ConnState::NeedsAuth;
                out.push(OutAction::Notice(Notice::RequestFailed {
                    turn: None,
                    message: format!("authenticate failed: {}", e.message),
                }));
                // The agent still needs a login: offer the methods again, so
                // the user can pick another or give up.
                out.push(OutAction::Notice(Notice::AuthRequired {
                    methods: self.auth_methods.clone(),
                }));
            }
        }
    }

    fn prompt(&mut self, blocks: Vec<ContentBlock>, out: &mut Vec<OutAction>) {
        if self.state != ConnState::Ready {
            let reason = if matches!(
                self.state,
                ConnState::Prompting { .. } | ConnState::Cancelling { .. }
            ) {
                "a turn is already running"
            } else {
                "no session is ready"
            };
            out.push(OutAction::Notice(Notice::PromptRejected {
                reason: reason.to_owned(),
            }));
            return;
        }
        let Some(session_id) = self.session.clone() else {
            out.push(OutAction::Notice(Notice::PromptRejected {
                reason: "no session is ready".to_owned(),
            }));
            return;
        };
        self.turn += 1;
        let turn = self.turn;
        // No deadline: a turn may run as long as the agent needs.
        self.request(
            Pending::Prompt { turn },
            "session/prompt",
            to_value(&PromptParams {
                session_id,
                prompt: blocks,
            }),
            None,
            out,
        );
        self.state = ConnState::Prompting { turn };
        out.push(OutAction::Notice(Notice::TurnStarted { turn }));
    }

    fn cancel(&mut self, out: &mut Vec<OutAction>) {
        let ConnState::Prompting { turn } = self.state else {
            out.push(OutAction::Log(
                "cancel ignored: no turn is running".to_owned(),
            ));
            return;
        };
        let Some(session_id) = self.session.clone() else {
            return;
        };
        self.state = ConnState::Cancelling { turn };
        out.push(OutAction::Send(encode(&Message::Notification {
            method: "session/cancel".to_owned(),
            params: to_value(&CancelParams { session_id }),
        })));
        self.cancel_permissions(out);
        out.push(OutAction::Notice(Notice::CancelSent { turn }));
    }

    /// Answers every queued permission request as `cancelled`. Never approves.
    fn cancel_permissions(&mut self, out: &mut Vec<OutAction>) {
        while let Some(queued) = self.permissions.pop_front() {
            let outcome = RequestPermissionResponse {
                outcome: PermissionOutcome::Cancelled,
            };
            self.respond(queued.id, Ok(to_value(&outcome)), out);
        }
    }

    fn on_turn_result(
        &mut self,
        turn: u64,
        result: Result<Value, RpcError>,
        out: &mut Vec<OutAction>,
    ) {
        let current = match self.state {
            ConnState::Prompting { turn } | ConnState::Cancelling { turn } => turn,
            _ => {
                out.push(OutAction::Log(
                    "ignored a prompt response: no turn is running".to_owned(),
                ));
                return;
            }
        };
        if current != turn {
            out.push(OutAction::Log(format!(
                "ignored a prompt response for turn {turn}; turn {current} is running"
            )));
            return;
        }
        self.state = ConnState::Ready;
        // A turn that ended with requests still queued: they are answered
        // as cancelled, never approved.
        self.cancel_permissions(out);
        match result {
            Ok(v) => match serde_json::from_value::<PromptResult>(v) {
                Ok(r) => out.push(OutAction::Notice(Notice::TurnEnded {
                    turn,
                    stop_reason: r.stop_reason,
                })),
                Err(e) => out.push(OutAction::Notice(Notice::RequestFailed {
                    turn: Some(turn),
                    message: format!("invalid session/prompt response: {e}"),
                })),
            },
            // Auth required on a prompt is the same state as on session/new
            // (research M2): the agent needs a login before it can answer.
            Err(e) if e.code == AUTH_REQUIRED => {
                self.state = ConnState::NeedsAuth;
                out.push(OutAction::Notice(Notice::AuthRequired {
                    methods: self.auth_methods.clone(),
                }));
            }
            Err(e) => out.push(OutAction::Notice(Notice::RequestFailed {
                turn: Some(turn),
                message: e.message,
            })),
        }
    }

    fn on_notification(&mut self, method: &str, params: Value, out: &mut Vec<OutAction>) {
        if method != "session/update" {
            out.push(OutAction::Log(format!("ignored notification {method}")));
            return;
        }
        let params: SessionUpdateParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                out.push(OutAction::Log(format!(
                    "ignored a malformed session/update: {e}"
                )));
                return;
            }
        };
        if !self.is_session(&params.session_id) {
            out.push(OutAction::Log(
                "ignored a session/update for another session".to_owned(),
            ));
            return;
        }
        match SessionUpdate::from_value(params.update) {
            SessionUpdate::Other { kind, .. } => {
                out.push(OutAction::Log(format!(
                    "ignored session/update kind {kind}"
                )));
            }
            update => out.push(OutAction::Update(update)),
        }
    }

    fn on_request(&mut self, id: Id, method: &str, params: Value, out: &mut Vec<OutAction>) {
        match method {
            "session/request_permission" => self.on_permission_request(id, params, out),
            "fs/read_text_file" => self.on_read_request(id, params, out),
            "fs/write_text_file" => self.on_write_request(id, params, out),
            _ => self.respond(
                id,
                Err(RpcError::new(
                    METHOD_NOT_FOUND,
                    format!("method not supported by rvim: {method}"),
                )),
                out,
            ),
        }
    }

    fn on_permission_request(&mut self, id: Id, params: Value, out: &mut Vec<OutAction>) {
        let params: RequestPermissionParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                let err = RpcError::new(INVALID_PARAMS, format!("invalid params: {e}"));
                self.respond(id, Err(err), out);
                return;
            }
        };
        if !self.is_session(&params.session_id) {
            let err = RpcError::new(INVALID_PARAMS, "unknown session");
            self.respond(id, Err(err), out);
            return;
        }
        if matches!(self.state, ConnState::Cancelling { .. }) {
            // The request raced the cancel: answer it without asking.
            let outcome = RequestPermissionResponse {
                outcome: PermissionOutcome::Cancelled,
            };
            self.respond(id, Ok(to_value(&outcome)), out);
            return;
        }
        let request = PermissionRequest {
            session_id: params.session_id,
            tool_call: params.tool_call,
            options: params.options,
        };
        self.permissions.push_back(QueuedPermission { id, request });
        self.ask_head(out);
    }

    /// Emits the head of the queue. Called after every change to the queue,
    /// so the count the user sees is current.
    fn ask_head(&self, out: &mut Vec<OutAction>) {
        if let Some(head) = self.permissions.front() {
            out.push(OutAction::AskPermission {
                request: head.request.clone(),
                queued: self.permissions.len(),
            });
        }
    }

    fn on_read_request(&mut self, id: Id, params: Value, out: &mut Vec<OutAction>) {
        let p: ReadTextFileParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                let err = RpcError::new(INVALID_PARAMS, format!("invalid params: {e}"));
                self.respond(id, Err(err), out);
                return;
            }
        };
        if !self.is_session(&p.session_id) {
            let err = RpcError::new(INVALID_PARAMS, "unknown session");
            self.respond(id, Err(err), out);
            return;
        }
        self.serves.push(id.clone());
        out.push(OutAction::ServeRead {
            request: id,
            session: p.session_id,
            path: p.path,
            line: p.line,
            limit: p.limit,
        });
    }

    fn on_write_request(&mut self, id: Id, params: Value, out: &mut Vec<OutAction>) {
        let p: WriteTextFileParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                let err = RpcError::new(INVALID_PARAMS, format!("invalid params: {e}"));
                self.respond(id, Err(err), out);
                return;
            }
        };
        if !self.is_session(&p.session_id) {
            let err = RpcError::new(INVALID_PARAMS, "unknown session");
            self.respond(id, Err(err), out);
            return;
        }
        self.serves.push(id.clone());
        out.push(OutAction::ServeWrite {
            request: id,
            session: p.session_id,
            path: p.path,
            content: p.content,
        });
    }

    fn take_serve(&mut self, id: &Id) -> bool {
        match self.serves.iter().position(|s| s == id) {
            Some(pos) => {
                self.serves.remove(pos);
                true
            }
            None => false,
        }
    }

    fn answer_read(
        &mut self,
        request: Id,
        result: Result<String, RpcError>,
        out: &mut Vec<OutAction>,
    ) {
        if !self.take_serve(&request) {
            out.push(OutAction::Log(
                "ignored an answer to no file request".to_owned(),
            ));
            return;
        }
        let result = result.map(|content| to_value(&ReadTextFileResponse { content }));
        self.respond(request, result, out);
    }

    fn answer_write(
        &mut self,
        request: Id,
        result: Result<(), RpcError>,
        out: &mut Vec<OutAction>,
    ) {
        if !self.take_serve(&request) {
            out.push(OutAction::Log(
                "ignored an answer to no file request".to_owned(),
            ));
            return;
        }
        let result = result.map(|()| to_value(&WriteTextFileResponse {}));
        self.respond(request, result, out);
    }

    fn answer_permission(&mut self, choice: PermissionChoice, out: &mut Vec<OutAction>) {
        let Some(head) = self.permissions.front() else {
            out.push(OutAction::Log(
                "no permission request is waiting".to_owned(),
            ));
            return;
        };
        let outcome = match choice {
            PermissionChoice::Select(option_id) => {
                if !head
                    .request
                    .options
                    .iter()
                    .any(|o| o.option_id == option_id)
                {
                    out.push(OutAction::Log(format!(
                        "unknown permission option {option_id}; still waiting"
                    )));
                    return;
                }
                PermissionOutcome::Selected { option_id }
            }
            PermissionChoice::Cancelled => PermissionOutcome::Cancelled,
        };
        let Some(answered) = self.permissions.pop_front() else {
            return;
        };
        let response = RequestPermissionResponse { outcome };
        self.respond(answered.id, Ok(to_value(&response)), out);
        self.ask_head(out);
    }

    fn tick(&mut self, now: Instant, out: &mut Vec<OutAction>) {
        let expired = self
            .outstanding
            .iter()
            .position(|o| o.deadline.is_some_and(|d| now >= d));
        if let Some(pos) = expired {
            let request = self.outstanding.remove(pos);
            let what = match request.kind {
                Pending::Initialize => "initialize",
                Pending::NewSession => "session/new",
                _ => "request",
            };
            self.fail(
                format!("no reply to {what} after {}s", HANDSHAKE_TIMEOUT.as_secs()),
                out,
            );
        }
    }

    /// The connection is gone. Turn and queue are lost; the next connection
    /// starts a new turn (the lost one is not resumed).
    fn lost(&mut self, out: &mut Vec<OutAction>) {
        let turn = match self.state {
            ConnState::Prompting { turn } | ConnState::Cancelling { turn } => Some(turn),
            _ => None,
        };
        self.reset();
        out.push(OutAction::Notice(Notice::ConnectionLost { turn }));
    }

    fn reset(&mut self) {
        self.state = ConnState::Disconnected;
        self.decoder = LineDecoder::new();
        self.outstanding.clear();
        self.session = None;
        self.permissions.clear();
        self.serves.clear();
        self.malformed = 0;
        self.auth_methods.clear();
    }

    fn fail(&mut self, reason: String, out: &mut Vec<OutAction>) {
        self.reset();
        self.state = ConnState::Closed;
        out.push(OutAction::Notice(Notice::Failed { reason }));
        out.push(OutAction::Close);
    }
}
