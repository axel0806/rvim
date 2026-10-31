//! Differential testing against a real Vim. Every case is a starting text and
//! a key sequence in Vim notation. All cases run in one headless Vim process
//! (`vim -N -u NONE -es`, i.e. Vim defaults without any vimrc), fed with
//! `feedkeys(keys, 'tx')` so they behave as typed (a failing motion does not
//! abort the rest, unlike `:normal`). The resulting lines and cursor are
//! compared with rvim's.
//!
//! The Vim binary is `$RVIM_REFERENCE_VIM` or `vim` on PATH. If it cannot be
//! run the tests are skipped with a warning, or fail when
//! `RVIM_REQUIRE_VIM=1`.

use rvim::{Editor, Key, Mode, key::parse_keys};
use std::fmt::Write as _;
use std::process::Command;

pub struct Outcome {
    pub lines: Vec<String>,
    /// 1-based line and character column, like Vim's `line('.')`/`charcol('.')`.
    pub cursor: (usize, usize),
}

pub fn run_rvim(text: &str, keys: &str) -> Outcome {
    let mut ed = Editor::from_text(text);
    ed.feed(keys);
    // feedkeys(..., 'x') ends Insert mode as if <Esc> was typed.
    if ed.mode() != Mode::Normal {
        ed.handle_key(Key::Esc);
    }
    let c = ed.cursor();
    Outcome {
        lines: ed.buf.lines(),
        cursor: (c.line + 1, c.col + 1),
    }
}

fn vim_single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The keys as a Vim double-quoted string, via rvim's own parser so both
/// sides see the same key sequence.
fn vim_key_string(keys: &str) -> String {
    let mut out = String::from("\"");
    for k in parse_keys(keys) {
        match k {
            Key::Char('"') => out.push_str("\\\""),
            Key::Char('\\') => out.push_str("\\\\"),
            Key::Char('<') => out.push_str("\\<lt>"),
            Key::Char(c) => out.push(c),
            Key::Ctrl(c) => write!(out, "\\<C-{c}>").unwrap(),
            other => {
                let name = match other {
                    Key::Esc => "Esc",
                    Key::Enter => "CR",
                    Key::Backspace => "BS",
                    Key::Tab => "Tab",
                    Key::Delete => "Del",
                    Key::Left => "Left",
                    Key::Right => "Right",
                    Key::Up => "Up",
                    Key::Down => "Down",
                    Key::Home => "Home",
                    Key::End => "End",
                    Key::PageUp => "PageUp",
                    Key::PageDown => "PageDown",
                    Key::Char(_) | Key::Ctrl(_) => unreachable!(),
                };
                write!(out, "\\<{name}>").unwrap();
            }
        }
    }
    out.push('"');
    out
}

const SCRIPT_BODY: &str = r#"
let s:out = []
for [s:lines, s:keys] in g:cases
  silent %delete _
  call setline(1, s:lines)
  set nomodified
  call cursor(1, 1)
  " silent!: an error message must not abort the remaining cases.
  silent! call feedkeys(s:keys, 'tx')
  if mode() !=# 'n'
    call feedkeys("\<Esc>", 'tx')
  endif
  let s:got = getline(1, '$')
  call add(s:out, '@@ ' . line('.') . ' ' . charcol('.') . ' ' . len(s:got))
  call extend(s:out, s:got)
endfor
call writefile(s:out, 'out.txt')
qa!
"#;

/// Runs all cases in Vim. `None` if Vim is unavailable.
pub fn run_vim(cases: &[(&str, &str)]) -> Option<Vec<Outcome>> {
    let vim = std::env::var("RVIM_REFERENCE_VIM").unwrap_or_else(|_| "vim".into());
    let dir = tempfile::tempdir().unwrap();
    let mut script = String::from("let g:cases = []\n");
    for (text, keys) in cases {
        let lines: Vec<String> = text.split('\n').map(vim_single_quoted).collect();
        writeln!(
            script,
            "call add(g:cases, [[{}], {}])",
            lines.join(", "),
            vim_key_string(keys)
        )
        .unwrap();
    }
    script.push_str(SCRIPT_BODY);
    std::fs::write(dir.path().join("cases.vim"), script).unwrap();

    let status = Command::new(&vim)
        .current_dir(dir.path())
        .args(["-N", "-u", "NONE", "-i", "NONE", "-n", "-es"])
        .args(["--cmd", "set encoding=utf-8", "-S", "cases.vim"])
        .stdout(std::process::Stdio::null())
        .status();
    // Vim exits with 1 if any case raised an error message, which is
    // expected (e.g. E16); only missing output means the run failed.
    let out = std::fs::read_to_string(dir.path().join("out.txt")).ok();
    let Some(out) = out else {
        let msg = format!("reference Vim `{vim}` failed or is missing ({status:?})");
        if std::env::var("RVIM_REQUIRE_VIM").as_deref() == Ok("1") {
            panic!("{msg}");
        }
        eprintln!("warning: {msg}; skipping compatibility checks");
        return None;
    };

    let mut lines = out.lines();
    let mut outcomes = Vec::new();
    while let Some(header) = lines.next() {
        let nums: Vec<usize> = header
            .strip_prefix("@@ ")
            .expect("case header")
            .split(' ')
            .map(|n| n.parse().unwrap())
            .collect();
        let got = (0..nums[2])
            .map(|_| lines.next().unwrap().to_string())
            .collect();
        outcomes.push(Outcome {
            lines: got,
            cursor: (nums[0], nums[1]),
        });
    }
    assert_eq!(outcomes.len(), cases.len(), "Vim output is incomplete");
    Some(outcomes)
}

/// Compares rvim with Vim on every case and fails listing all mismatches.
pub fn check(cases: &[(&str, &str)]) {
    let Some(want) = run_vim(cases) else { return };
    let mut failures = Vec::new();
    for ((text, keys), want) in cases.iter().zip(want) {
        let got = run_rvim(text, keys);
        if got.lines != want.lines || got.cursor != want.cursor {
            failures.push(format!(
                "text {text:?} keys {keys:?}\n    vim:  {:?} cursor {:?}\n    rvim: {:?} cursor {:?}",
                want.lines, want.cursor, got.lines, got.cursor
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from Vim:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// Every key sequence applied to every text.
pub fn cross<'a>(texts: &[&'a str], keys: &[&'a str]) -> Vec<(&'a str, &'a str)> {
    texts
        .iter()
        .flat_map(|t| keys.iter().map(move |k| (*t, *k)))
        .collect()
}
