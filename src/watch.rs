// SPDX-License-Identifier: Apache-2.0
//! The file watcher (PLAN.md, "Files changed under rvim" and "Agent
//! experience: how it fits together", milestone 7 step 2b).
//!
//! A worker thread polls the files of the open documents every
//! [`POLL_INTERVAL`]. It looks at each file's modification time and size; when
//! they differ from the last look, it reads the file and hashes it. A file
//! whose contents hash the same as the state the buffer last synced with is an
//! echo of our own write, or a touch, and is not reported. Anything else is
//! sent to the editor as an [`Event`]; the editor decides what it means
//! (`Editor::check_disk`, the same decision as `:checktime`).
//!
//! The core is pure: [`poll`] works on a [`WatchSet`] and a [`FileSystem`],
//! so tests use a fake file system. [`run`] is the loop of the thread, and
//! [`spawn`] starts it with the real file system.
//!
//! The thread is always started by `main.rs`, but it does no work until the
//! editor sends a [`WatchCmd::Set`]. The editor sends one only when the option
//! `filewatch` is on (see `Editor::take_watch_commands`), so with the option
//! off the thread blocks on its command channel and never touches the disk.

use crate::document::DocId;
use crate::event::Event;
use crate::reload::DiskState;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

/// How often the worker looks at the watched files.
pub const POLL_INTERVAL: Duration = Duration::from_millis(750);

/// The largest file the worker reads. A changed file above this is not read:
/// the editor is told it is too large, and the user loads it with `:e!`.
pub const MAX_WATCH_BYTES: usize = 64 << 20;

/// What the file system says about a path: the modification time and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub mtime: Option<SystemTime>,
    pub size: usize,
}

/// The file system the watcher reads. The real one is [`RealFs`]; tests use
/// a fake.
pub trait FileSystem {
    /// `Ok(None)` when the path does not exist. Any other failure is an error,
    /// so an unreadable file is not taken for a deleted one.
    fn stat(&self, path: &Path) -> io::Result<Option<Stat>>;
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
}

/// The real file system, with `std::fs`.
pub struct RealFs;

impl FileSystem for RealFs {
    fn stat(&self, path: &Path) -> io::Result<Option<Stat>> {
        match std::fs::metadata(path) {
            Ok(m) => Ok(Some(Stat {
                mtime: m.modified().ok(),
                size: m.len() as usize,
            })),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
}

/// One file the editor wants watched: the document, its path, the state the
/// buffer last synced with (None when there is no file on disk yet), and the
/// document version at the time of the command. The version is copied into
/// the events, so the editor can drop an event for text it has changed since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchTarget {
    pub(crate) doc: DocId,
    pub(crate) path: PathBuf,
    pub(crate) synced: Option<DiskState>,
    pub(crate) version: u64,
}

/// A command from the editor to the watcher thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchCmd {
    /// The complete set of watched files, replacing the previous one. An
    /// empty set stops all polling.
    Set(Vec<WatchTarget>),
    /// Look at this document's file again on the next poll, even if it has
    /// not changed since the last look. Sent when an event was dropped
    /// because the document had changed, so the change is reported again.
    Resync(DocId),
}

/// What the last look at a file found. A look that finds the same thing again
/// reports nothing, so each change is reported once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seen {
    stat: Option<Stat>,
    /// The read or stat failed with this kind; `stat` is what was stat'ed.
    error: Option<ErrorKind>,
}

impl Seen {
    /// The look that the synced state implies: the file is as the editor
    /// last read or wrote it.
    fn of_synced(synced: &Option<DiskState>) -> Self {
        Seen {
            stat: synced.as_ref().map(|d| Stat {
                mtime: d.mtime,
                size: d.size,
            }),
            error: None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    target: WatchTarget,
    /// `None` after a resync: the next poll reads the file.
    seen: Option<Seen>,
}

/// The files being watched, with what the last poll saw of each.
#[derive(Debug, Default)]
pub(crate) struct WatchSet {
    entries: Vec<Entry>,
}

impl WatchSet {
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replaces the set. An entry whose synced state is unchanged keeps what
    /// the last poll saw. One whose synced state changed (the editor read or
    /// wrote the file) starts from the synced state: the file is taken to be
    /// as the editor left it.
    pub(crate) fn set(&mut self, targets: Vec<WatchTarget>) {
        let old = std::mem::take(&mut self.entries);
        self.entries = targets
            .into_iter()
            .map(|t| {
                let prev = old
                    .iter()
                    .find(|e| e.target.doc == t.doc && e.target.path == t.path);
                let seen = match prev {
                    Some(p) if p.target.synced == t.synced => p.seen,
                    _ => Some(Seen::of_synced(&t.synced)),
                };
                Entry { target: t, seen }
            })
            .collect();
    }

    /// Forgets what the last poll saw of `doc`'s file, so the next poll reads
    /// it again.
    pub(crate) fn resync(&mut self, doc: DocId) {
        for e in self.entries.iter_mut().filter(|e| e.target.doc == doc) {
            e.seen = None;
        }
    }
}

/// What a poll found for one file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Observation {
    /// The file's contents differ from the synced state. Also a file that
    /// appeared when the buffer had none on disk.
    Changed {
        doc: DocId,
        version: u64,
        state: DiskState,
        bytes: Vec<u8>,
    },
    /// The file that was on disk is gone.
    Deleted { doc: DocId, version: u64 },
    /// The file could not be stat'ed or read. Reported once per error.
    Unreadable { doc: DocId, message: String },
    /// The file changed but is over `MAX_WATCH_BYTES`, so it was not read.
    TooLarge {
        doc: DocId,
        version: u64,
        mtime: Option<SystemTime>,
        size: usize,
    },
}

/// Looks at every watched file once and returns what changed since the
/// previous poll. A file whose modification time and size are unchanged since
/// the last look is not read. A file that changed but hashes the same as its
/// synced state (touched, or our own write) is not reported.
pub(crate) fn poll(set: &mut WatchSet, fs: &dyn FileSystem) -> Vec<Observation> {
    set.entries.iter_mut().filter_map(|e| look(e, fs)).collect()
}

fn look(e: &mut Entry, fs: &dyn FileSystem) -> Option<Observation> {
    let doc = e.target.doc;
    let version = e.target.version;
    let stat = match fs.stat(&e.target.path) {
        Ok(stat) => stat,
        Err(err) => {
            let seen = Seen {
                stat: None,
                error: Some(err.kind()),
            };
            if e.seen == Some(seen) {
                return None;
            }
            e.seen = Some(seen);
            return Some(Observation::Unreadable {
                doc,
                message: err.to_string(),
            });
        }
    };
    if e.seen == Some(Seen { stat, error: None }) {
        return None;
    }
    let Some(st) = stat else {
        // Gone since the last look. Only a file the buffer had is deleted.
        e.seen = Some(Seen {
            stat: None,
            error: None,
        });
        return e
            .target
            .synced
            .as_ref()
            .map(|_| Observation::Deleted { doc, version });
    };
    if st.size > MAX_WATCH_BYTES {
        // Not read. Recorded like a read, so the same size is not reported again.
        e.seen = Some(Seen {
            stat: Some(st),
            error: None,
        });
        return Some(Observation::TooLarge {
            doc,
            version,
            mtime: st.mtime,
            size: st.size,
        });
    }
    match fs.read(&e.target.path) {
        Err(err) => {
            let seen = Seen {
                stat: Some(st),
                error: Some(err.kind()),
            };
            if e.seen == Some(seen) {
                return None;
            }
            e.seen = Some(seen);
            Some(Observation::Unreadable {
                doc,
                message: err.to_string(),
            })
        }
        Ok(bytes) => {
            e.seen = Some(Seen {
                stat: Some(st),
                error: None,
            });
            let state = DiskState::of(st.mtime, &bytes);
            match &e.target.synced {
                // Our own write read back, or a touch: not a change.
                Some(d) if d.hash == state.hash => None,
                _ => Some(Observation::Changed {
                    doc,
                    version,
                    state,
                    bytes,
                }),
            }
        }
    }
}

/// The event for an observation; unreadable files send nothing.
fn event_of(obs: Observation) -> Option<Event> {
    match obs {
        Observation::Changed {
            doc,
            version,
            state,
            bytes,
        } => Some(Event::FileChanged {
            doc,
            version,
            bytes,
            mtime: state.mtime,
            size: state.size,
            hash: state.hash,
        }),
        Observation::Deleted { doc, version } => Some(Event::FileDeleted { doc, version }),
        Observation::TooLarge {
            doc,
            version,
            mtime,
            size,
        } => Some(Event::FileTooLarge {
            doc,
            version,
            mtime,
            size,
        }),
        Observation::Unreadable { .. } => None,
    }
}

/// The loop of the watcher thread. It returns when the command channel or
/// the event channel is closed. With no watched files it waits for a command
/// and does not poll; with some, it polls every `interval`, and a command
/// arriving in between does not move the poll time.
pub(crate) fn run(
    cmds: Receiver<WatchCmd>,
    events: Sender<Event>,
    fs: &dyn FileSystem,
    interval: Duration,
) {
    let mut set = WatchSet::default();
    let mut next_poll = Instant::now() + interval;
    loop {
        let cmd = if set.is_empty() {
            match cmds.recv() {
                Ok(cmd) => cmd,
                Err(_) => return,
            }
        } else {
            match cmds.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
                Ok(cmd) => cmd,
                Err(RecvTimeoutError::Timeout) => {
                    next_poll = Instant::now() + interval;
                    for ev in poll(&mut set, fs).into_iter().filter_map(event_of) {
                        if events.send(ev).is_err() {
                            return;
                        }
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        let was_empty = set.is_empty();
        match cmd {
            WatchCmd::Set(targets) => set.set(targets),
            WatchCmd::Resync(doc) => set.resync(doc),
        }
        if was_empty && !set.is_empty() {
            next_poll = Instant::now() + interval;
        }
    }
}

/// Starts the watcher thread with the real file system and
/// [`POLL_INTERVAL`]. Its events go to `events`, the main loop's channel.
pub fn spawn(cmds: Receiver<WatchCmd>, events: Sender<Event>) {
    spawn_with(cmds, events, Box::new(RealFs), POLL_INTERVAL);
}

fn spawn_with(
    cmds: Receiver<WatchCmd>,
    events: Sender<Event>,
    fs: Box<dyn FileSystem + Send>,
    interval: Duration,
) {
    std::thread::spawn(move || run(cmds, events, fs.as_ref(), interval));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::sync::mpsc;

    /// A file in the fake file system.
    #[derive(Clone)]
    struct FakeFile {
        mtime: u64,
        bytes: Vec<u8>,
        /// `read` fails with an `Other` error while set.
        unreadable: bool,
    }

    /// An in-memory file system with a fake clock (the mtimes are seconds
    /// the test chooses), and a count of the stat calls.
    #[derive(Default)]
    struct FakeFs {
        files: RefCell<BTreeMap<PathBuf, FakeFile>>,
        stats: Cell<usize>,
    }

    impl FakeFs {
        fn put(&self, path: &str, mtime: u64, bytes: &[u8]) {
            self.files.borrow_mut().insert(
                PathBuf::from(path),
                FakeFile {
                    mtime,
                    bytes: bytes.to_vec(),
                    unreadable: false,
                },
            );
        }
        fn remove(&self, path: &str) {
            self.files.borrow_mut().remove(Path::new(path));
        }
        fn set_unreadable(&self, path: &str, on: bool) {
            if let Some(f) = self.files.borrow_mut().get_mut(Path::new(path)) {
                f.unreadable = on;
            }
        }
    }

    fn time(secs: u64) -> Option<SystemTime> {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
    }

    impl FileSystem for FakeFs {
        fn stat(&self, path: &Path) -> io::Result<Option<Stat>> {
            self.stats.set(self.stats.get() + 1);
            Ok(self.files.borrow().get(path).map(|f| Stat {
                mtime: time(f.mtime),
                size: f.bytes.len(),
            }))
        }
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            match self.files.borrow().get(path) {
                Some(f) if f.unreadable => Err(io::Error::other("locked")),
                Some(f) => Ok(f.bytes.clone()),
                None => Err(io::Error::new(ErrorKind::NotFound, "gone")),
            }
        }
    }

    /// A watch target for `path`, synced with `synced` (bytes and mtime in
    /// seconds, None for no file on disk yet).
    fn target(doc: usize, path: &str, synced: Option<(u64, &[u8])>) -> WatchTarget {
        WatchTarget {
            doc: DocId(doc),
            path: PathBuf::from(path),
            synced: synced.map(|(secs, bytes)| DiskState::of(time(secs), bytes)),
            version: 7,
        }
    }

    fn set_of(targets: Vec<WatchTarget>) -> WatchSet {
        let mut set = WatchSet::default();
        set.set(targets);
        set
    }

    /// The observation of a change to `doc` to `bytes`, which the fake file
    /// system gives modification time `secs`.
    fn changed(doc: usize, secs: u64, bytes: &[u8]) -> Observation {
        Observation::Changed {
            doc: DocId(doc),
            version: 7,
            state: DiskState::of(time(secs), bytes),
            bytes: bytes.to_vec(),
        }
    }

    /// The observation's hash, with the mtime left out of the comparison.
    fn hash_of(obs: &Observation) -> u64 {
        match obs {
            Observation::Changed { state, .. } => state.hash,
            other => panic!("not a change: {other:?}"),
        }
    }

    #[test]
    fn an_unchanged_file_is_not_reported() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        assert!(poll(&mut set, &fs).is_empty());
        assert!(poll(&mut set, &fs).is_empty());
    }

    #[test]
    fn a_new_mtime_with_the_same_contents_is_ignored() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.put("a.txt", 9, b"one\n");
        assert!(poll(&mut set, &fs).is_empty(), "touch is not a change");
        // The touch is recorded: a later poll does not read the file again.
        let before = fs.stats.get();
        assert!(poll(&mut set, &fs).is_empty());
        assert_eq!(fs.stats.get(), before + 1, "one stat per poll");
    }

    #[test]
    fn changed_contents_are_observed_with_their_bytes_and_hash() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.put("a.txt", 2, b"one two\n");
        let obs = poll(&mut set, &fs);
        assert_eq!(obs, vec![changed(0, 2, b"one two\n")]);
        assert_eq!(hash_of(&obs[0]), DiskState::of(None, b"one two\n").hash);
        // Reported once: the next poll sees the same state.
        assert!(poll(&mut set, &fs).is_empty());
    }

    #[test]
    fn a_deleted_file_is_observed_once() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.remove("a.txt");
        assert_eq!(
            poll(&mut set, &fs),
            vec![Observation::Deleted {
                doc: DocId(0),
                version: 7
            }]
        );
        assert!(poll(&mut set, &fs).is_empty());
    }

    #[test]
    fn a_missing_file_the_buffer_never_had_is_not_a_deletion() {
        let fs = FakeFs::default();
        let mut set = set_of(vec![target(0, "new.txt", None)]);
        assert!(poll(&mut set, &fs).is_empty());
    }

    #[test]
    fn a_created_file_is_observed_when_the_buffer_had_none() {
        let fs = FakeFs::default();
        let mut set = set_of(vec![target(0, "new.txt", None)]);
        fs.put("new.txt", 3, b"hello\n");
        assert_eq!(poll(&mut set, &fs), vec![changed(0, 3, b"hello\n")]);
    }

    #[test]
    fn two_documents_are_reported_separately() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"a\n");
        fs.put("b.txt", 1, b"b\n");
        let mut set = set_of(vec![
            target(0, "a.txt", Some((1, b"a\n"))),
            target(1, "b.txt", Some((1, b"b\n"))),
        ]);
        fs.put("b.txt", 2, b"bb\n");
        assert_eq!(poll(&mut set, &fs), vec![changed(1, 2, b"bb\n")]);
        fs.put("a.txt", 3, b"aa\n");
        assert_eq!(poll(&mut set, &fs), vec![changed(0, 3, b"aa\n")]);
    }

    #[test]
    fn our_own_write_is_not_reported_as_a_change() {
        // The buffer wrote "xone\n" at mtime 2. A poll that runs after the
        // write but before the editor records it reads the file and sees the
        // new contents; the hash equals the write's, so it is not an event
        // once the set carries the write's state.
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.put("a.txt", 2, b"xone\n");
        // The editor's write completes and the set is updated with its state.
        set.set(vec![target(0, "a.txt", Some((2, b"xone\n")))]);
        assert!(poll(&mut set, &fs).is_empty(), "echo of our write");
    }

    #[test]
    fn a_write_read_back_with_the_old_synced_state_is_suppressed_by_hash() {
        // The synced state is the write's (hash of "xone\n"), but the stat
        // is that of a later file: the read hashes the same, so no event.
        let fs = FakeFs::default();
        fs.put("a.txt", 5, b"xone\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((2, b"xone\n")))]);
        assert!(poll(&mut set, &fs).is_empty());
    }

    #[test]
    fn a_change_after_our_write_is_still_reported() {
        let fs = FakeFs::default();
        fs.put("a.txt", 2, b"xone\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((2, b"xone\n")))]);
        fs.put("a.txt", 3, b"other\n");
        assert_eq!(poll(&mut set, &fs), vec![changed(0, 3, b"other\n")]);
    }

    #[test]
    fn an_unreadable_file_is_an_error_observation_once() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.put("a.txt", 2, b"two\n");
        fs.set_unreadable("a.txt", true);
        let obs = poll(&mut set, &fs);
        assert_eq!(obs.len(), 1);
        assert!(matches!(
            obs[0],
            Observation::Unreadable { doc: DocId(0), .. }
        ));
        assert!(
            poll(&mut set, &fs).is_empty(),
            "the same error is not repeated"
        );
        // Readable again: the change is reported.
        fs.set_unreadable("a.txt", false);
        assert_eq!(poll(&mut set, &fs), vec![changed(0, 2, b"two\n")]);
    }

    #[test]
    fn a_stat_error_is_not_a_deletion() {
        struct Denied;
        impl FileSystem for Denied {
            fn stat(&self, _: &Path) -> io::Result<Option<Stat>> {
                Err(io::Error::new(ErrorKind::PermissionDenied, "no"))
            }
            fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
                unreachable!()
            }
        }
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        let obs = poll(&mut set, &Denied);
        assert!(matches!(obs.as_slice(), [Observation::Unreadable { .. }]));
        assert!(poll(&mut set, &Denied).is_empty());
    }

    #[test]
    fn resync_makes_the_next_poll_report_a_change_again() {
        // What a dropped event needs: the change was looked at, the editor
        // did not take it, so the next poll must look again.
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.put("a.txt", 2, b"two\n");
        assert_eq!(poll(&mut set, &fs).len(), 1);
        assert!(poll(&mut set, &fs).is_empty());
        set.resync(DocId(0));
        assert_eq!(poll(&mut set, &fs), vec![changed(0, 2, b"two\n")]);
    }

    #[test]
    fn a_new_synced_state_restarts_from_what_the_editor_has() {
        // The editor reloaded "two" (synced at mtime 2): the file it reads
        // is the same, so nothing is reported, even though the last look
        // was before the reload.
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut set = set_of(vec![target(0, "a.txt", Some((1, b"one\n")))]);
        fs.put("a.txt", 2, b"two\n");
        set.set(vec![target(0, "a.txt", Some((2, b"two\n")))]);
        assert!(poll(&mut set, &fs).is_empty());
    }

    #[test]
    fn the_version_of_the_command_is_carried_into_the_observation() {
        let fs = FakeFs::default();
        fs.put("a.txt", 1, b"one\n");
        let mut t = target(0, "a.txt", Some((1, b"one\n")));
        t.version = 42;
        let mut set = set_of(vec![t]);
        fs.put("a.txt", 2, b"two\n");
        match poll(&mut set, &fs).as_slice() {
            [Observation::Changed { version, .. }] => assert_eq!(*version, 42),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_event_carries_the_state_of_the_bytes_it_read() {
        let obs = Observation::Changed {
            doc: DocId(3),
            version: 9,
            state: DiskState::of(time(4), b"abc"),
            bytes: b"abc".to_vec(),
        };
        match event_of(obs) {
            Some(Event::FileChanged {
                doc,
                version,
                bytes,
                mtime,
                size,
                hash,
            }) => {
                assert_eq!((doc, version, bytes.as_slice()), (DocId(3), 9, &b"abc"[..]));
                assert_eq!(mtime, time(4));
                assert_eq!(size, 3);
                assert_eq!(hash, DiskState::of(None, b"abc").hash);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            event_of(Observation::Deleted {
                doc: DocId(0),
                version: 1
            }),
            Some(Event::FileDeleted {
                doc: DocId(0),
                version: 1
            })
        );
        assert_eq!(
            event_of(Observation::Unreadable {
                doc: DocId(0),
                message: String::new()
            }),
            None
        );
    }

    #[test]
    fn a_file_over_the_cap_is_not_read_and_is_reported_once() {
        /// Reports a file one byte over the cap, and counts the reads.
        struct Huge {
            reads: Cell<usize>,
        }
        impl FileSystem for Huge {
            fn stat(&self, _: &Path) -> io::Result<Option<Stat>> {
                Ok(Some(Stat {
                    mtime: time(5),
                    size: MAX_WATCH_BYTES + 1,
                }))
            }
            fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
                self.reads.set(self.reads.get() + 1);
                Ok(Vec::new())
            }
        }
        let fs = Huge {
            reads: Cell::new(0),
        };
        let mut set = set_of(vec![target(0, "big.bin", Some((1, b"one\n")))]);
        assert_eq!(
            poll(&mut set, &fs),
            vec![Observation::TooLarge {
                doc: DocId(0),
                version: 7,
                mtime: time(5),
                size: MAX_WATCH_BYTES + 1,
            }]
        );
        assert_eq!(fs.reads.get(), 0, "the file was not read");
        assert!(poll(&mut set, &fs).is_empty(), "reported once");
        assert_eq!(fs.reads.get(), 0);
        assert!(matches!(
            event_of(Observation::TooLarge {
                doc: DocId(0),
                version: 7,
                mtime: None,
                size: 1,
            }),
            Some(Event::FileTooLarge { size: 1, .. })
        ));
    }

    /// A real file in a temp dir, watched by the real loop with a 20 ms
    /// interval. The test waits on the event channel with a timeout.
    #[test]
    fn the_worker_thread_reports_a_real_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "one\n").unwrap();
        let disk = RealFs.stat(&path).unwrap().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let synced = DiskState::of(disk.mtime, &bytes);

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (ev_tx, ev_rx) = mpsc::channel();
        spawn_with(cmd_rx, ev_tx, Box::new(RealFs), Duration::from_millis(20));
        cmd_tx
            .send(WatchCmd::Set(vec![WatchTarget {
                doc: DocId(0),
                path: path.clone(),
                synced: Some(synced),
                version: 1,
            }]))
            .unwrap();
        // A different size, so the change shows even on a coarse clock.
        std::fs::write(&path, "one two three\n").unwrap();
        match ev_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Event::FileChanged {
                doc,
                version,
                bytes,
                ..
            }) => {
                assert_eq!(doc, DocId(0));
                assert_eq!(version, 1);
                assert_eq!(bytes, b"one two three\n");
            }
            other => panic!("no change event: {other:?}"),
        }
        // Removing the file reports a deletion.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            ev_rx.recv_timeout(Duration::from_secs(10)),
            Ok(Event::FileDeleted {
                doc: DocId(0),
                version: 1
            })
        );
    }

    /// With nothing watched the loop never looks at the file system: it
    /// waits for a command. The test sends none and shuts the loop down.
    #[test]
    fn with_no_watched_files_the_loop_never_stats() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        /// Counts the calls; `Send`, so the loop can run on its thread.
        struct Counting(Arc<AtomicUsize>);
        impl FileSystem for Counting {
            fn stat(&self, _: &Path) -> io::Result<Option<Stat>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }
            fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let (cmd_tx, cmd_rx) = mpsc::channel::<WatchCmd>();
        let (ev_tx, ev_rx) = mpsc::channel();
        let fs = Counting(calls.clone());
        let worker = std::thread::spawn(move || run(cmd_rx, ev_tx, &fs, Duration::from_millis(1)));
        // Many intervals pass while the loop waits for a command.
        std::thread::sleep(Duration::from_millis(50));
        drop(cmd_tx);
        worker.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(ev_rx.try_recv().is_err());
    }
}
