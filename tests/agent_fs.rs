// SPDX-License-Identifier: Apache-2.0
//! Served file calls, `fs/read_text_file` and `fs/write_text_file` (PLAN.md,
//! "Agent experience", sections 3, 4 and 10; milestone 7 step 6b). The agent
//! is the in-process fake of tests/agent_turn.rs; the file thread is run in
//! the test with `agent_fs::run`, on a real temporary project directory.

use std::path::{Path, PathBuf};

use rvim::agent_approval::Approvals;
use rvim::agent_conn::{AgentEvent, ConnCmd};
use rvim::agent_detect::{How, ScanSummary};
use rvim::agent_fs;
use rvim::event::Event;
use rvim::{Editor, view};
use serde_json::{Value, json};

/// A fresh project directory for one test.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rvim-fs-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn editor(root: &Path, file: Option<&Path>) -> Editor {
    let mut ed = match file {
        Some(f) => Editor::open(f),
        None => Editor::new(),
    };
    ed.set_screen_size(200, 24);
    ed.set_agent_project(root.to_path_buf());
    let mut approvals = Approvals::default();
    approvals.approve("claude", &root.to_string_lossy());
    ed.set_agent_approvals(approvals, None);
    ed.handle_event(Event::AgentScan(ScanSummary {
        found: vec![("claude", How::OnPath)],
        config_only: vec![],
        not_found: vec![],
        warnings: vec![],
    }));
    ed
}

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

fn agent(ed: &mut Editor, v: Value) {
    let mut bytes = serde_json::to_vec(&v).expect("json");
    bytes.push(b'\n');
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Bytes(bytes),
    });
}

/// Connects to the fake agent (generation 1, session `s1`).
fn connect(ed: &mut Editor) {
    ed.agent_connect(None);
    loop {
        let requests = sent(&ed.take_agent_conn_commands());
        if requests.is_empty() {
            break;
        }
        for req in requests {
            let id = req["id"].clone();
            let reply = match req["method"].as_str().unwrap_or_default() {
                "initialize" => {
                    let fs = &req["params"]["clientCapabilities"]["fs"];
                    assert_eq!(fs["readTextFile"], true);
                    assert_eq!(fs["writeTextFile"], true);
                    json!({"jsonrpc": "2.0", "id": id, "result": {
                        "protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}})
                }
                "session/new" => {
                    json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "s1"}})
                }
                _ => continue,
            };
            agent(ed, reply);
        }
    }
    assert!(status(ed).contains("ai: claude"), "{}", status(ed));
}

/// The file jobs among `cmds`, taken out.
fn fs_jobs(cmds: Vec<ConnCmd>) -> (Vec<agent_fs::FsJob>, Vec<ConnCmd>) {
    let mut jobs = Vec::new();
    let mut rest = Vec::new();
    for c in cmds {
        match c {
            ConnCmd::Fs { generation: 1, job } => jobs.push(job),
            other => rest.push(other),
        }
    }
    (jobs, rest)
}

/// Runs the jobs as the file thread does and returns what rvim sent after.
fn run_jobs(ed: &mut Editor, jobs: Vec<agent_fs::FsJob>) -> Vec<Value> {
    for job in jobs {
        let done = agent_fs::run(job);
        ed.handle_event(Event::AgentConn {
            generation: 1,
            event: AgentEvent::Fs(done),
        });
    }
    sent(&ed.take_agent_conn_commands())
}

/// Sends one file request and runs the file thread; returns the response.
fn call(ed: &mut Editor, id: i64, method: &str, params: Value) -> Value {
    let mut params = params;
    params["sessionId"] = json!("s1");
    agent(
        ed,
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
    );
    let (jobs, rest) = fs_jobs(ed.take_agent_conn_commands());
    let mut replies = sent(&rest);
    replies.extend(run_jobs(ed, jobs));
    replies
        .into_iter()
        .find(|v| v["id"] == id)
        .expect("a response to the file request")
}

fn read(ed: &mut Editor, id: i64, path: &Path, line: Option<u32>, limit: Option<u32>) -> Value {
    let mut params = json!({"path": path.to_string_lossy()});
    if let Some(l) = line {
        params["line"] = json!(l);
    }
    if let Some(l) = limit {
        params["limit"] = json!(l);
    }
    call(ed, id, "fs/read_text_file", params)
}

fn text(ed: &Editor) -> String {
    ed.buf().lines().join("\n")
}

/// FNV-1a 64, as `reload::fnv1a` (the watcher's hash).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The watcher's report of `bytes` for the current document.
fn watcher(ed: &mut Editor, bytes: &[u8]) {
    let (doc, version) = ed.current_doc_version();
    ed.handle_event(Event::FileChanged {
        doc,
        version,
        bytes: bytes.to_vec(),
        mtime: None,
        size: bytes.len(),
        hash: fnv1a(bytes),
    });
}

#[test]
fn a_read_of_an_open_file_returns_its_unsaved_text() {
    let root = project("unsaved");
    let file = root.join("a.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").expect("write");
    let mut ed = editor(&root, Some(&file));
    connect(&mut ed);
    ed.feed("ddAx<Esc>");
    assert_eq!(text(&ed), "twox\nthree");
    let reply = read(&mut ed, 50, &file, None, None);
    assert_eq!(reply["result"]["content"], "twox\nthree\n", "{reply}");
    // With line and limit, from the buffer too.
    let reply = read(&mut ed, 51, &file, Some(2), Some(1));
    assert_eq!(reply["result"]["content"], "three\n", "{reply}");
    // The disk was not touched.
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        "one\ntwo\nthree\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_read_of_a_file_not_open_comes_from_disk_with_line_and_limit() {
    let root = project("disk");
    std::fs::create_dir_all(root.join("src")).expect("dir");
    let file = root.join("src").join("b.rs");
    std::fs::write(&file, "a\nb\nc\nd\n").expect("write");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    let reply = read(&mut ed, 60, &file, None, None);
    assert_eq!(reply["result"]["content"], "a\nb\nc\nd\n", "{reply}");
    let reply = read(&mut ed, 61, &file, Some(2), Some(2));
    assert_eq!(reply["result"]["content"], "b\nc\n", "{reply}");
    let reply = read(&mut ed, 62, &file, Some(10), None);
    assert_eq!(reply["result"]["content"], "", "{reply}");
    let log = ed.agent_log_text().expect("the log");
    assert!(log.contains("[fs] read "), "{log}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn paths_outside_relative_excluded_or_missing_get_errors() {
    let root = project("errors");
    let secret = root.join(".env");
    std::fs::write(&secret, "TOKEN=hunter2\n").expect("write");
    let outside = std::env::temp_dir().join("rvim-fs-test-outside.txt");
    std::fs::write(&outside, "outside text\n").expect("write");
    let mut ed = editor(&root, None);
    connect(&mut ed);

    let reply = read(&mut ed, 70, &outside, None, None);
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(
        reply["error"]["message"],
        "path outside shareable directories"
    );
    let escape = root.join("..").join("rvim-fs-test-outside.txt");
    let reply = read(&mut ed, 71, &escape, None, None);
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    let reply = call(&mut ed, 72, "fs/read_text_file", json!({"path": "a.txt"}));
    assert_eq!(reply["error"]["message"], "path must be absolute");
    let reply = read(&mut ed, 73, &secret, None, None);
    assert_eq!(reply["error"]["message"], "path excluded from sharing");
    assert!(!reply.to_string().contains("hunter2"));
    let reply = read(&mut ed, 74, &root.join("missing.txt"), None, None);
    assert_eq!(reply["error"]["code"], -32002, "{reply}");
    // A write outside is refused and nothing is written.
    let target = std::env::temp_dir().join("rvim-fs-test-not-written.txt");
    let _ = std::fs::remove_file(&target);
    let reply = call(
        &mut ed,
        75,
        "fs/write_text_file",
        json!({"path": target.to_string_lossy(), "content": "x"}),
    );
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert!(!target.exists());
    let reply = call(
        &mut ed,
        76,
        "fs/write_text_file",
        json!({"path": secret.to_string_lossy(), "content": "TOKEN=x\n"}),
    );
    assert_eq!(reply["error"]["message"], "path excluded from sharing");
    assert_eq!(
        std::fs::read_to_string(&secret).expect("read"),
        "TOKEN=hunter2\n"
    );
    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_served_write_is_applied_once_tagged_and_undone_by_u() {
    let root = project("write");
    let file = root.join("a.txt");
    std::fs::write(&file, "a\nb\n").expect("write");
    let mut ed = editor(&root, Some(&file));
    ed.feed(":set filewatch<CR>");
    connect(&mut ed);
    assert_eq!(ed.undo_source().as_deref(), Some("user"));

    let content = "a\nB\nc\n";
    agent(
        &mut ed,
        json!({"jsonrpc": "2.0", "id": 80, "method": "fs/write_text_file",
               "params": {"sessionId": "s1", "path": file.to_string_lossy(), "content": content}}),
    );
    let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
    assert_eq!(jobs.len(), 1);
    // The watcher sees the write before the file thread reports it: dropped,
    // the served write applies it.
    watcher(&mut ed, content.as_bytes());
    assert_eq!(text(&ed), "a\nb");
    let replies = run_jobs(&mut ed, jobs);
    let reply = replies.iter().find(|v| v["id"] == 80).expect("a reply");
    assert_eq!(reply["result"], json!({}), "{reply}");
    assert_eq!(std::fs::read_to_string(&file).expect("read"), content);
    assert_eq!(text(&ed), "a\nB\nc");
    assert_eq!(ed.undo_source().as_deref(), Some("agent:claude"));
    assert!(!ed.buf().modified, "the buffer matches the disk");
    assert!(
        status(&ed).contains("(agent:claude): +2 -1 lines"),
        "{}",
        status(&ed)
    );
    // The watcher's later report of the same write is not applied again.
    watcher(&mut ed, content.as_bytes());
    assert_eq!(ed.undo_source().as_deref(), Some("agent:claude"));
    // One `u` undoes the agent's write.
    ed.handle_event(Event::Key(rvim::Key::Char('u')));
    assert_eq!(text(&ed), "a\nb");
    assert_eq!(ed.undo_source().as_deref(), Some("user"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_write_while_typing_waits_for_normal_mode() {
    let root = project("insert");
    let file = root.join("a.txt");
    std::fs::write(&file, "a\n").expect("write");
    let mut ed = editor(&root, Some(&file));
    connect(&mut ed);
    ed.handle_event(Event::Key(rvim::Key::Char('i')));
    let reply = call(
        &mut ed,
        90,
        "fs/write_text_file",
        json!({"path": file.to_string_lossy(), "content": "z\n"}),
    );
    assert_eq!(reply["result"], json!({}));
    // Insert mode is never interrupted.
    assert_eq!(text(&ed), "a");
    ed.handle_event(Event::Key(rvim::Key::Esc));
    assert_eq!(text(&ed), "z");
    assert_eq!(ed.undo_source().as_deref(), Some("agent:claude"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_write_to_a_modified_buffer_keeps_it_and_warns() {
    let root = project("modified");
    let file = root.join("a.txt");
    std::fs::write(&file, "a\n").expect("write");
    let mut ed = editor(&root, Some(&file));
    connect(&mut ed);
    ed.feed("Ax<Esc>");
    let reply = call(
        &mut ed,
        95,
        "fs/write_text_file",
        json!({"path": file.to_string_lossy(), "content": "z\n"}),
    );
    assert_eq!(reply["result"], json!({}));
    // Never a silent reload of a modified buffer (no opt-in here: W12).
    assert_eq!(text(&ed), "ax");
    assert!(status(&ed).contains("W12"), "{}", status(&ed));
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "z\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_write_to_a_file_not_open_creates_it() {
    let root = project("create");
    let file = root.join("new").join("c.txt");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    let reply = call(
        &mut ed,
        99,
        "fs/write_text_file",
        json!({"path": file.to_string_lossy(), "content": "new\n"}),
    );
    assert_eq!(reply["result"], json!({}), "{reply}");
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "new\n");
    assert_eq!(text(&ed), "");
    let _ = std::fs::remove_dir_all(&root);
}

// ---- sensitive writes wait for a key (2026-11-13) ---------------------------

/// A `fs/write_text_file` request with the file thread not run yet.
fn write_request(ed: &mut Editor, id: i64, path: &Path, content: &str) {
    agent(
        ed,
        json!({"jsonrpc": "2.0", "id": id, "method": "fs/write_text_file",
               "params": {"sessionId": "s1", "path": path.to_string_lossy(), "content": content}}),
    );
}

/// `rel` (with `/`) below `root`.
fn below(root: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(root.to_path_buf(), |p, part| p.join(part))
}

#[test]
fn a_sensitive_write_waits_and_nothing_is_written_until_y() {
    let root = project("sensitive-y");
    let hook = root.join(".git").join("hooks").join("pre-commit");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    write_request(&mut ed, 100, &hook, "#!/bin/sh\necho hi\n");
    let (jobs, rest) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty(), "no write is handed to the file thread");
    assert!(sent(&rest).iter().all(|v| v["id"] != 100));
    assert!(!hook.exists());
    assert!(ed.permission_prompt_up());
    assert!(
        status(&ed).contains(
            "claude-acp wants to write .git/hooks/pre-commit (sensitive). [y]es [n]o (1/1)"
        ),
        "{}",
        status(&ed)
    );
    assert!(status(&ed).contains("PERMISSION 1"), "{}", status(&ed));
    let log = ed.agent_log_text().expect("the log");
    assert!(log.contains("waiting for permission (sensitive)"), "{log}");
    // No grant keys, and no other key answers it.
    ed.feed("aNx:");
    let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty() && !hook.exists());
    ed.feed("y");
    let (jobs, rest) = fs_jobs(ed.take_agent_conn_commands());
    assert_eq!(jobs.len(), 1);
    assert!(sent(&rest).iter().all(|v| v["id"] != 100));
    let replies = run_jobs(&mut ed, jobs);
    let reply = replies.iter().find(|v| v["id"] == 100).expect("a reply");
    assert_eq!(reply["result"], json!({}), "{reply}");
    assert_eq!(
        std::fs::read_to_string(&hook).expect("read"),
        "#!/bin/sh\necho hi\n"
    );
    assert!(!ed.permission_queued());
    assert!(!status(&ed).contains("PERMISSION"), "{}", status(&ed));
    let log = ed.agent_log_text().expect("the log");
    assert!(
        log.contains("[permission] write .git/hooks/pre-commit: allowed"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn n_and_esc_refuse_a_sensitive_write_with_an_error() {
    for key in ["n", "<Esc>", "<C-c>"] {
        let root = project("sensitive-n");
        let file = root.join(".claude").join("settings.json");
        let mut ed = editor(&root, None);
        connect(&mut ed);
        write_request(&mut ed, 101, &file, "{\"secret-looking\": true}");
        assert!(ed.permission_prompt_up());
        ed.feed(key);
        let (jobs, rest) = fs_jobs(ed.take_agent_conn_commands());
        assert!(jobs.is_empty(), "{key}");
        let replies = sent(&rest);
        let reply = replies.iter().find(|v| v["id"] == 101).expect("a reply");
        assert_eq!(reply["error"]["code"], -32603, "{key}: {reply}");
        assert_eq!(reply["error"]["message"], "write refused by the user");
        assert!(!reply.to_string().contains("secret-looking"), "no contents");
        assert!(!file.exists(), "{key}");
        assert!(!ed.permission_queued(), "{key}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn a_normal_write_needs_no_prompt() {
    let root = project("not-sensitive");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    for (id, rel) in [
        (102, "src/a.rs"),
        (103, "docs/git.md"),
        (104, ".gitignore"),
        (105, ".github/ISSUE_TEMPLATE/bug.md"),
        (106, "notes/CLAUDE.md.txt"),
        (107, "config.toml"),
    ] {
        let file = below(&root, rel);
        let reply = call(
            &mut ed,
            id,
            "fs/write_text_file",
            json!({"path": file.to_string_lossy(), "content": "x\n"}),
        );
        assert_eq!(reply["result"], json!({}), "{rel}: {reply}");
        assert!(file.exists(), "{rel}");
        assert!(!ed.permission_queued(), "{rel}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_listed_kind_of_path_waits() {
    let root = project("sensitive-list");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    let paths = [
        ".git/hooks/pre-push",
        ".git/config",
        "sub/.git/hooks/commit-msg",
        ".vscode/tasks.json",
        ".idea/workspace.xml",
        ".zed/settings.json",
        ".claude/settings.json",
        ".codex/config.toml",
        ".gemini/settings.json",
        ".cursor/rules/a.mdc",
        ".mcp.json",
        "mcp.json",
        ".github/workflows/ci.yml",
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "pkg/CLAUDE.md",
        "rvim/config.toml",
        "agent-approvals.toml",
    ];
    for (i, rel) in paths.iter().enumerate() {
        let file = below(&root, rel);
        write_request(&mut ed, 200 + i as i64, &file, "x\n");
        let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
        assert!(jobs.is_empty(), "{rel} was not asked about");
        assert!(ed.permission_prompt_up(), "{rel}");
        assert!(!file.exists(), "{rel}");
        ed.feed("n");
        assert!(!ed.permission_queued(), "{rel}");
        ed.take_agent_conn_commands();
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(windows)]
#[test]
fn matching_is_case_insensitive_on_windows() {
    let root = project("sensitive-case");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    for (i, rel) in [
        ".GIT/hooks/pre-commit",
        ".Github/Workflows/ci.yml",
        "claude.md",
        "Agents.MD",
        ".VSCODE/settings.json",
        "MCP.JSON",
    ]
    .iter()
    .enumerate()
    {
        let file = below(&root, rel);
        write_request(&mut ed, 300 + i as i64, &file, "x\n");
        let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
        assert!(jobs.is_empty(), "{rel} was not asked about");
        assert!(ed.permission_prompt_up(), "{rel}");
        ed.feed("n");
        ed.take_agent_conn_commands();
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_sensitive_write_waits_for_normal_mode_and_cancel_answers_it() {
    let root = project("sensitive-cancel");
    let hook = root.join(".git").join("hooks").join("post-commit");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    ed.feed("A");
    write_request(&mut ed, 110, &hook, "x\n");
    // Insert mode: queued, and `y` is typed text.
    assert!(ed.permission_queued() && !ed.permission_prompt_up());
    ed.feed("y");
    assert_eq!(text(&ed), "y");
    let (jobs, rest) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty());
    assert!(sent(&rest).iter().all(|v| v["id"] != 110));
    assert!(status(&ed).contains("PERMISSION 1"), "{}", status(&ed));
    // `<Esc>` leaves Insert mode and the prompt is up; the next `<Esc>`
    // refuses it.
    ed.feed("<Esc>");
    assert!(ed.permission_prompt_up());
    ed.feed("<Esc>");
    let (_, rest) = fs_jobs(ed.take_agent_conn_commands());
    let replies = sent(&rest);
    let reply = replies.iter().find(|v| v["id"] == 110).expect("a reply");
    assert_eq!(reply["error"]["code"], -32603, "{reply}");
    // A command line can be typed while requests wait: `:AgentCancel`.
    ed.feed(":AgentCan");
    write_request(&mut ed, 111, &hook, "x\n");
    write_request(&mut ed, 112, &hook, "x\n");
    assert!(ed.permission_queued() && !ed.permission_prompt_up());
    assert_eq!(ed.cmdline(), "AgentCan");
    ed.feed("cel<CR>");
    let (jobs, rest) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty());
    let replies = sent(&rest);
    for id in [111, 112] {
        let reply = replies.iter().find(|v| v["id"] == id).expect("a reply");
        assert_eq!(reply["error"]["code"], -32800, "{reply}");
        assert!(reply["result"].is_null());
    }
    assert!(!hook.exists());
    assert!(!ed.permission_queued());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_disconnect_drops_a_sensitive_write_and_never_writes_it() {
    let root = project("sensitive-disconnect");
    let file = root.join(".vscode").join("tasks.json");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    write_request(&mut ed, 120, &file, "x\n");
    assert!(ed.permission_prompt_up());
    ed.agent_disconnect();
    assert!(!ed.permission_queued());
    let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty());
    // A `y` now is Vim's `y`: nothing is written.
    ed.feed("y");
    let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty() && !file.exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_lost_connection_drops_a_sensitive_write() {
    let root = project("sensitive-lost");
    let file = root.join(".idea").join("a.xml");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    write_request(&mut ed, 130, &file, "x\n");
    assert!(ed.permission_prompt_up());
    ed.handle_event(Event::AgentConn {
        generation: 1,
        event: AgentEvent::Ended(Some(1)),
    });
    assert!(!ed.permission_queued());
    ed.feed("y");
    let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
    assert!(jobs.is_empty() && !file.exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rvims_own_config_outside_the_project_is_refused_not_asked() {
    let root = project("sensitive-config");
    let outside = project("sensitive-config-elsewhere");
    let mut ed = editor(&root, None);
    connect(&mut ed);
    for (i, rel) in ["rvim/config.toml", "agent-approvals.toml"]
        .iter()
        .enumerate()
    {
        let file = below(&outside, rel);
        let reply = call(
            &mut ed,
            140 + i as i64,
            "fs/write_text_file",
            json!({"path": file.to_string_lossy(), "content": "x\n"}),
        );
        assert_eq!(reply["error"]["code"], -32602, "{rel}: {reply}");
        assert_eq!(
            reply["error"]["message"],
            "rvim's own configuration is not served"
        );
        assert!(!file.exists());
        assert!(!ed.permission_queued());
    }
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn a_sensitive_write_to_an_open_unmodified_file_is_applied_after_y() {
    let root = project("sensitive-open");
    let file = root.join("CLAUDE.md");
    std::fs::write(&file, "old\n").expect("write");
    let mut ed = editor(&root, Some(&file));
    connect(&mut ed);
    write_request(&mut ed, 150, &file, "new\n");
    assert_eq!(text(&ed), "old");
    ed.feed("y");
    let (jobs, _) = fs_jobs(ed.take_agent_conn_commands());
    let replies = run_jobs(&mut ed, jobs);
    assert_eq!(
        replies.iter().find(|v| v["id"] == 150).expect("a reply")["result"],
        json!({})
    );
    assert_eq!(text(&ed), "new");
    assert_eq!(ed.undo_source().as_deref(), Some("agent:claude"));
    let _ = std::fs::remove_dir_all(&root);
}
