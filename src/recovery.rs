// SPDX-License-Identifier: Apache-2.0
//! Emergency recovery files: when rvim panics, the text of a modified buffer
//! is written here so no work is lost. (Swap files come with milestone 5.)

use crate::buffer::Buffer;
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

/// Writes the buffer text to `dir/<name>.<unix time>` and returns the path.
/// Line endings and a missing final newline follow the buffer's own flags.
pub fn write_recovery(buf: &Buffer, dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = buf
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map_or("unnamed".to_string(), |n| n.to_string_lossy().into_owned());
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut path = dir.join(format!("{name}.{secs}"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{name}.{secs}.{n}"));
        n += 1;
    }
    let mut text = buf.text();
    if buf.crlf {
        text = text.replace('\n', "\r\n");
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_text_and_avoids_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let mut ed = crate::Editor::from_text("one\ntwo\n");
        ed.feed("x");
        let a = write_recovery(&ed.buf, dir.path()).unwrap();
        let b = write_recovery(&ed.buf, dir.path()).unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(a).unwrap(), "ne\ntwo\n");
    }
}
