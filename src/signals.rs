// SPDX-License-Identifier: Apache-2.0
//! Signals that end the session: SIGHUP (the terminal was closed) and SIGTERM.
//! The signal handler does no work of its own. signal-hook's iterator thread
//! reads the signal and sends [`Event::Signal`] into the main loop's channel;
//! the main thread then writes the recovery files and exits (`main.rs`).
//! SIGINT, SIGTSTP and SIGCONT are not touched.

use crate::event::Event;
use std::io;
use std::sync::mpsc::Sender;

/// SIGHUP. The POSIX numbers, the same on Linux, macOS and the BSDs.
pub const HANGUP: i32 = 1;
/// SIGTERM.
pub const TERM: i32 = 15;

/// The shell's exit status for a process ended by signal `sig`: 128 + n.
pub fn exit_code(sig: i32) -> i32 {
    128 + sig
}

/// The signal's name for messages.
pub fn name(sig: i32) -> String {
    match sig {
        HANGUP => "SIGHUP".to_string(),
        TERM => "SIGTERM".to_string(),
        n => format!("signal {n}"),
    }
}

/// Registers SIGHUP and SIGTERM and forwards each one to `tx` as an
/// [`Event::Signal`]. On Windows nothing is registered (console close
/// events are not handled).
#[cfg(unix)]
pub fn spawn(tx: Sender<Event>) -> io::Result<()> {
    use signal_hook::consts::{SIGHUP, SIGTERM};
    use signal_hook::iterator::Signals;
    debug_assert!(HANGUP == SIGHUP && TERM == SIGTERM);
    let mut signals = Signals::new([SIGHUP, SIGTERM])?;
    std::thread::spawn(move || {
        for sig in signals.forever() {
            if tx.send(Event::Signal(sig)).is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// Non-Unix: no signals are registered.
#[cfg(not(unix))]
pub fn spawn(_tx: Sender<Event>) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_is_128_plus_signal_number() {
        assert_eq!(exit_code(HANGUP), 129);
        assert_eq!(exit_code(TERM), 143);
        assert_eq!(exit_code(2), 130);
    }

    #[test]
    fn names_for_messages() {
        assert_eq!(name(HANGUP), "SIGHUP");
        assert_eq!(name(TERM), "SIGTERM");
        assert_eq!(name(2), "signal 2");
    }

    #[cfg(unix)]
    #[test]
    fn sighup_and_sigterm_reach_the_channel() {
        use std::sync::mpsc;
        use std::time::Duration;
        let (tx, rx) = mpsc::channel();
        spawn(tx).unwrap();
        // The handler is registered before `raise`, so the process survives.
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(Event::Signal(TERM))
        );
        signal_hook::low_level::raise(signal_hook::consts::SIGHUP).unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(Event::Signal(HANGUP))
        );
    }
}
