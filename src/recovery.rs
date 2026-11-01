// SPDX-License-Identifier: Apache-2.0
//! Emergency recovery files: when rvim panics, the text of a modified buffer
//! is written here so no work is lost. (Swap files come with milestone 5.)

use crate::buffer::Buffer;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `$XDG_STATE_HOME/rvim/recovery`, `~/.local/state/rvim/recovery`, or
/// `%LOCALAPPDATA%\rvim\recovery` on Windows.
pub fn recovery_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| PathBuf::from(h).join(".local").join("state"))
        })
        .unwrap_or_else(std::env::temp_dir);
    base.join("rvim").join("recovery")
}

/// Where [`write_recovery`] put the text.
#[derive(Debug)]
pub struct Saved {
    /// The new recovery file.
    pub path: PathBuf,
    /// Set when `dir` was not used because it is a symbolic link or is not
    /// owned by the current user. The file then went to a fresh private
    /// directory under the temp dir.
    pub refused_dir: Option<PathBuf>,
}

/// Writes the buffer text to `dir/<name>.<unix time>` and returns the path.
/// The name is never overwritten: a clash gets `.1`, `.2`, ... appended.
/// Line endings and a missing final newline follow the buffer's own flags.
///
/// On Unix the directory is created with mode 0700 and the file with mode
/// 0600 (the umask can only remove bits). An existing directory owned by the
/// current user is tightened to 0700. Symbolic links are not followed.
pub fn write_recovery(buf: &Buffer, dir: &Path) -> io::Result<Saved> {
    let name = buf
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map_or("unnamed".to_string(), |n| n.to_string_lossy().into_owned());
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut text = buf.text();
    if buf.crlf {
        text = text.replace('\n', "\r\n");
    }
    let (mut file, path, refused_dir) = create_in(dir, &name, secs)?;
    file.write_all(text.as_bytes())?;
    Ok(Saved { path, refused_dir })
}

/// Writes every modified buffer in `bufs` with [`write_recovery`] and returns
/// one result per modified buffer, in order. Unmodified buffers are skipped.
/// The panic hook and the SIGHUP/SIGTERM path both use this.
pub fn write_modified(bufs: &[&Buffer], dir: &Path) -> Vec<io::Result<Saved>> {
    bufs.iter()
        .filter(|b| b.modified)
        .map(|b| write_recovery(b, dir))
        .collect()
}

/// Creates `dir/<name>.<secs>`, or the first free `<name>.<secs>.<n>`, with
/// `create_new`, so an existing file or a symbolic link at that name is never
/// written through.
fn create_unique(dir: &Path, name: &str, secs: u64) -> io::Result<(PathBuf, File)> {
    let mut n = 0u32;
    loop {
        let file_name = if n == 0 {
            format!("{name}.{secs}")
        } else {
            format!("{name}.{secs}.{n}")
        };
        let path = dir.join(file_name);
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && n < 10_000 => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// Windows: the default directory is under `%LOCALAPPDATA%`, which is already
/// per user, so no permissions are set here. Owner-only files are a Unix
/// guarantee; setting a Windows ACL would need the security API.
#[cfg(not(unix))]
fn create_in(dir: &Path, name: &str, secs: u64) -> io::Result<(File, PathBuf, Option<PathBuf>)> {
    std::fs::create_dir_all(dir)?;
    let (path, file) = create_unique(dir, name, secs)?;
    Ok((file, path, None))
}

/// Unix: owner-only directory and files, as described on [`write_recovery`].
#[cfg(unix)]
fn create_in(dir: &Path, name: &str, secs: u64) -> io::Result<(File, PathBuf, Option<PathBuf>)> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let (target, refused) = unix_dir(dir)?;
    let (path, file) = create_unique(&target, name, secs)?;
    // This process just created the file, so its owner is the current user.
    // A directory with another owner is not used: chmod would fail there,
    // and another user could rename or replace our files.
    let owner = file.metadata()?.uid();
    let meta = std::fs::metadata(&target)?;
    if meta.uid() == owner {
        if meta.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700))?;
        }
        return Ok((file, path, refused));
    }
    drop(file);
    let _ = std::fs::remove_file(&path);
    let fresh = fresh_private_dir()?;
    let (path, file) = create_unique(&fresh, name, secs)?;
    Ok((file, path, Some(dir.to_path_buf())))
}

/// Returns the directory to write into, creating it with mode 0700 if it is
/// missing. A symbolic link is never followed: the result is then a fresh
/// private directory, and the link is returned as the refused path.
#[cfg(unix)]
fn unix_dir(dir: &Path) -> io::Result<(PathBuf, Option<PathBuf>)> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.file_type().is_symlink() => Ok((fresh_private_dir()?, Some(dir.to_path_buf()))),
        Ok(m) if m.is_dir() => Ok((dir.to_path_buf(), None)),
        Ok(_) => Err(io::Error::other(format!(
            "{} is not a directory",
            dir.display()
        ))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match std::fs::DirBuilder::new().mode(0o700).create(dir) {
                Ok(()) => Ok((dir.to_path_buf(), None)),
                // Created by someone else in the meantime: look again.
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => unix_dir(dir),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Creates a new directory with mode 0700 under the temp dir. `mkdir` fails
/// on an existing name, symbolic links included, so nothing is followed.
#[cfg(unix)]
fn fresh_private_dir() -> io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for n in 0..100 {
        let dir = base.join(format!("rvim-recovery-{}-{nanos}-{n}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free name for a recovery directory",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> crate::Editor {
        let mut ed = crate::Editor::from_text("one\ntwo\n");
        ed.feed("x");
        ed
    }

    #[test]
    fn writes_text_and_avoids_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let ed = sample();
        let a = write_recovery(&ed.doc().buf, dir.path()).unwrap().path;
        let b = write_recovery(&ed.doc().buf, dir.path()).unwrap().path;
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(a).unwrap(), "ne\ntwo\n");
    }

    #[test]
    fn only_modified_documents_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let clean = crate::Editor::from_text("kept\n");
        let dirty = sample();
        let results = write_modified(&[clean.buf(), dirty.buf()], dir.path());
        assert_eq!(results.len(), 1);
        let saved = results.into_iter().next().unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(&saved.path).unwrap(), "ne\ntwo\n");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn no_modified_documents_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let clean = crate::Editor::from_text("kept\n");
        assert!(write_modified(&[clean.buf()], dir.path()).is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn existing_name_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let taken = dir.path().join("notes.txt.5");
        std::fs::write(&taken, "keep me").unwrap();
        let (path, _file) = create_unique(dir.path(), "notes.txt", 5).unwrap();
        assert_eq!(path, dir.path().join("notes.txt.5.1"));
        assert_eq!(std::fs::read_to_string(&taken).unwrap(), "keep me");
    }

    #[cfg(unix)]
    fn mode_of(p: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn new_directory_and_file_are_owner_only() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("state").join("recovery");
        let saved = write_recovery(sample().buf(), &dir).unwrap();
        assert!(saved.refused_dir.is_none());
        assert_eq!(saved.path.parent(), Some(dir.as_path()));
        assert_eq!(mode_of(&saved.path), 0o600);
        assert_eq!(mode_of(&dir), 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn loose_directory_we_own_is_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("recovery");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let saved = write_recovery(sample().buf(), &dir).unwrap();
        assert!(saved.refused_dir.is_none());
        assert_eq!(mode_of(&dir), 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_is_not_followed() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = base.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let saved = write_recovery(sample().buf(), &link).unwrap();
        assert_eq!(saved.refused_dir.as_deref(), Some(link.as_path()));
        assert!(!saved.path.starts_with(&link));
        assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
        assert_eq!(std::fs::read_to_string(&saved.path).unwrap(), "ne\ntwo\n");
        assert_eq!(mode_of(&saved.path), 0o600);
        let fresh = saved.path.parent().unwrap();
        assert_eq!(mode_of(fresh), 0o700);
        let _ = std::fs::remove_dir_all(fresh);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_at_file_name_is_not_followed() {
        let base = tempfile::tempdir().unwrap();
        let victim = base.path().join("victim");
        std::fs::write(&victim, "keep me").unwrap();
        let dir = base.path().join("recovery");
        std::fs::create_dir(&dir).unwrap();
        std::os::unix::fs::symlink(&victim, dir.join("notes.txt.5")).unwrap();
        let (path, _file) = create_unique(&dir, "notes.txt", 5).unwrap();
        assert_eq!(path, dir.join("notes.txt.5.1"));
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep me");
    }
}
