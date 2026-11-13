// SPDX-License-Identifier: Apache-2.0
//! The agent's file calls, `fs/read_text_file` and `fs/write_text_file`
//! (PLAN.md, "Agent experience", sections 3, 4 and 10; milestone 7 step 6b;
//! "Served file calls, step 6b").
//!
//! Three parts:
//!
//! - The path check, pure: an absolute path, normalized without touching the
//!   disk, inside a shareable directory (the project root), and with no
//!   excluded name (`.env*`, `*.pem`, `*.key`, `id_rsa*`) in it
//!   ([`check_request`]). The editor runs it when the request arrives.
//! - The worker side ([`run`]): the same check again after symlinks are
//!   resolved ([`check_resolved`], which needs the disk), then the read
//!   (capped at [`MAX_SERVE_BYTES`]) or the write. It runs on the connection
//!   manager's file thread (`agent_conn.rs`), never on the editor's.
//! - The editor side: a read of a file open in a document is answered with
//!   the document's text, unsaved changes included (the file-system page says
//!   the client may do so); other reads with the disk's text. A write is
//!   written by the worker and then applied once to the open document as one
//!   undo step tagged `agent:<id>`, with the disk state it wrote recorded, so
//!   the watcher's report of the same write is not applied again.
//!
//! No new prompt is shown for a write: PLAN section 4 puts the agent's own
//! `session/request_permission` before its writes (step 6a owns those
//! prompts), and the path rules here are the client's own check, which the
//! file-system page requires whatever the agent was granted.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use crate::Editor;
use crate::acp::{INVALID_PARAMS, Id, InEvent, RESOURCE_NOT_FOUND, RpcError};
use crate::agent_conn::ConnCmd;
use crate::buffer::Buffer;
use crate::document::DocId;
use crate::reload::{DiskState, fnv1a};
use crate::undo::UndoSource;

/// Largest file served by `fs/read_text_file`, as the watcher's read cap.
pub const MAX_SERVE_BYTES: usize = crate::watch::MAX_WATCH_BYTES;
/// JSON-RPC "Internal error" (schema `ErrorCode`): an IO failure.
pub const INTERNAL_ERROR: i64 = -32603;

/// Why a path is refused. The messages name the rule, never file contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// Not an absolute path (the schema requires one).
    NotAbsolute,
    /// Outside every shareable directory, or `..` above the file system root,
    /// or a symlink whose target is outside.
    Outside,
    /// An excluded name (`.env*`, `*.pem`, `*.key`, `id_rsa*`).
    Excluded,
}

impl PathError {
    pub fn message(self) -> &'static str {
        match self {
            PathError::NotAbsolute => "path must be absolute",
            PathError::Outside => "path outside shareable directories",
            PathError::Excluded => "path excluded from sharing",
        }
    }

    fn error(self) -> FsError {
        FsError::new(INVALID_PARAMS, self.message())
    }
}

/// An error for the agent: a JSON-RPC code and a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    pub code: i64,
    pub message: String,
}

impl FsError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn rpc(&self) -> RpcError {
        RpcError::new(self.code, self.message.clone())
    }
}

/// `path` with `.` dropped and `..` applied, without looking at the disk.
/// None when `..` goes above the root.
pub fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                out.push(c.as_os_str())
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() || out.as_os_str().is_empty() {
                    return None;
                }
            }
        }
    }
    Some(out)
}

/// The part of `path` below `root`, when `path` is inside it. Windows paths
/// compare without case, as the file system does.
fn below<'a>(path: &'a Path, root: &Path) -> Option<Vec<Component<'a>>> {
    let mut p = path.components();
    for r in root.components() {
        let c = p.next()?;
        let same = if cfg!(windows) {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&r.as_os_str().to_string_lossy())
        } else {
            c == r
        };
        if !same {
            return None;
        }
    }
    Some(p.collect())
}

/// Whether a file or directory name is excluded from sharing everywhere
/// (PLAN.md, "Prompting"): `.env*`, `*.pem`, `*.key`, `id_rsa*`. Case is
/// ignored, so that `.ENV` is excluded on every system.
pub fn is_excluded_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with(".env") || n.ends_with(".pem") || n.ends_with(".key") || n.starts_with("id_rsa")
}

/// Checks `path` (normalized and absolute) against `roots` (normalized and
/// absolute): inside one of them, and no excluded name below it.
fn check_inside(path: &Path, roots: &[PathBuf]) -> Result<(), PathError> {
    let rest = roots
        .iter()
        .find_map(|r| below(path, r))
        .ok_or(PathError::Outside)?;
    let excluded = rest
        .iter()
        .any(|c| is_excluded_name(&c.as_os_str().to_string_lossy()));
    if excluded {
        Err(PathError::Excluded)
    } else {
        Ok(())
    }
}

/// The check made when a request arrives, without the disk: `path` is
/// absolute, stays inside a shareable directory after `..` is applied, and
/// has no excluded name. Returns the normalized path.
pub fn check_request(path: &str, roots: &[PathBuf]) -> Result<PathBuf, PathError> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(PathError::NotAbsolute);
    }
    let n = normalize(p).ok_or(PathError::Outside)?;
    let roots: Vec<PathBuf> = roots.iter().filter_map(|r| normalize(r)).collect();
    check_inside(&n, &roots)?;
    Ok(n)
}

/// The check again with symlinks resolved (disk access: the worker runs it).
/// The deepest part of `path` that exists is canonicalized and the rest is
/// appended, so a file not yet written is checked by its directory. A
/// symlink that cannot be resolved (dangling) is refused, because a write
/// would follow it. Returns the resolved path.
pub fn check_resolved(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, PathError> {
    let roots: Vec<PathBuf> = roots
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok())
        .collect();
    let mut existing = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let base = loop {
        match std::fs::canonicalize(&existing) {
            Ok(c) => break c,
            Err(_) => {
                if std::fs::symlink_metadata(&existing).is_ok() {
                    // It exists but does not resolve: a dangling link.
                    return Err(PathError::Outside);
                }
                let name = existing
                    .file_name()
                    .ok_or(PathError::Outside)?
                    .to_os_string();
                rest.push(name);
                if !existing.pop() {
                    return Err(PathError::Outside);
                }
            }
        }
    };
    let mut full = base;
    for name in rest.into_iter().rev() {
        full.push(name);
    }
    check_inside(&full, &roots)?;
    Ok(full)
}

/// The lines `line` (1-based; 0 is taken as 1) to `line + limit - 1` of
/// `text`, line ends kept. No `line` starts at the first line; no `limit`
/// goes to the end. A start past the end gives the empty string.
pub fn slice_lines(text: &str, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return text.to_owned();
    }
    let skip = line.unwrap_or(1).max(1) as usize - 1;
    let lines = text.split_inclusive('\n').skip(skip);
    match limit {
        Some(n) => lines.take(n as usize).collect(),
        None => lines.collect(),
    }
}

/// Work for the file thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsJob {
    /// Read `path` (already checked by [`check_request`]).
    Read {
        request: Id,
        path: PathBuf,
        roots: Vec<PathBuf>,
        line: Option<u32>,
        limit: Option<u32>,
    },
    /// Write `content` to `path` (already checked), creating the file and
    /// its directories.
    Write {
        request: Id,
        path: PathBuf,
        roots: Vec<PathBuf>,
        content: String,
    },
}

/// The disk state after a served write (as `reload::DiskState`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenState {
    pub mtime: Option<SystemTime>,
    pub size: usize,
    pub hash: u64,
}

/// The file thread's answer to a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsDone {
    Read {
        request: Id,
        path: PathBuf,
        line: Option<u32>,
        limit: Option<u32>,
        /// The check with symlinks resolved. When it fails, the open
        /// document is not served either.
        checked: Result<(), FsError>,
        /// The file's bytes (not read when the check failed).
        bytes: Result<Vec<u8>, FsError>,
    },
    Written {
        request: Id,
        path: PathBuf,
        result: Result<WrittenState, FsError>,
    },
}

fn io_error(e: &std::io::Error) -> FsError {
    if e.kind() == std::io::ErrorKind::NotFound {
        FsError::new(RESOURCE_NOT_FOUND, "file not found")
    } else {
        FsError::new(INTERNAL_ERROR, format!("{e}"))
    }
}

fn read_capped(path: &Path) -> Result<Vec<u8>, FsError> {
    let file = std::fs::File::open(path).map_err(|e| io_error(&e))?;
    let meta = file.metadata().map_err(|e| io_error(&e))?;
    if meta.is_dir() {
        return Err(FsError::new(INVALID_PARAMS, "path is a directory"));
    }
    let too_large = || FsError::new(INVALID_PARAMS, "file larger than 64 MiB; not served");
    if meta.len() > MAX_SERVE_BYTES as u64 {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SERVE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(&e))?;
    if bytes.len() > MAX_SERVE_BYTES {
        return Err(too_large());
    }
    Ok(bytes)
}

/// Runs one job (disk access). Called on the file thread, and by tests.
pub fn run(job: FsJob) -> FsDone {
    match job {
        FsJob::Read {
            request,
            path,
            roots,
            line,
            limit,
        } => {
            let resolved = check_resolved(&path, &roots).map_err(PathError::error);
            let (checked, bytes) = match resolved {
                Ok(real) => (Ok(()), read_capped(&real)),
                Err(e) => (Err(e.clone()), Err(e)),
            };
            FsDone::Read {
                request,
                path,
                line,
                limit,
                checked,
                bytes,
            }
        }
        FsJob::Write {
            request,
            path,
            roots,
            content,
        } => {
            let result = check_resolved(&path, &roots)
                .map_err(PathError::error)
                .and_then(|real| write(&real, content.as_bytes()));
            FsDone::Written {
                request,
                path,
                result,
            }
        }
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<WrittenState, FsError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io_error(&e))?;
    }
    std::fs::write(path, bytes).map_err(|e| io_error(&e))?;
    let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
    Ok(WrittenState {
        mtime,
        size: bytes.len(),
        hash: fnv1a(bytes),
    })
}

/// A served write on its way to the disk.
#[derive(Debug)]
struct Intent {
    request: Id,
    generation: u64,
    path: PathBuf,
    hash: u64,
    content: String,
    agent: &'static str,
}

/// A served write that is on disk and waits for Normal mode to be applied to
/// its open document.
#[derive(Debug)]
struct Apply {
    path: PathBuf,
    state: DiskState,
    bytes: Vec<u8>,
    agent: &'static str,
}

/// The editor's state for served file calls.
#[derive(Debug, Default)]
pub(crate) struct FsState {
    intents: Vec<Intent>,
    applies: Vec<Apply>,
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

impl Editor {
    /// The shareable directories: the project root (PLAN.md, "Failures and
    /// privacy"; the `[agent] shareable` config key is not supported yet).
    fn shareable_roots(&self) -> Vec<PathBuf> {
        self.agent.project.iter().cloned().collect()
    }

    /// `path` as the agent would name it: the shortest form for the log,
    /// relative to the project root when inside it.
    fn fs_display(&self, path: &Path) -> String {
        self.agent
            .project
            .as_ref()
            .and_then(|r| path.strip_prefix(r).ok())
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// The absolute, normalized path of a document's file.
    fn doc_abs_path(&self, doc: DocId) -> Option<PathBuf> {
        let p = self.document(doc).buf.path.as_ref()?;
        let abs = if p.is_absolute() {
            p.clone()
        } else {
            self.agent.project.as_ref()?.join(p)
        };
        normalize(&abs)
    }

    /// The document whose file is `path` (normalized, absolute).
    fn doc_for_path(&self, path: &Path) -> Option<DocId> {
        self.document_ids()
            .find(|&d| self.doc_abs_path(d).is_some_and(|p| same_path(&p, path)))
    }

    /// Gives the connection the answer to a served call.
    fn fs_answer(&mut self, generation: u64, event: InEvent) {
        let Some(link) = self
            .agent
            .link
            .as_mut()
            .filter(|l| l.generation == generation)
        else {
            return;
        };
        let actions = link.conn.on_event(event);
        self.agent_apply(generation, actions, std::time::Instant::now());
    }

    fn agent_id_of(&self, generation: u64) -> Option<&'static str> {
        self.agent
            .link
            .as_ref()
            .filter(|l| l.generation == generation)
            .map(|l| l.id)
    }

    /// `fs/read_text_file` (`OutAction::ServeRead`): the request's path is
    /// checked, then the worker checks it with symlinks resolved and reads
    /// the disk; the answer is made when it returns ([`Self::agent_fs_done`]).
    pub(crate) fn agent_serve_read(
        &mut self,
        generation: u64,
        request: Id,
        path: String,
        line: Option<u32>,
        limit: Option<u32>,
    ) {
        let roots = self.shareable_roots();
        match check_request(&path, &roots) {
            Err(e) => {
                self.agent_log_line(&format!("[fs] read {path}: {}", e.message()));
                let result = Err(e.error().rpc());
                self.fs_answer(generation, InEvent::FileReadAnswer { request, result });
            }
            Ok(path) => {
                let job = FsJob::Read {
                    request,
                    path,
                    roots,
                    line,
                    limit,
                };
                self.agent.outbox.push(ConnCmd::Fs { generation, job });
            }
        }
    }

    /// `fs/write_text_file` (`OutAction::ServeWrite`): the path is checked,
    /// the write intent recorded, and the worker writes the file.
    pub(crate) fn agent_serve_write(
        &mut self,
        generation: u64,
        request: Id,
        path: String,
        content: String,
    ) {
        let roots = self.shareable_roots();
        let Some(agent) = self.agent_id_of(generation) else {
            return;
        };
        match check_request(&path, &roots) {
            Err(e) => {
                self.agent_log_line(&format!("[fs] write {path}: {}", e.message()));
                let result = Err(e.error().rpc());
                self.fs_answer(generation, InEvent::FileWriteAnswer { request, result });
            }
            Ok(path) => {
                self.agent.fs.intents.push(Intent {
                    request: request.clone(),
                    generation,
                    path: path.clone(),
                    hash: fnv1a(content.as_bytes()),
                    content: content.clone(),
                    agent,
                });
                let job = FsJob::Write {
                    request,
                    path,
                    roots,
                    content,
                };
                self.agent.outbox.push(ConnCmd::Fs { generation, job });
            }
        }
    }

    /// The file thread finished a job.
    pub(crate) fn agent_fs_done(&mut self, generation: u64, done: FsDone) {
        match done {
            FsDone::Read {
                request,
                path,
                line,
                limit,
                checked,
                bytes,
            } => {
                let shown = self.fs_display(&path);
                let result = match checked {
                    Err(e) => Err(e),
                    Ok(()) => match self.doc_for_path(&path) {
                        // The open document's text, unsaved changes
                        // included, as `:w` would write it.
                        Some(doc) => {
                            Ok(String::from_utf8_lossy(&self.document(doc).buf.to_bytes())
                                .into_owned())
                        }
                        None => bytes.and_then(|b| {
                            String::from_utf8(b)
                                .map_err(|_| FsError::new(INVALID_PARAMS, "not a UTF-8 text file"))
                        }),
                    },
                };
                let result = result.map(|text| slice_lines(&text, line, limit));
                match &result {
                    Ok(_) => self.agent_log_line(&format!("[fs] read {shown}")),
                    Err(e) => self.agent_log_line(&format!("[fs] read {shown}: {}", e.message)),
                }
                let result = result.map_err(|e| e.rpc());
                self.fs_answer(generation, InEvent::FileReadAnswer { request, result });
            }
            FsDone::Written {
                request,
                path,
                result,
            } => {
                let pos = self
                    .agent
                    .fs
                    .intents
                    .iter()
                    .position(|i| i.request == request && i.generation == generation);
                let intent = pos.map(|p| self.agent.fs.intents.remove(p));
                let shown = self.fs_display(&path);
                match result {
                    Ok(state) => {
                        self.agent_log_line(&format!("[fs] write {shown}"));
                        if let Some(intent) = intent {
                            self.agent.fs.applies.push(Apply {
                                path: intent.path,
                                state: DiskState {
                                    mtime: state.mtime,
                                    size: state.size,
                                    hash: state.hash,
                                },
                                bytes: intent.content.into_bytes(),
                                agent: intent.agent,
                            });
                        }
                        let result = Ok(());
                        self.fs_answer(generation, InEvent::FileWriteAnswer { request, result });
                        self.apply_served_writes();
                    }
                    Err(e) => {
                        self.agent_log_line(&format!("[fs] write {shown}: {}", e.message));
                        self.error(format!("write failed: {}", e.message));
                        let result = Err(e.rpc());
                        self.fs_answer(generation, InEvent::FileWriteAnswer { request, result });
                    }
                }
            }
        }
    }

    /// Whether a watcher report for `doc` with contents `hash` is a served
    /// write that is being written or waits to be applied: it is applied by
    /// the served write itself, once, so the watcher's report is dropped.
    pub(crate) fn agent_fs_echo(&self, doc: DocId, hash: u64) -> bool {
        let fs = &self.agent.fs;
        if fs.intents.is_empty() && fs.applies.is_empty() {
            return false;
        }
        let Some(path) = self.doc_abs_path(doc) else {
            return false;
        };
        fs.intents
            .iter()
            .any(|i| i.hash == hash && same_path(&i.path, &path))
            || fs
                .applies
                .iter()
                .any(|a| a.state.hash == hash && same_path(&a.path, &path))
    }

    /// Applies the served writes that are on disk to their open documents,
    /// when the editor is ready for a change from outside (Normal mode, as
    /// for the watcher). An unmodified document takes the text as one undo
    /// step tagged `agent:<id>`, whatever 'autoread' is: the write went
    /// through the editor. A modified one gets the rules of a file changed
    /// on disk (the conflict prompt with the opt-in, W12 without).
    pub(crate) fn apply_served_writes(&mut self) {
        if self.agent.fs.applies.is_empty() || !self.ready_for_change() {
            return;
        }
        for a in std::mem::take(&mut self.agent.fs.applies) {
            let Some(doc) = self.doc_for_path(&a.path) else {
                continue;
            };
            let Some(path) = self.document(doc).buf.path.clone() else {
                continue;
            };
            if self.document(doc).buf.modified {
                if let Some(e) = self.check_disk(doc, Some((a.state, &a.bytes)), false) {
                    self.error(e);
                }
                continue;
            }
            let (new, _) = Buffer::from_file_bytes(&path, Some(&a.bytes));
            let lines = new.lines();
            let source = UndoSource::Agent(a.agent);
            let change = self.apply_external_change(doc, new, source);
            let d = self.document_mut(doc);
            d.baseline = Some(lines);
            d.disk = Some(a.state);
            d.stale = None;
            d.warned = None;
            let name = path.display();
            let text = match change {
                None => format!("\"{name}\" written ({source}), no change"),
                Some(c) => format!(
                    "\"{name}\" changed on disk ({source}): +{} -{} lines",
                    c.added, c.removed
                ),
            };
            self.info(text);
        }
    }

    /// The source tag of the current document's newest undo step (`user`,
    /// `external`, `agent:<id>`), for tests and later `:AgentChanges`.
    pub fn undo_source(&self) -> Option<String> {
        let undo = &self.doc().undo;
        undo.source(undo.seq()).map(|s| s.to_string())
    }

    /// The current window's document and its version, as the watcher's
    /// events carry them; for tests that send `Event::FileChanged`.
    pub fn current_doc_version(&self) -> (DocId, u64) {
        (self.win().doc, self.doc().version())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    const ROOT: &str = "C:\\proj";
    #[cfg(not(windows))]
    const ROOT: &str = "/proj";

    fn p(rel: &str) -> String {
        let sep = std::path::MAIN_SEPARATOR;
        format!("{ROOT}{sep}{}", rel.replace('/', &sep.to_string()))
    }

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from(ROOT)]
    }

    #[test]
    fn a_path_inside_the_root_is_accepted() {
        assert_eq!(
            check_request(&p("src/a.rs"), &roots()),
            Ok(PathBuf::from(p("src/a.rs")))
        );
        assert_eq!(
            check_request(&p("./src/../b.rs"), &roots()),
            Ok(PathBuf::from(p("b.rs")))
        );
    }

    #[test]
    fn a_relative_path_is_refused() {
        assert_eq!(
            check_request("src/a.rs", &roots()),
            Err(PathError::NotAbsolute)
        );
    }

    #[test]
    fn a_path_outside_or_escaping_with_dotdot_is_refused() {
        #[cfg(windows)]
        let outside = "C:\\etc\\passwd";
        #[cfg(not(windows))]
        let outside = "/etc/passwd";
        assert_eq!(check_request(outside, &roots()), Err(PathError::Outside));
        assert_eq!(
            check_request(&p("../other/a.rs"), &roots()),
            Err(PathError::Outside)
        );
        assert_eq!(
            check_request(&p("src/../../proj2/a.rs"), &roots()),
            Err(PathError::Outside)
        );
        // A sibling whose name starts with the root's is not inside it.
        assert_eq!(
            check_request(
                &format!("{ROOT}x{}a.rs", std::path::MAIN_SEPARATOR),
                &roots()
            ),
            Err(PathError::Outside)
        );
        // `..` above the file system root.
        #[cfg(not(windows))]
        assert_eq!(check_request("/../../a", &roots()), Err(PathError::Outside));
    }

    #[test]
    fn excluded_names_are_refused_inside_the_root() {
        for rel in [
            ".env",
            ".env.local",
            "config/.ENV",
            "certs/server.pem",
            "tls/private.key",
            ".ssh/id_rsa",
            "id_rsa.pub",
            ".envrc/x.txt",
        ] {
            assert_eq!(
                check_request(&p(rel), &roots()),
                Err(PathError::Excluded),
                "{rel}"
            );
        }
        for rel in ["environment.rs", "keys.rs", "src/key_map.rs", "pem.txt"] {
            assert!(check_request(&p(rel), &roots()).is_ok(), "{rel}");
        }
    }

    #[test]
    fn root_names_above_the_project_do_not_count_as_excluded() {
        #[cfg(windows)]
        let root = PathBuf::from("C:\\home\\.envs\\proj");
        #[cfg(not(windows))]
        let root = PathBuf::from("/home/.envs/proj");
        let file = root.join("a.rs");
        assert!(check_request(&file.to_string_lossy(), &[root]).is_ok());
    }

    #[test]
    fn line_and_limit_are_one_based_line_counts() {
        let text = "one\ntwo\nthree\nfour";
        assert_eq!(slice_lines(text, None, None), text);
        assert_eq!(slice_lines(text, Some(2), None), "two\nthree\nfour");
        assert_eq!(slice_lines(text, Some(2), Some(2)), "two\nthree\n");
        assert_eq!(slice_lines(text, None, Some(1)), "one\n");
        assert_eq!(slice_lines(text, Some(0), Some(1)), "one\n");
        assert_eq!(slice_lines(text, Some(9), Some(1)), "");
        assert_eq!(slice_lines(text, Some(1), Some(0)), "");
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rvim-agent-fs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn the_resolved_check_takes_files_not_yet_written() {
        let root = temp_root("resolved");
        let file = root.join("new").join("deep.rs");
        let got = check_resolved(&file, std::slice::from_ref(&root)).expect("inside");
        assert!(got.ends_with(Path::new("new").join("deep.rs")), "{got:?}");
        let outside = std::env::temp_dir().join("rvim-agent-fs-elsewhere.rs");
        assert_eq!(
            check_resolved(&outside, std::slice::from_ref(&root)),
            Err(PathError::Outside)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_pointing_outside_is_refused() {
        let root = temp_root("symlink");
        let outside = temp_root("symlink-target");
        std::fs::write(outside.join("secret.txt"), "s").expect("write");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("symlink");
        std::os::unix::fs::symlink(outside.join("gone"), root.join("dangling")).expect("symlink");
        let roots = vec![root.clone()];
        let through = root.join("link").join("secret.txt");
        // The lexical check passes; the resolved one does not.
        assert!(check_request(&through.to_string_lossy(), &roots).is_ok());
        assert_eq!(check_resolved(&through, &roots), Err(PathError::Outside));
        assert_eq!(
            check_resolved(&root.join("dangling"), &roots),
            Err(PathError::Outside)
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// Windows symlinks need a privilege (or developer mode) that a test run
    /// may not have; the test runs when creating one works.
    #[cfg(windows)]
    #[test]
    fn a_symlink_pointing_outside_is_refused() {
        let root = temp_root("symlink");
        let outside = temp_root("symlink-target");
        std::fs::write(outside.join("secret.txt"), "s").expect("write");
        if std::os::windows::fs::symlink_dir(&outside, root.join("link")).is_err() {
            eprintln!("skipped: cannot create a symlink here");
            let _ = std::fs::remove_dir_all(&root);
            let _ = std::fs::remove_dir_all(&outside);
            return;
        }
        let roots = vec![root.clone()];
        let through = root.join("link").join("secret.txt");
        assert_eq!(check_resolved(&through, &roots), Err(PathError::Outside));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn the_worker_reads_writes_and_reports_missing_files() {
        let root = temp_root("worker");
        let roots = vec![root.clone()];
        let file = root.join("sub").join("a.txt");
        let done = run(FsJob::Write {
            request: Id::Num(1),
            path: file.clone(),
            roots: roots.clone(),
            content: "hi\n".to_owned(),
        });
        let FsDone::Written { result, .. } = done else {
            panic!("expected Written");
        };
        let state = result.expect("written");
        assert_eq!(state.hash, fnv1a(b"hi\n"));
        assert_eq!(std::fs::read(&file).expect("read"), b"hi\n");
        let done = run(FsJob::Read {
            request: Id::Num(2),
            path: root.join("missing.txt"),
            roots,
            line: None,
            limit: None,
        });
        let FsDone::Read { checked, bytes, .. } = done else {
            panic!("expected Read");
        };
        assert_eq!(checked, Ok(()));
        assert_eq!(bytes.map_err(|e| e.code), Err(RESOURCE_NOT_FOUND));
        let _ = std::fs::remove_dir_all(&root);
    }
}
