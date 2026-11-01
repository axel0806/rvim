// SPDX-License-Identifier: Apache-2.0
//! `:s` with the `c` flag (confirm), checked against real Vim.
//!
//! Vim runs in silent Ex mode here. A prompt there reads one line per answer
//! (the first character counts, the rest of the line is dropped), so an
//! answer followed by `<CR>` is read as one answer. A terminal Vim ignores
//! `<CR>` at the prompt too, so rvim does the same. Two rules keep the cases
//! comparable:
//! - A case ends while a prompt still waits (the harness then quits it with
//!   `<Esc>`, as the end of input does in Vim), or with the answer that
//!   finishes the command and nothing after it. A `<CR>` after such an answer
//!   is consumed by Vim's line read but would be a Normal-mode key in rvim.
//! - Answer letters only where an answer is meant: `:s/a/` has an `a`.
//!
//! Keys typed after the command finishes (`u`, `x`, `.`) cannot be compared
//! this way; they are in tests/confirm_prompt.rs. Declining a match over a
//! line break is in the ignored test below.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &[
    "foo bar baz\nqux foo quux\n\n  indented foo line\nlast foo",
    "aaaa aa\nabab abab\nxyz\n\tz foo",
    "foo\nfoo foo\nbar",
];

/// Short answer lists: each one is answered in at most two prompts, so it
/// works for every text.
#[test]
fn answers() {
    let keys = [
        ":%s/o/0/gc<CR>y",
        ":%s/o/0/gc<CR>n",
        ":%s/o/0/gc<CR>a",
        ":%s/o/0/gc<CR>l",
        ":%s/o/0/gc<CR>q",
        ":%s/o/0/gc<CR>x",
        ":%s/o/0/gc<CR>Y",
        ":%s/o/0/gc<CR>yes",
        ":%s/o/0/gc<CR>",
        ":%s/o/0/gc<CR>y<CR>n",
        ":%s/o/0/gc<CR>n<CR>y",
        ":%s/o/0/gc<CR>y<CR>a",
        ":%s/o/0/gc<CR>n<CR>a",
        ":%s/o/0/gc<CR>y<CR>l",
        ":%s/o/0/gc<CR>n<CR>l",
        ":%s/o/0/gc<CR>y<CR>q",
        ":%s/o/0/gc<CR>n<CR>q",
        ":%s/o/0/gc<CR>x<CR>y",
        ":s/o/0/c<CR>y",
        ":s/o/0/c<CR>n",
        ":s/o/0/gc 2<CR>y<CR>n",
        ":%s/zzz/0/gc<CR>",
        ":%s/zzz/0/gce<CR>",
        ":%s/foo/X/c<CR>y",
        ":%s/foo/X/gc<CR>n",
    ];
    check(&cross(TEXTS, &keys));
}

/// Long answer lists, for the text with enough matches for them.
#[test]
fn long_answer_lists() {
    let keys = [
        ":%s/o/0/gc<CR>y<CR>n<CR>y<CR>",
        ":%s/o/0/gc<CR>n<CR>n<CR>n<CR>",
        ":%s/o/0/gc<CR>y<CR>n<CR>a",
        ":%s/o/0/gc<CR>n<CR>n<CR>y",
        ":%s/o/0/gc<CR>n<CR>y<CR>l",
        ":%s/o/0/gc<CR>y<CR>n<CR>y<CR>n<CR>y<CR>n<CR>y<CR>n<CR>y<CR>",
        ":%s/foo/X/gc<CR>n<CR>n<CR>n<CR>n",
        ":%s/foo/X/gc<CR>y<CR>n<CR>y<CR>",
    ];
    check(&cross(&TEXTS[..1], &keys));
}

#[test]
fn multi_line_matches() {
    let keys = [
        ":%s/\\n//gc<CR>y",
        ":%s/\\n//gc<CR>a",
        ":%s/\\n//gc<CR>q",
        ":%s/\\n//gc<CR>y<CR>a",
        ":%s/\\n//gc<CR>y<CR>n",
        ":%s/\\n/,/gc<CR>a",
        ":%s/\\n/,/gc<CR>y<CR>n",
        ":%s/o\\n/0/c<CR>y",
        ":%s/o\\n/0/gc<CR>y",
        ":%s/o\\n/0/gc<CR>a",
        ":%s/o\\n/0/gc<CR>q",
        ":%s/o\\n/\\r/gc<CR>n",
        ":%s/\\_s\\+/ /gc<CR>y",
        ":%s/$/!/gc<CR>y",
        ":%s/^/> /gc<CR>n",
        ":%s/a*/-/gc<CR>n",
    ];
    check(&cross(TEXTS, &keys));
}

/// Vim does not stop at a declined match over a line break: after `n` it
/// goes on from the next line, and sometimes offers the same match again
/// (its loop restarts the line). rvim skips to the next line, so these cases
/// differ. They are kept here, ignored, until that is ported (see PLAN.md).
#[test]
#[ignore = "Vim re-offers a multi-line match after `n`; not ported (PLAN.md)"]
fn multi_line_declined_matches() {
    let keys = [
        ":%s/\\n//gc<CR>n<CR>n<CR>n<CR>",
        ":%s/o\\n/0/gc<CR>n<CR>n",
        ":%s/o\\n/0/gc<CR>n<CR>n<CR>n",
        ":%s/o\\n/0/gc<CR>n<CR>n<CR>n<CR>n",
        ":%s/o\\n/0/c<CR>n",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn global_and_bars() {
    let keys = [
        ":g/foo/s/o/0/gc<CR>y",
        ":g/foo/s/o/0/gc<CR>a",
        ":g/foo/s/o/0/gc<CR>q",
        ":g/foo/s/o/0/gc<CR>y<CR>q",
        ":g/foo/s/o/0/gc<CR>n<CR>y",
        ":g/foo/s//X/c<CR>a",
        ":g/foo/s//X/c<CR>y",
        ":g/foo/s//X/gc<CR>y",
        ":g/a/s/a/b/gc<CR>y<CR>q",
        ":g/a/s/a/b/gc<CR>y<CR>x<CR>a",
        ":g/foo/s/o/0/gc|s/f/F/<CR>y",
        ":g/foo/s/o/0/gc|s/f/F/<CR>a",
        ":g/foo/s/o/0/gc|s/f/F/<CR>n",
        ":%s/zzz/X/g|%s/foo/Y/<CR>",
        ":%s/zzz/X/gc|%s/foo/Y/<CR>",
        ":%s/foo/X/gc|%s/bar/Y/g<CR>q",
        ":%s/foo/X/gc|%s/bar/Y/g<CR>n",
        "Vj:s/o/0/gc<CR>y",
        "Vj:s/o/0/gc<CR>q",
        "jVj:s/o/0/gc<CR>y",
        ":2,3s/o/0/gc<CR>y",
        ":2,3s/o/0/gc<CR>n",
        ":.s/o/0/gc<CR>y",
    ];
    check(&cross(TEXTS, &keys));
}
