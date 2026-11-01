//! Normal and Visual mode: parsing `["x][count]cmd[count][motion]` key
//! sequences, the motions (ports of Vim's normal.c `nv_*` functions) and
//! command dispatch. Operators themselves live in ops.rs.

use crate::editor::{Editor, Mode, Redo, VisualMode, VisualRedo};
use crate::key::Key;
use crate::motion::{
    self, Cur, Pos, findmatchlimit, findpar, findsent, first_non_blank, gchar, searchc,
};
use crate::register::valid_name;
use crate::scroll::ScrollCmd;
use crate::text;
use crate::textobject::{self, ObjOut, VisualCtx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Delete,
    Change,
    Yank,
    LShift,
    RShift,
    Tilde,
    Upper,
    Lower,
    Rot13,
    Join,
    JoinNs,
    Replace(char),
    Insert,
    Append,
}

impl Op {
    /// Vim's `op_on_lines()`: operators that always work on whole lines.
    pub(crate) fn on_lines(self) -> bool {
        matches!(self, Op::Join | Op::JoinNs | Op::LShift | Op::RShift)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Motion {
    Left {
        bs: bool,
    },
    Right {
        space: bool,
    },
    Up,
    Down,
    UpFnb,
    DownFnb,
    Underscore,
    Zero,
    Caret,
    Dollar,
    Bar,
    LastNonBlank,
    /// `w W b B e E`, and `g`/`G` for `ge`/`gE`.
    Word(char),
    Goto {
        first: bool,
    },
    Percent,
    Para {
        fwd: bool,
    },
    Sent {
        fwd: bool,
    },
    /// `H`, `M` or `L`: linewise, to the first non-blank.
    ScreenLine(char),
    Find {
        c: char,
        fwd: bool,
        till: bool,
    },
    RepeatFind {
        reverse: bool,
    },
    /// `/` or `?` with its pattern; `klen` keys including the final `<CR>`.
    Search {
        fwd: bool,
        klen: usize,
    },
    /// `n` and `N`.
    SearchNext {
        reverse: bool,
    },
    /// `* # g* g#`.
    SearchWord {
        fwd: bool,
        whole: bool,
    },
    /// `'{a-z}` (linewise, first non-blank) and `` `{a-z} `` (exact), and
    /// `''` and ``` `` ``` for the previous context mark.
    Mark {
        mark: char,
        linewise: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Obj {
    pub kind: char,
    pub inner: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Motion(Motion),
    Obj(Obj),
    /// The operator doubled (`dd`, `>>`), Vim's `nv_lineop`.
    Line,
    /// `gn` / `gN` after an operator: the match becomes the Visual area.
    Gn {
        fwd: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Simple {
    /// `i a I A o O`, and `g` for `gI`.
    Insert(char),
    /// `r{char}`; `'\n'` for `r<CR>`.
    ReplaceChar(char),
    ReplaceMode,
    Join {
        spaces: bool,
    },
    Put {
        before: bool,
        gp: bool,
    },
    SwapCase,
    Undo,
    Redo,
    UndoLine,
    Dot,
    Visual(VisualMode),
    Reselect,
    CmdLine,
    /// `m{a-z}`, and `m'` and ``m` `` for the context mark.
    SetMark(char),
    /// `<C-o>` (older) and `<Tab>` (newer) in the jumplist.
    JumpOlder,
    JumpNewer,
    /// `&` (`:s`) and `g&` (`:%s//~/&`).
    Ampersand {
        all: bool,
    },
    /// `q{reg}`.
    Record(char),
    /// `@{reg}`.
    Macro(char),
    WriteQuit,
    Quit,
    FileInfo,
    Redraw,
    Esc,
    /// Visual `o` / `O`.
    VisOther {
        horiz: bool,
    },
    /// `gn` / `gN` (Normal and Visual mode): select the next match.
    Gn {
        fwd: bool,
    },
    /// CTRL-E/Y/F/B/D/U and `zt zz zb z<CR> z. z-`.
    Scroll(ScrollCmd),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Body {
    Motion(Motion),
    Op {
        op: Op,
        force: Option<char>,
        target: Target,
    },
    /// An operator key in Visual mode: the original key decides linewise
    /// variants (`X D Y C S R`).
    VisOp {
        op: Op,
        key: char,
    },
    Obj(Obj),
    Simple(Simple),
}

#[derive(Clone, Debug)]
pub(crate) struct Cmd {
    pub reg: Option<char>,
    pub count: Option<usize>,
    pub body: Body,
    /// The keys after register and counts (for `.`).
    pub keys: Vec<Key>,
    /// Pattern typed for a `/` or `?` motion.
    pub search: Option<String>,
}

pub(crate) enum Parse {
    Incomplete,
    Invalid,
    Done(Cmd),
}

/// `Ok(None)`: need more keys. `Err(())`: not valid.
type Res<T> = Result<Option<T>, ()>;

const OBJ_CHARS: &str = "wWsp()b{}B[]<>\"'`t";

fn ch(k: Option<&Key>) -> Option<char> {
    match k {
        Some(Key::Char(c)) => Some(*c),
        _ => None,
    }
}

/// A char argument (`f{char}`, `r{char}`): `<Tab>` counts as a char.
fn char_arg(k: Option<&Key>) -> Res<char> {
    match k {
        None => Ok(None),
        Some(Key::Char(c)) => Ok(Some(*c)),
        Some(Key::Tab) => Ok(Some('\t')),
        _ => Err(()),
    }
}

/// The pattern of a `/` or `?` motion and the keys used up to `<CR>`
/// (`None` until `<CR>`). The editing keys of the prompt are handled by
/// cmdline.rs, which hands the parser the finished text as plain characters.
pub(crate) fn search_text(k: &[Key]) -> Option<(String, usize)> {
    let mut s = String::new();
    for (i, key) in k.iter().enumerate() {
        match key {
            Key::Enter => return Some((s, i + 1)),
            Key::Char(c) => s.push(*c),
            Key::Tab => s.push('\t'),
            _ => {}
        }
    }
    None
}

pub(crate) fn parse_motion(k: &[Key]) -> Res<(Motion, usize)> {
    let Some(&k0) = k.first() else {
        return Ok(None);
    };
    let m = match k0 {
        Key::Char('h') | Key::Left => Motion::Left { bs: false },
        Key::Backspace => Motion::Left { bs: true },
        Key::Char('l') | Key::Right => Motion::Right { space: false },
        Key::Char(' ') => Motion::Right { space: true },
        Key::Char('j') | Key::Down | Key::Ctrl('n' | 'j') => Motion::Down,
        Key::Char('k') | Key::Up | Key::Ctrl('p') => Motion::Up,
        Key::Char('+') | Key::Enter => Motion::DownFnb,
        Key::Char('-') => Motion::UpFnb,
        Key::Char('_') => Motion::Underscore,
        Key::Char('0') | Key::Home => Motion::Zero,
        Key::Char('^') => Motion::Caret,
        Key::Char('$') | Key::End => Motion::Dollar,
        Key::Char('|') => Motion::Bar,
        Key::Char(c @ ('w' | 'W' | 'b' | 'B' | 'e' | 'E')) => Motion::Word(c),
        Key::Char(c @ ('H' | 'M' | 'L')) => Motion::ScreenLine(c),
        Key::Char('G') => Motion::Goto { first: false },
        Key::Char('%') => Motion::Percent,
        Key::Char('{') => Motion::Para { fwd: false },
        Key::Char('}') => Motion::Para { fwd: true },
        Key::Char('(') => Motion::Sent { fwd: false },
        Key::Char(')') => Motion::Sent { fwd: true },
        Key::Char(';') => Motion::RepeatFind { reverse: false },
        Key::Char(',') => Motion::RepeatFind { reverse: true },
        Key::Char('n') => Motion::SearchNext { reverse: false },
        Key::Char('N') => Motion::SearchNext { reverse: true },
        Key::Char('*') => Motion::SearchWord {
            fwd: true,
            whole: true,
        },
        Key::Char('#') => Motion::SearchWord {
            fwd: false,
            whole: true,
        },
        Key::Char(d @ ('/' | '?')) => {
            return match search_text(&k[1..]) {
                None => Ok(None),
                Some((_, n)) => Ok(Some((
                    Motion::Search {
                        fwd: d == '/',
                        klen: 1 + n,
                    },
                    1 + n,
                ))),
            };
        }
        Key::Char(q @ ('\'' | '`')) => {
            return match k.get(1) {
                None => Ok(None),
                // `''` and ``` `` ``` are the previous context mark.
                Some(Key::Char(c)) if c.is_ascii_lowercase() || matches!(c, '\'' | '`') => {
                    Ok(Some((
                        Motion::Mark {
                            mark: *c,
                            linewise: q == '\'',
                        },
                        2,
                    )))
                }
                _ => Err(()),
            };
        }
        Key::Char(f @ ('f' | 'F' | 't' | 'T')) => {
            let Some(c) = char_arg(k.get(1))? else {
                return Ok(None);
            };
            return Ok(Some((
                Motion::Find {
                    c,
                    fwd: f == 'f' || f == 't',
                    till: f == 't' || f == 'T',
                },
                2,
            )));
        }
        Key::Char('g') => {
            let m = match k.get(1) {
                None => return Ok(None),
                Some(Key::Char('g')) => Motion::Goto { first: true },
                Some(Key::Char('e')) => Motion::Word('g'),
                Some(Key::Char('E')) => Motion::Word('G'),
                Some(Key::Char('_')) => Motion::LastNonBlank,
                Some(Key::Char('*')) => Motion::SearchWord {
                    fwd: true,
                    whole: false,
                },
                Some(Key::Char('#')) => Motion::SearchWord {
                    fwd: false,
                    whole: false,
                },
                _ => return Err(()),
            };
            return Ok(Some((m, 2)));
        }
        _ => return Err(()),
    };
    Ok(Some((m, 1)))
}

/// Operator keys in Normal mode, with their length.
fn parse_operator(k: &[Key]) -> Res<(Op, usize)> {
    let op = match ch(k.first()) {
        Some('d') => Op::Delete,
        Some('c') => Op::Change,
        Some('y') => Op::Yank,
        Some('<') => Op::LShift,
        Some('>') => Op::RShift,
        Some('g') => {
            let op = match k.get(1) {
                None => return Ok(None),
                Some(Key::Char('~')) => Op::Tilde,
                Some(Key::Char('u')) => Op::Lower,
                Some(Key::Char('U')) => Op::Upper,
                Some(Key::Char('?')) => Op::Rot13,
                _ => return Err(()),
            };
            return Ok(Some((op, 2)));
        }
        _ => return Err(()),
    };
    Ok(Some((op, 1)))
}

/// Length of the doubled form of `op` at the start of `k` (`d` after `d`,
/// `~` or `g~` after `g~`).
fn doubled(op: Op, k: &[Key]) -> Res<usize> {
    let single = match op {
        Op::Delete => 'd',
        Op::Change => 'c',
        Op::Yank => 'y',
        Op::LShift => '<',
        Op::RShift => '>',
        Op::Tilde => '~',
        Op::Lower => 'u',
        Op::Upper => 'U',
        Op::Rot13 => '?',
        _ => return Err(()),
    };
    match ch(k.first()) {
        Some(c) if c == single => Ok(Some(1)),
        Some('g') if matches!(op, Op::Tilde | Op::Lower | Op::Upper | Op::Rot13) => {
            match ch(k.get(1)) {
                None if k.len() < 2 => Ok(None),
                Some(c) if c == single => Ok(Some(2)),
                _ => Err(()),
            }
        }
        _ => Err(()),
    }
}

fn parse_obj(k: &[Key]) -> Res<(Obj, usize)> {
    match ch(k.first()) {
        Some(c @ ('i' | 'a')) => match k.get(1) {
            None => Ok(None),
            Some(Key::Char(o)) if OBJ_CHARS.contains(*o) => Ok(Some((
                Obj {
                    kind: *o,
                    inner: c == 'i',
                },
                2,
            ))),
            _ => Err(()),
        },
        _ => Err(()),
    }
}

/// Parses everything after register and count. Returns the body, a second
/// count (after an operator) and the number of keys used.
fn parse_body(k: &[Key], visual: bool) -> Res<(Body, Option<usize>, usize)> {
    let Some(&k0) = k.first() else {
        return Ok(None);
    };
    let simple = |s: Simple, n: usize| Ok(Some((Body::Simple(s), None, n)));

    if visual {
        let vop = |op: Op, key: char, n: usize| Ok(Some((Body::VisOp { op, key }, None, n)));
        match k0 {
            Key::Char(c @ ('d' | 'x')) => return vop(Op::Delete, c, 1),
            Key::Delete => return vop(Op::Delete, 'd', 1),
            Key::Char(c @ ('X' | 'D')) => return vop(Op::Delete, c, 1),
            Key::Char(c @ ('y' | 'Y')) => return vop(Op::Yank, c, 1),
            Key::Char(c @ ('c' | 's' | 'C' | 'S' | 'R')) => return vop(Op::Change, c, 1),
            Key::Char('<') => return vop(Op::LShift, '<', 1),
            Key::Char('>') => return vop(Op::RShift, '>', 1),
            Key::Char('~') => return vop(Op::Tilde, '~', 1),
            Key::Char('u') => return vop(Op::Lower, 'u', 1),
            Key::Char('U') => return vop(Op::Upper, 'U', 1),
            Key::Char('J') => return vop(Op::Join, 'J', 1),
            Key::Char('I') => return vop(Op::Insert, 'I', 1),
            Key::Char('A') => return vop(Op::Append, 'A', 1),
            Key::Char('r') => {
                let c = match k.get(1) {
                    None => return Ok(None),
                    Some(Key::Char(c)) => *c,
                    Some(Key::Tab) => '\t',
                    _ => return Err(()),
                };
                return vop(Op::Replace(c), 'r', 2);
            }
            Key::Char('o') => return simple(Simple::VisOther { horiz: false }, 1),
            Key::Char('O') => return simple(Simple::VisOther { horiz: true }, 1),
            Key::Char('p') => {
                return simple(
                    Simple::Put {
                        before: false,
                        gp: false,
                    },
                    1,
                );
            }
            Key::Char('P') => {
                return simple(
                    Simple::Put {
                        before: true,
                        gp: false,
                    },
                    1,
                );
            }
            Key::Esc | Key::Ctrl('c') => return simple(Simple::Esc, 1),
            Key::Char('i' | 'a') => {
                let Some((o, n)) = parse_obj(k)? else {
                    return Ok(None);
                };
                return Ok(Some((Body::Obj(o), None, n)));
            }
            Key::Char('g') => match ch(k.get(1)) {
                None if k.len() < 2 => return Ok(None),
                Some('J') => return vop(Op::JoinNs, 'J', 2),
                Some('~') => return vop(Op::Tilde, '~', 2),
                Some('u') => return vop(Op::Lower, 'u', 2),
                Some('U') => return vop(Op::Upper, 'U', 2),
                Some('?') => return vop(Op::Rot13, '?', 2),
                Some('v') => return simple(Simple::Reselect, 2),
                Some('n') => return simple(Simple::Gn { fwd: true }, 2),
                Some('N') => return simple(Simple::Gn { fwd: false }, 2),
                _ => {}
            },
            _ => {}
        }
    }

    // Operators (Normal mode).
    if !visual {
        match parse_operator(k) {
            Ok(None) => return Ok(None),
            Ok(Some((op, oplen))) => {
                let mut j = oplen;
                let mut count2: Option<usize> = None;
                loop {
                    match k.get(j) {
                        None => return Ok(None),
                        Some(Key::Char(d @ '0'..='9')) if *d != '0' || count2.is_some() => {
                            let v = *d as usize - '0' as usize;
                            count2 = Some(count2.unwrap_or(0).saturating_mul(10).saturating_add(v));
                            j += 1;
                        }
                        _ => break,
                    }
                }
                let mut force = None;
                if let Some(f @ ('v' | 'V')) = ch(k.get(j)) {
                    force = Some(f);
                    j += 1;
                }
                if j >= k.len() {
                    return Ok(None);
                }
                if let Ok(r) = doubled(op, &k[j..]) {
                    let Some(n) = r else { return Ok(None) };
                    return Ok(Some((
                        Body::Op {
                            op,
                            force,
                            target: Target::Line,
                        },
                        count2,
                        j + n,
                    )));
                }
                if ch(k.get(j)) == Some('g') {
                    match k.get(j + 1) {
                        None => return Ok(None),
                        Some(Key::Char(c @ ('n' | 'N'))) => {
                            return Ok(Some((
                                Body::Op {
                                    op,
                                    force,
                                    target: Target::Gn { fwd: *c == 'n' },
                                },
                                count2,
                                j + 2,
                            )));
                        }
                        _ => {}
                    }
                }
                if matches!(ch(k.get(j)), Some('i' | 'a')) {
                    let Some((o, n)) = parse_obj(&k[j..])? else {
                        return Ok(None);
                    };
                    return Ok(Some((
                        Body::Op {
                            op,
                            force,
                            target: Target::Obj(o),
                        },
                        count2,
                        j + n,
                    )));
                }
                let Some((m, n)) = parse_motion(&k[j..])? else {
                    return Ok(None);
                };
                return Ok(Some((
                    Body::Op {
                        op,
                        force,
                        target: Target::Motion(m),
                    },
                    count2,
                    j + n,
                )));
            }
            Err(()) => {}
        }

        // Commands that are operators in disguise (Vim's nv_optrans).
        let opt = |op: Op, target: Target| {
            Ok(Some((
                Body::Op {
                    op,
                    force: None,
                    target,
                },
                None,
                1,
            )))
        };
        match k0 {
            Key::Char('x') | Key::Delete => {
                return opt(Op::Delete, Target::Motion(Motion::Right { space: false }));
            }
            Key::Char('X') => return opt(Op::Delete, Target::Motion(Motion::Left { bs: false })),
            Key::Char('D') => return opt(Op::Delete, Target::Motion(Motion::Dollar)),
            Key::Char('C') => return opt(Op::Change, Target::Motion(Motion::Dollar)),
            Key::Char('s') => {
                return opt(Op::Change, Target::Motion(Motion::Right { space: false }));
            }
            Key::Char('S') => return opt(Op::Change, Target::Line),
            Key::Char('Y') => return opt(Op::Yank, Target::Line),
            _ => {}
        }
    }

    // Commands common to both modes.
    match k0 {
        Key::Char('v') => return simple(Simple::Visual(VisualMode::Char), 1),
        Key::Char('V') => return simple(Simple::Visual(VisualMode::Line), 1),
        Key::Ctrl('v') => return simple(Simple::Visual(VisualMode::Block), 1),
        Key::Ctrl('g') => return simple(Simple::FileInfo, 1),
        Key::Ctrl('l') => return simple(Simple::Redraw, 1),
        Key::Char(':') => return simple(Simple::CmdLine, 1),
        Key::Char('m') => {
            return match k.get(1) {
                None => Ok(None),
                Some(Key::Char(c)) if c.is_ascii_lowercase() || matches!(c, '\'' | '`') => {
                    simple(Simple::SetMark(*c), 2)
                }
                _ => Err(()),
            };
        }
        Key::Ctrl('o') => return simple(Simple::JumpOlder, 1),
        Key::Tab => return simple(Simple::JumpNewer, 1),
        Key::Ctrl('e') => return simple(Simple::Scroll(ScrollCmd::LineDown), 1),
        Key::Ctrl('y') => return simple(Simple::Scroll(ScrollCmd::LineUp), 1),
        Key::Ctrl('f') => return simple(Simple::Scroll(ScrollCmd::PageDown), 1),
        Key::Ctrl('b') => return simple(Simple::Scroll(ScrollCmd::PageUp), 1),
        Key::Ctrl('d') => return simple(Simple::Scroll(ScrollCmd::HalfDown), 1),
        Key::Ctrl('u') => return simple(Simple::Scroll(ScrollCmd::HalfUp), 1),
        Key::Char('z') => {
            let cmd = match k.get(1) {
                None => return Ok(None),
                Some(Key::Enter) => ScrollCmd::Top {
                    first_non_blank: true,
                },
                Some(Key::Char(c)) => match c {
                    't' => ScrollCmd::Top {
                        first_non_blank: false,
                    },
                    'z' => ScrollCmd::Middle {
                        first_non_blank: false,
                    },
                    'b' => ScrollCmd::Bottom {
                        first_non_blank: false,
                    },
                    '.' => ScrollCmd::Middle {
                        first_non_blank: true,
                    },
                    '-' => ScrollCmd::Bottom {
                        first_non_blank: true,
                    },
                    _ => return Err(()),
                },
                _ => return Err(()),
            };
            return simple(Simple::Scroll(cmd), 2);
        }
        _ => {}
    }

    if !visual {
        match k0 {
            Key::Char(c @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')) => {
                return simple(Simple::Insert(c), 1);
            }
            Key::Char('R') => return simple(Simple::ReplaceMode, 1),
            Key::Char('r') => {
                let c = match k.get(1) {
                    None => return Ok(None),
                    Some(Key::Char(c)) => *c,
                    Some(Key::Tab) => '\t',
                    Some(Key::Enter) => '\n',
                    _ => return Err(()),
                };
                return simple(Simple::ReplaceChar(c), 2);
            }
            Key::Char('J') => return simple(Simple::Join { spaces: true }, 1),
            Key::Char('p') => {
                return simple(
                    Simple::Put {
                        before: false,
                        gp: false,
                    },
                    1,
                );
            }
            Key::Char('P') => {
                return simple(
                    Simple::Put {
                        before: true,
                        gp: false,
                    },
                    1,
                );
            }
            Key::Char('~') => return simple(Simple::SwapCase, 1),
            Key::Char('u') => return simple(Simple::Undo, 1),
            Key::Ctrl('r') => return simple(Simple::Redo, 1),
            Key::Char('U') => return simple(Simple::UndoLine, 1),
            Key::Char('.') => return simple(Simple::Dot, 1),
            Key::Char('&') => return simple(Simple::Ampersand { all: false }, 1),
            Key::Char('q') => {
                return match k.get(1) {
                    None => Ok(None),
                    Some(Key::Char(c)) if c.is_ascii_alphanumeric() || *c == '"' => {
                        simple(Simple::Record(*c), 2)
                    }
                    _ => Err(()),
                };
            }
            Key::Char('@') => {
                return match k.get(1) {
                    None => Ok(None),
                    Some(Key::Char(c)) if c.is_ascii_alphanumeric() || "@:.-_\"".contains(*c) => {
                        simple(Simple::Macro(*c), 2)
                    }
                    _ => Err(()),
                };
            }
            Key::Esc => return simple(Simple::Esc, 1),
            Key::Char('Z') => {
                return match ch(k.get(1)) {
                    None if k.len() < 2 => Ok(None),
                    Some('Z') => simple(Simple::WriteQuit, 2),
                    Some('Q') => simple(Simple::Quit, 2),
                    _ => Err(()),
                };
            }
            Key::Char('g') => match ch(k.get(1)) {
                None if k.len() < 2 => return Ok(None),
                Some('J') => return simple(Simple::Join { spaces: false }, 2),
                Some('&') => return simple(Simple::Ampersand { all: true }, 2),
                Some('p') => {
                    return simple(
                        Simple::Put {
                            before: false,
                            gp: true,
                        },
                        2,
                    );
                }
                Some('P') => {
                    return simple(
                        Simple::Put {
                            before: true,
                            gp: true,
                        },
                        2,
                    );
                }
                Some('v') => return simple(Simple::Reselect, 2),
                Some('I') => return simple(Simple::Insert('g'), 2),
                Some('n') => return simple(Simple::Gn { fwd: true }, 2),
                Some('N') => return simple(Simple::Gn { fwd: false }, 2),
                _ => {}
            },
            _ => {}
        }
    }

    let Some((m, n)) = parse_motion(k)? else {
        return Ok(None);
    };
    Ok(Some((Body::Motion(m), None, n)))
}

/// Parses a complete command from the keys typed so far.
pub(crate) fn parse(keys: &[Key], visual: bool) -> Parse {
    let mut i = 0;
    let mut reg = None;
    let mut count: Option<usize> = None;
    loop {
        match keys.get(i) {
            None => return Parse::Incomplete,
            Some(Key::Char('"')) => match keys.get(i + 1) {
                None => return Parse::Incomplete,
                Some(Key::Char(c)) if valid_name(*c) => {
                    reg = Some(*c);
                    i += 2;
                }
                _ => return Parse::Invalid,
            },
            Some(Key::Char(d @ '0'..='9')) if *d != '0' || count.is_some() => {
                let v = *d as usize - '0' as usize;
                count = Some(count.unwrap_or(0).saturating_mul(10).saturating_add(v));
                i += 1;
            }
            // <Del> while typing a count removes its last digit.
            Some(Key::Delete) if count.is_some() => {
                count = count.map(|c| c / 10).filter(|&c| c > 0);
                i += 1;
            }
            _ => break,
        }
    }
    match parse_body(&keys[i..], visual) {
        Ok(None) => Parse::Incomplete,
        Err(()) => Parse::Invalid,
        Ok(Some((body, count2, n))) => {
            let count = match (count, count2) {
                (Some(a), Some(b)) => Some(a.saturating_mul(b)),
                (a, b) => a.or(b),
            };
            let used = &keys[i..i + n];
            let search = match &body {
                Body::Motion(Motion::Search { klen, .. })
                | Body::Op {
                    target: Target::Motion(Motion::Search { klen, .. }),
                    ..
                } => search_text(&used[n - klen + 1..]).map(|(t, _)| t),
                _ => None,
            };
            Parse::Done(Cmd {
                reg,
                count,
                body,
                keys: used.to_vec(),
                search,
            })
        }
    }
}

/// What a motion did, for an operator (Vim's `oparg_T` fields).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MInfo {
    pub linewise: bool,
    pub inclusive: bool,
    /// Vim's CA_NO_ADJ_OP_END.
    pub no_adj: bool,
    pub use_reg_one: bool,
}

impl Editor {
    pub(crate) fn is_visual(&self) -> bool {
        matches!(self.mode, Mode::Visual(_))
    }

    pub(crate) fn visual_mode(&self) -> Option<VisualMode> {
        match self.mode {
            Mode::Visual(m) => Some(m),
            _ => None,
        }
    }

    /// Vim's `coladvance()`: in Insert and Visual mode the cursor may stop
    /// on the end-of-line position.
    pub(crate) fn coladvance(&mut self, want: usize) {
        let allow_end = matches!(self.mode, Mode::Insert | Mode::Replace | Mode::Visual(_));
        self.cursor.col = text::col_for_vcol(
            self.buf.line(self.cursor.line),
            want,
            allow_end,
            self.opts.tabstop(),
        );
    }

    fn fnb(&mut self) {
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
    }

    pub(crate) fn cursor_down_n(&mut self, n: usize) -> bool {
        if n == 0 {
            return true;
        }
        if self.cursor.line >= self.buf.last_line() {
            return false;
        }
        self.cursor.line = (self.cursor.line + n).min(self.buf.last_line());
        true
    }

    pub(crate) fn cursor_up_n(&mut self, n: usize) -> bool {
        if n == 0 {
            return true;
        }
        if self.cursor.line == 0 {
            return false;
        }
        self.cursor.line = self.cursor.line.saturating_sub(n);
        true
    }

    /// Applies the line edits made since the last call to the jumplist and
    /// the context mark (see `PosEdit`).
    pub(crate) fn sync_jumps(&mut self) {
        for edit in self.buf.take_pos_edits() {
            self.jumps.apply(&edit);
        }
    }

    /// Vim's setpcmark(): the cursor becomes the previous context mark and
    /// is added to the jumplist. Inside `:global` it does nothing, since
    /// `:global` sets it once.
    pub(crate) fn set_pcmark(&mut self) {
        if self.global_busy {
            return;
        }
        self.sync_jumps();
        self.jumps.set_pcmark(self.cursor);
    }

    /// `<C-o>` and `<Tab>` (Vim's movemark() through nv_pcmark()). In Visual
    /// mode they do nothing, as in Vim.
    fn move_in_jumplist(&mut self, count: isize) {
        if self.is_visual() {
            self.keep_curswant = true;
            return;
        }
        self.sync_jumps();
        let Some(p) = self.jumps.move_by(count, self.cursor, !self.global_busy) else {
            self.keep_curswant = true;
            self.beep();
            return;
        };
        if p.line >= self.buf.line_count() {
            self.keep_curswant = true;
            self.error("E19: Mark has invalid line number");
            return;
        }
        self.cursor = Pos::new(p.line, p.col);
    }

    /// Runs a motion, moving the cursor. `None` means "clear the operator
    /// and beep" (Vim's `clearopbeep`). `op` is the pending operator.
    pub(crate) fn do_motion(
        &mut self,
        m: Motion,
        count0: Option<usize>,
        op: Option<Op>,
    ) -> Option<MInfo> {
        let count1 = count0.unwrap_or(1).max(1);
        let visual = self.is_visual();
        let mut info = MInfo::default();
        match m {
            Motion::Left { bs } => {
                let mut n = count1;
                while n > 0 {
                    if self.cursor.col == 0 {
                        if bs && self.cursor.line > 0 {
                            self.cursor.line -= 1;
                            self.coladvance(usize::MAX);
                            if matches!(op, Some(Op::Delete | Op::Change))
                                && self.buf.line_len(self.cursor.line) > 0
                            {
                                if gchar(&self.buf, self.cursor).is_some() {
                                    self.cursor.col += 1;
                                }
                                info.no_adj = true;
                            }
                            n -= 1;
                            continue;
                        } else if op.is_none() && n == count1 {
                            // oneleft() failed: the desired column stays.
                            self.keep_curswant = true;
                            return None;
                        }
                        break;
                    }
                    self.cursor.col -= 1;
                    n -= 1;
                }
            }
            Motion::Right { space } => {
                let mut n = count1;
                while n > 0 {
                    let len = self.buf.line_len(self.cursor.line);
                    let fail = if visual {
                        self.cursor.col >= len
                    } else {
                        self.cursor.col + 1 >= len
                    };
                    if fail {
                        if space && self.cursor.line < self.buf.last_line() {
                            if op.is_some() && !info.inclusive && len > 0 {
                                info.inclusive = true;
                            } else {
                                self.cursor.line += 1;
                                self.cursor.col = 0;
                                info.inclusive = false;
                            }
                            n -= 1;
                            continue;
                        }
                        if op.is_none() {
                            if n == count1 {
                                self.keep_curswant = true;
                                return None;
                            }
                        } else if len > 0 {
                            info.inclusive = true;
                        }
                        break;
                    }
                    self.cursor.col += 1;
                    n -= 1;
                }
            }
            Motion::Up | Motion::Down => {
                info.linewise = true;
                self.keep_curswant = true;
                let ok = if m == Motion::Down {
                    self.cursor_down_n(count1)
                } else {
                    self.cursor_up_n(count1)
                };
                if !ok {
                    return None;
                }
                self.coladvance(self.curswant);
            }
            Motion::UpFnb | Motion::DownFnb => {
                info.linewise = true;
                let ok = if m == Motion::DownFnb {
                    self.cursor_down_n(count1)
                } else {
                    self.cursor_up_n(count1)
                };
                if !ok {
                    return None;
                }
                self.fnb();
            }
            Motion::Underscore => return self.line_motion(count1, op),
            Motion::Zero => self.cursor.col = 0,
            Motion::Caret => self.fnb(),
            Motion::Dollar => {
                info.inclusive = true;
                self.curswant = usize::MAX;
                self.keep_curswant = true;
                if !self.cursor_down_n(count1 - 1) {
                    return None;
                }
                self.coladvance(usize::MAX);
            }
            Motion::Bar => {
                self.curswant = count0.map_or(0, |c| c - 1);
                self.keep_curswant = true;
                self.coladvance(self.curswant);
            }
            Motion::LastNonBlank => {
                info.inclusive = true;
                self.curswant = usize::MAX;
                if !self.cursor_down_n(count1 - 1) {
                    return None;
                }
                self.coladvance(usize::MAX);
                let line = self.buf.line(self.cursor.line);
                let len = line.len_chars();
                if self.cursor.col > 0 && self.cursor.col >= len {
                    self.cursor.col -= 1;
                }
                while self.cursor.col > 0 && matches!(line.char(self.cursor.col), ' ' | '\t') {
                    self.cursor.col -= 1;
                }
            }
            Motion::Word(c) => return self.word_motion(c, count1, op),
            Motion::ScreenLine(which) => {
                info.linewise = true;
                // H, M and L are jumps (Vim's nv_scroll()).
                self.set_pcmark();
                self.cursor.line = self.screen_line(which, count1);
                self.fnb();
            }
            Motion::Goto { first } => {
                info.linewise = true;
                // Vim's nv_goto(): a jump even if the cursor does not move.
                self.set_pcmark();
                let line = match count0 {
                    Some(c) => c.saturating_sub(1),
                    None if first => 0,
                    None => self.buf.last_line(),
                };
                self.cursor.line = line.min(self.buf.last_line());
                self.fnb();
            }
            Motion::Percent => {
                info.inclusive = true;
                if let Some(c) = count0 {
                    if c > 100 {
                        return None;
                    }
                    info.linewise = true;
                    self.set_pcmark();
                    let n = self.buf.line_count();
                    let line = (n * c).div_ceil(100).clamp(1, n);
                    self.cursor.line = line - 1;
                    self.fnb();
                } else {
                    info.use_reg_one = true;
                    let r = findmatchlimit(&self.buf, self.cursor, None, None, false)?;
                    self.set_pcmark();
                    self.cursor = r.pos;
                    info.linewise = r.linewise;
                }
            }
            Motion::Para { fwd } => {
                info.use_reg_one = true;
                let (p, incl) = findpar(&self.buf, self.cursor, fwd, count1, '\0', false)?;
                self.set_pcmark();
                self.cursor = p;
                info.inclusive = incl;
            }
            Motion::Sent { fwd } => {
                info.use_reg_one = true;
                let p = findsent(&self.buf, self.cursor, fwd, count1)?;
                self.set_pcmark();
                self.cursor = p;
                self.adjust_cursor(&mut info);
            }
            Motion::Find { c, fwd, till } => {
                self.last_find = Some((c, fwd, till));
                info.inclusive = fwd;
                self.cursor.col = searchc(&self.buf, self.cursor, c, fwd, till, count1, true)?;
            }
            Motion::Mark { mark, linewise } => {
                let target = if matches!(mark, '\'' | '`') {
                    self.sync_jumps();
                    self.jumps.pcmark()
                } else {
                    self.buf.mark(mark)
                };
                let Some(p) = target else {
                    self.error("E20: Mark not set");
                    return None;
                };
                // Like Vim's nv_cursormark(): jumping to a mark sets the
                // context mark, which is copied above first.
                self.set_pcmark();
                // Column clamping: Normal mode clamps the cursor after the
                // command, an operator may take the end of the line.
                if linewise {
                    info.linewise = true;
                    self.cursor.line = p.line.min(self.buf.last_line());
                    self.fnb();
                } else {
                    // Like Vim's nv_cursormark(): a backtick motion also
                    // uses register 1 when it deletes.
                    info.use_reg_one = true;
                    let line = p.line.min(self.buf.last_line());
                    self.cursor = Pos::new(line, p.col.min(self.buf.line_len(line)));
                }
            }
            Motion::RepeatFind { reverse } => {
                let (c, fwd, till) = self.last_find?;
                let dir = if reverse { !fwd } else { fwd };
                let stop = !(count1 == 1 && till);
                info.inclusive = dir;
                self.cursor.col = searchc(&self.buf, self.cursor, c, dir, till, count1, stop)?;
            }
            Motion::Search { fwd, .. } => {
                let pat = self.cmd_search.take().unwrap_or_default();
                return self.search_motion(Some(if fwd { '/' } else { '?' }), Some(pat), count1);
            }
            Motion::SearchNext { reverse } => {
                let fwd = self.search.fwd != reverse;
                let dirc = if fwd { '/' } else { '?' };
                let saved = self.search.fwd;
                let r = self.search_motion(Some(dirc), None, count1);
                // `N` does not change the direction `n` uses.
                self.search.fwd = saved;
                return r;
            }
            Motion::SearchWord { fwd, whole } => {
                let Some((col, word)) = self.ident_under_cursor() else {
                    self.error("E348: No string under cursor");
                    return None;
                };
                let esc = if fwd { "/.*~[^$\\" } else { "/?.*~[^$\\" };
                let mut pat = String::new();
                let is_word = |c: char| crate::motion::char_class(c) >= 2;
                if whole && word.chars().next().is_some_and(is_word) {
                    pat.push_str(r"\<");
                }
                for c in word.chars() {
                    if esc.contains(c) {
                        pat.push('\\');
                    }
                    pat.push(c);
                }
                if whole && word.chars().last().is_some_and(is_word) {
                    pat.push_str(r"\>");
                }
                self.cursor.col = col;
                let saved = self.cursor;
                crate::cmdline::push_history(&mut self.search_hist, &pat);
                let r = self.search_motion(Some(if fwd { '/' } else { '?' }), Some(pat), count1);
                if r.is_none() {
                    self.cursor = saved;
                }
                return r;
            }
        }
        Some(info)
    }

    /// `/ ? n N * #`: Vim's `normal_search()`.
    fn search_motion(
        &mut self,
        dirc: Option<char>,
        pat: Option<String>,
        count1: usize,
    ) -> Option<MInfo> {
        let (pos, linewise, inclusive) = self.do_search(dirc, pat.as_deref(), count1)?;
        self.set_pcmark();
        self.cursor = pos;
        // "/$" can end after the last char.
        if !self.is_visual() {
            let len = self.buf.line_len(self.cursor.line);
            if self.cursor.col >= len {
                self.cursor.col = len.saturating_sub(1);
            }
        }
        Some(MInfo {
            linewise,
            inclusive,
            use_reg_one: true,
            ..MInfo::default()
        })
    }

    /// Vim's `adjust_cursor()`: leave the end-of-line position (not in
    /// Visual mode), making the motion inclusive.
    fn adjust_cursor(&mut self, info: &mut MInfo) {
        if self.cursor.col > 0 && gchar(&self.buf, self.cursor).is_none() && !self.is_visual() {
            self.cursor.col -= 1;
            info.inclusive = true;
        }
    }

    /// `_` and doubled operators (Vim's `nv_lineop`).
    pub(crate) fn line_motion(&mut self, count1: usize, op: Option<Op>) -> Option<MInfo> {
        if !self.cursor_down_n(count1 - 1) {
            return None;
        }
        if op != Some(Op::Yank) {
            self.fnb();
        }
        Some(MInfo {
            linewise: true,
            ..MInfo::default()
        })
    }

    /// `w W e E b B ge gE` (Vim's `nv_wordcmd`, `nv_bck_word`).
    fn word_motion(&mut self, c: char, count1: usize, op: Option<Op>) -> Option<MInfo> {
        let mut info = MInfo::default();
        let big = c.is_ascii_uppercase();
        match c.to_ascii_lowercase() {
            'b' => {
                let mut cur = Cur::new(&self.buf, self.cursor, big);
                let ok = cur.bck_word(count1, false);
                self.cursor = cur.p;
                if !ok {
                    return None;
                }
            }
            'g' => {
                info.inclusive = true;
                let mut cur = Cur::new(&self.buf, self.cursor, big);
                let ok = cur.bckend_word(count1, false);
                self.cursor = cur.p;
                if !ok {
                    return None;
                }
            }
            _ => {
                let mut word_end = c == 'e' || c == 'E';
                info.inclusive = word_end;
                let mut flag = false;
                if !word_end
                    && op == Some(Op::Change)
                    && let Some(ch) = gchar(&self.buf, self.cursor)
                    && ch != ' '
                    && ch != '\t'
                {
                    // "cw" works like "ce" ('cpoptions' contains 'z').
                    info.inclusive = true;
                    word_end = true;
                    flag = true;
                }
                let start = self.cursor;
                let mut cur = Cur::new(&self.buf, self.cursor, big);
                let ok = if word_end {
                    cur.end_word(count1, flag, false)
                } else {
                    cur.fwd_word(count1, op.is_some())
                };
                self.cursor = cur.p;
                if start < self.cursor {
                    self.adjust_cursor(&mut info);
                }
                if !ok && op.is_none() {
                    return None;
                }
            }
        }
        Some(info)
    }

    /// Selects a text object. Returns the start and motion info; the cursor
    /// moves to the end.
    pub(crate) fn text_object(&mut self, o: Obj, count1: usize) -> Option<(Pos, MInfo)> {
        let visual = self.visual_mode().map(|m| VisualCtx {
            start: self.vstart,
            linewise: m == VisualMode::Line,
        });
        let buf = &self.buf;
        let cur = self.cursor;
        let include = !o.inner;
        let out: ObjOut = match o.kind {
            'w' | 'W' => textobject::current_word(buf, cur, visual, count1, include, o.kind == 'W'),
            's' => textobject::current_sent(buf, cur, visual, count1, include),
            'p' => textobject::current_par(buf, cur, visual, count1, include),
            '(' | ')' | 'b' => {
                textobject::current_block(buf, cur, visual, count1, include, '(', ')')
            }
            '{' | '}' | 'B' => {
                textobject::current_block(buf, cur, visual, count1, include, '{', '}')
            }
            '[' | ']' => textobject::current_block(buf, cur, visual, count1, include, '[', ']'),
            '<' | '>' => textobject::current_block(buf, cur, visual, count1, include, '<', '>'),
            '"' | '\'' | '`' => {
                textobject::current_quote(buf, cur, visual, count1, include, o.kind)
            }
            _ => None,
        }?;
        self.cursor = out.cursor;
        if let Some(m) = self.visual_mode() {
            let new_mode = match o.kind {
                'p' => {
                    if out.linewise {
                        VisualMode::Line
                    } else {
                        m
                    }
                }
                's' => {
                    if out.linewise {
                        VisualMode::Line
                    } else {
                        VisualMode::Char
                    }
                }
                'w' | 'W' | '"' | '\'' | '`' => {
                    if m == VisualMode::Line {
                        VisualMode::Char
                    } else {
                        m
                    }
                }
                _ => VisualMode::Char,
            };
            self.vstart = out.start;
            self.mode = Mode::Visual(new_mode);
        }
        Some((
            out.start,
            MInfo {
                linewise: out.linewise,
                inclusive: out.inclusive,
                ..MInfo::default()
            },
        ))
    }

    // ---- dispatch -------------------------------------------------------

    /// A key in Normal or Visual mode.
    pub(crate) fn normal_key(&mut self, key: Key) {
        self.pending.push(key);
        self.run_pending(key);
    }

    /// Runs the pending keys if they now form a command. `key` is the last
    /// key typed.
    pub(crate) fn run_pending(&mut self, key: Key) {
        match parse(&self.pending, self.is_visual()) {
            Parse::Incomplete => {
                self.keep_curswant = true;
                self.search_prompt_start(key);
            }
            Parse::Invalid => {
                self.pending.clear();
                self.keep_curswant = true;
                if !matches!(key, Key::Esc | Key::Backspace | Key::Ctrl('c')) {
                    self.beep();
                }
            }
            Parse::Done(cmd) => {
                self.pending.clear();
                self.cmd_search = cmd.search.clone();
                if self.is_visual() {
                    self.exec_visual(cmd);
                } else {
                    self.exec_normal(cmd);
                }
            }
        }
    }

    fn is_change(body: &Body) -> bool {
        match body {
            Body::Op { op, .. } => *op != Op::Yank,
            Body::Simple(s) => matches!(
                s,
                Simple::Insert(_)
                    | Simple::ReplaceChar(_)
                    | Simple::ReplaceMode
                    | Simple::Join { .. }
                    | Simple::Put { .. }
                    | Simple::SwapCase
            ),
            _ => false,
        }
    }

    fn exec_normal(&mut self, cmd: Cmd) {
        if self.replaying_visual.is_none() && Self::is_change(&cmd.body) {
            self.redo_building = Some(Redo {
                reg: cmd.reg,
                count: cmd.count,
                keys: cmd.keys.clone(),
                visual: None,
            });
        }
        match cmd.body {
            Body::Motion(m) => {
                if self.do_motion(m, cmd.count, None).is_none() {
                    self.beep();
                }
            }
            Body::Op { op, force, target } => {
                self.op_command(cmd.reg, cmd.count, op, force, target)
            }
            Body::Simple(s) => self.simple(s, cmd.reg, cmd.count),
            Body::VisOp { .. } | Body::Obj(_) => self.beep(),
        }
        self.commit_redo_if_done();
    }

    /// Commits the redo being built unless an insert is still collecting
    /// keys for it.
    pub(crate) fn commit_redo_if_done(&mut self) {
        if !matches!(self.mode, Mode::Insert | Mode::Replace)
            && let Some(r) = self.redo_building.take()
        {
            self.redo = Some(r);
        }
    }

    /// An operator with a motion, text object or doubled (Normal mode).
    fn op_command(
        &mut self,
        reg: Option<char>,
        count: Option<usize>,
        op: Op,
        force: Option<char>,
        target: Target,
    ) {
        let start = self.cursor;
        let count1 = count.unwrap_or(1).max(1);
        if let Target::Gn { fwd } = target {
            if !self.current_search(count1, fwd) {
                self.beep();
                self.redo_building = None;
                return;
            }
            // The key only matters for the linewise variants (X Y D C S R),
            // which operators from `parse_operator` never are.
            let key = match op {
                Op::Delete => 'd',
                Op::Change => 'c',
                Op::Yank => 'y',
                Op::LShift => '<',
                Op::RShift => '>',
                Op::Tilde => '~',
                Op::Lower => 'u',
                Op::Upper => 'U',
                Op::Rot13 => '?',
                _ => '\0',
            };
            self.visual_operator(op, key, reg, None);
            // Vim repeats "gn" as typed, not as a Visual operation.
            if let Some(r) = &mut self.redo_building {
                r.visual = None;
            }
            return;
        }
        let res = match target {
            Target::Motion(m) => self.do_motion(m, count, Some(op)).map(|i| (start, i)),
            Target::Line => self.line_motion(count1, Some(op)).map(|i| (start, i)),
            Target::Obj(o) => self.text_object(o, count1),
            Target::Gn { .. } => unreachable!("handled above"),
        };
        let Some((start, info)) = res else {
            self.beep();
            self.redo_building = None;
            return;
        };
        self.pending_operator(op, reg, start, info, force);
    }

    fn simple(&mut self, s: Simple, reg: Option<char>, count: Option<usize>) {
        let count1 = count.unwrap_or(1).max(1);
        match s {
            Simple::Insert(c) => self.insert_command(c, count1),
            Simple::ReplaceChar(c) => self.replace_char(c, count1),
            Simple::ReplaceMode => {
                self.start_insert(count1, crate::editor::InsertKind::Inline, true);
            }
            Simple::Join { spaces } => {
                let mut n = count.unwrap_or(0).max(2);
                if self.cursor.line + n - 1 > self.buf.last_line() {
                    if n <= 2 {
                        self.beep();
                        self.redo_building = None;
                        return;
                    }
                    n = self.buf.line_count() - self.cursor.line;
                }
                if let Some(r) = &mut self.redo_building {
                    r.count = Some(n);
                }
                self.do_join(n, spaces, true);
            }
            Simple::Put { before, gp } => {
                self.do_put(
                    reg,
                    before,
                    count1,
                    crate::ops::PutFlags {
                        cursend: gp,
                        ..Default::default()
                    },
                );
            }
            Simple::SwapCase => self.swap_case_cmd(count1),
            Simple::Undo => self.undo_cmd(count1, false),
            Simple::Redo => self.undo_cmd(count1, true),
            Simple::UndoLine => self.undo_line(),
            Simple::Dot => self.dot(count),
            Simple::Visual(m) => {
                self.vstart = self.cursor;
                self.message = None;
                if let (Some(c), Some(r)) = (count, self.resel) {
                    self.reselect_size(r, c);
                    return;
                }
                self.mode = Mode::Visual(m);
                if count1 > 1 {
                    let n = Some(count1 - 1);
                    match m {
                        VisualMode::Line => {
                            self.do_motion(Motion::Down, n, None);
                        }
                        _ => {
                            self.do_motion(Motion::Right { space: false }, n, None);
                        }
                    }
                }
            }
            Simple::Reselect => self.reselect(),
            Simple::CmdLine => {
                self.keep_curswant = true;
                let was_visual = self.is_visual();
                if was_visual {
                    let mut start = self.vstart.min(self.cursor);
                    if self.visual_mode() == Some(VisualMode::Line) {
                        start.col = 0;
                    }
                    self.end_visual();
                    self.cursor = start;
                }
                let initial = if was_visual {
                    "'<,'>".to_string()
                } else if let Some(c) = count {
                    if c == 1 {
                        ".".to_string()
                    } else {
                        format!(".,.+{}", c - 1)
                    }
                } else {
                    String::new()
                };
                self.cmd_start(crate::cmdline::PromptKind::Ex, Mode::Normal, &initial);
            }
            Simple::SetMark(c) => {
                // Like Vim's nv_mark(): setting a mark keeps the desired column.
                self.keep_curswant = true;
                if matches!(c, '\'' | '`') {
                    // Vim's setmark_pos(): the context mark is kept even when
                    // the cursor does not move.
                    self.set_pcmark();
                    self.jumps.keep_pcmark();
                } else {
                    self.buf.set_mark(c, self.cursor);
                }
            }
            Simple::JumpOlder => self.move_in_jumplist(-(count1 as isize)),
            Simple::JumpNewer => self.move_in_jumplist(count1 as isize),
            Simple::Record(c) => self.start_recording(c),
            Simple::Macro(c) => self.exec_register(c, count1),
            Simple::Ampersand { all } => self.execute_ex(if all { "%s//~/&" } else { "s" }),
            Simple::WriteQuit => self.execute_ex("x"),
            Simple::Quit => self.execute_ex("q!"),
            Simple::FileInfo => {
                self.keep_curswant = true;
                self.file_info();
            }
            Simple::Redraw => self.keep_curswant = true,
            Simple::Scroll(cmd) => self.scroll_cmd(cmd, count),
            Simple::Esc => {
                self.keep_curswant = true;
                if self.is_visual() {
                    self.end_visual();
                } else if count.is_none() && reg.is_none() {
                    self.beep();
                }
            }
            Simple::VisOther { horiz } => self.visual_other(horiz),
            Simple::Gn { fwd } => {
                if !self.current_search(count1, fwd) {
                    self.beep();
                }
            }
        }
    }

    /// `i a I A o O gI`.
    fn insert_command(&mut self, c: char, count1: usize) {
        use crate::editor::InsertKind;
        match c {
            'i' => {}
            'a' => {
                if gchar(&self.buf, self.cursor).is_some() {
                    self.cursor.col += 1;
                }
            }
            'I' => self.cursor.col = first_non_blank(&self.buf, self.cursor.line, false),
            'g' => self.cursor.col = 0,
            'A' => self.cursor.col = self.buf.line_len(self.cursor.line),
            'o' | 'O' => {
                let below = c == 'o';
                let at = self.cursor.line + usize::from(below);
                self.u_save(at, 0, false);
                self.uline = None;
                self.buf.insert_line(at, "");
                self.cursor = Pos::new(at, 0);
                self.auto_indent(at, if below { at - 1 } else { at + 1 });
                self.start_insert(count1, InsertKind::OpenLine, false);
                return;
            }
            _ => {}
        }
        self.start_insert(count1, InsertKind::Inline, false);
    }

    /// `r{char}` (Vim's `nv_replace`).
    fn replace_char(&mut self, c: char, count1: usize) {
        let len = self.buf.line_len(self.cursor.line);
        if len - self.cursor.col.min(len) < count1 {
            self.beep();
            self.redo_building = None;
            return;
        }
        self.u_save_cursor();
        let Pos { line, col } = self.cursor;
        if c == '\n' {
            self.buf.remove(line, col, count1);
            self.uline = None;
            self.buf.insert(line, col, "\n");
            self.cursor = Pos::new(line + 1, 0);
            return;
        }
        let mut chars: Vec<char> = self.buf.line(line).chars().collect();
        for ch in &mut chars[col..col + count1] {
            *ch = c;
        }
        self.buf.set_line(line, &chars.iter().collect::<String>());
        self.op_start = self.cursor;
        self.cursor.col = col + count1 - 1;
        self.op_end = self.cursor;
    }

    /// `~` in Normal mode ('notildeop', Vim's `n_swapchar`).
    fn swap_case_cmd(&mut self, count1: usize) {
        if self.buf.line_len(self.cursor.line) == 0 {
            self.beep();
            self.redo_building = None;
            return;
        }
        self.u_save_cursor();
        let start = self.cursor;
        for _ in 0..count1 {
            self.swapchar(Op::Tilde, self.cursor);
            motion::inc(&self.buf, &mut self.cursor);
            if gchar(&self.buf, self.cursor).is_none() {
                break;
            }
        }
        self.op_start = start;
        self.op_end = self.cursor;
    }

    // ---- Visual mode ----------------------------------------------------

    fn exec_visual(&mut self, cmd: Cmd) {
        match cmd.body {
            Body::Motion(m) => {
                if self.do_motion(m, cmd.count, None).is_none() {
                    self.beep();
                }
            }
            Body::Obj(o) => {
                if self.text_object(o, cmd.count.unwrap_or(1).max(1)).is_none() {
                    self.beep();
                }
            }
            Body::VisOp { op, key } => {
                if op != Op::Yank && self.replaying_visual.is_none() {
                    self.redo_building = Some(Redo {
                        reg: cmd.reg,
                        count: None,
                        keys: cmd.keys.clone(),
                        visual: None,
                    });
                }
                self.visual_operator(op, key, cmd.reg, cmd.count);
                self.commit_redo_if_done();
            }
            Body::Simple(Simple::Put { before, .. }) => {
                self.visual_put(cmd.reg, before, cmd.count.unwrap_or(1).max(1));
            }
            Body::Simple(Simple::Visual(m)) => {
                self.keep_curswant = true;
                if self.visual_mode() == Some(m) {
                    self.end_visual();
                } else {
                    self.mode = Mode::Visual(m);
                }
            }
            Body::Simple(s) => self.simple(s, cmd.reg, cmd.count),
            Body::Op { .. } => self.beep(),
        }
    }

    /// Leaves Visual mode, remembering the area for `gv`.
    pub(crate) fn end_visual(&mut self) {
        if let Some(m) = self.visual_mode() {
            self.last_visual = Some(crate::editor::LastVisual {
                start: self.vstart,
                end: self.cursor,
                mode: m,
                curswant: self.curswant,
            });
            self.mode = Mode::Normal;
        }
    }

    /// `{count}v`: select `count` times the size of the last Visual
    /// operation (Vim's `nv_visual()` with `resel_VIsual_mode`).
    fn reselect_size(&mut self, r: VisualRedo, count: usize) {
        self.mode = Mode::Visual(r.mode);
        if r.mode != VisualMode::Char || r.line_count > 1 {
            self.cursor.line =
                (self.cursor.line + r.line_count * count - 1).min(self.buf.last_line());
        }
        if r.mode == VisualMode::Char {
            if r.line_count <= 1 {
                let here = self.cursor_vcol();
                self.curswant = here
                    .saturating_add(r.vcol.saturating_mul(count))
                    .saturating_sub(1);
            } else {
                self.curswant = r.vcol;
            }
            self.coladvance(self.curswant);
        }
        self.keep_curswant = true;
        if r.vcol == usize::MAX {
            self.curswant = usize::MAX;
            self.coladvance(usize::MAX);
        } else if r.mode == VisualMode::Block {
            let line = self.cursor.line;
            self.cursor.line = self.vstart.line;
            let here = self.cursor_vcol();
            self.curswant = here
                .saturating_add(r.vcol.saturating_mul(count))
                .saturating_sub(1);
            self.cursor.line = line;
            self.coladvance(self.curswant);
        } else if r.mode == VisualMode::Line {
            self.keep_curswant = false;
        }
    }

    /// `gv`.
    fn reselect(&mut self) {
        let Some(lv) = self.last_visual else {
            self.beep();
            return;
        };
        if let Some(m) = self.visual_mode() {
            self.last_visual = Some(crate::editor::LastVisual {
                start: self.vstart,
                end: self.cursor,
                mode: m,
                curswant: self.curswant,
            });
        }
        self.mode = Mode::Visual(lv.mode);
        let clamp = |b: &crate::buffer::Buffer, mut p: Pos| {
            p.line = p.line.min(b.last_line());
            p.col = p.col.min(b.line_len(p.line));
            p
        };
        self.vstart = clamp(&self.buf, lv.start);
        self.cursor = clamp(&self.buf, lv.end);
        self.curswant = lv.curswant;
        self.keep_curswant = true;
        if lv.curswant == usize::MAX {
            self.coladvance(usize::MAX);
        }
    }

    /// Visual `o` and `O` (Vim's `v_swap_corners`).
    fn visual_other(&mut self, horiz: bool) {
        if horiz && self.visual_mode() == Some(VisualMode::Block) {
            let old = self.cursor;
            let (left, right) = self.vcols_of(old, self.vstart);
            self.cursor.line = self.vstart.line;
            self.coladvance(left);
            self.vstart = self.cursor;
            self.cursor.line = old.line;
            self.curswant = right;
            self.coladvance(right);
            if self.cursor.col == old.col {
                self.cursor.line = self.vstart.line;
                self.coladvance(right);
                self.vstart = self.cursor;
                self.cursor.line = old.line;
                self.coladvance(left);
                self.curswant = left;
            }
            self.keep_curswant = true;
        } else {
            std::mem::swap(&mut self.cursor, &mut self.vstart);
        }
    }

    /// Leftmost and rightmost virtual columns of two positions (`getvcols`).
    pub(crate) fn vcols_of(&self, a: Pos, b: Pos) -> (usize, usize) {
        let ts = self.opts.tabstop();
        let (s1, e1) = text::vcol_range(self.buf.line(a.line), a.col, ts);
        let (s2, e2) = text::vcol_range(self.buf.line(b.line), b.col, ts);
        (s1.min(s2), e1.max(e2))
    }

    /// Repeats the last change (`.`).
    fn dot(&mut self, count: Option<usize>) {
        let Some(r) = self.redo.clone() else {
            self.beep();
            return;
        };
        self.keep_curswant = true;
        if let Some(vr) = r.visual {
            self.dot_visual(&r, vr);
            return;
        }
        let reg = r.reg.map(|c| {
            if ('1'..'9').contains(&c) {
                (c as u8 + 1) as char
            } else {
                c
            }
        });
        let count = count.or(r.count);
        let mut keys = Vec::new();
        if let Some(c) = reg {
            keys.push(Key::Char('"'));
            keys.push(Key::Char(c));
        }
        if let Some(n) = count {
            keys.extend(n.to_string().chars().map(Key::Char));
        }
        keys.extend(r.keys.iter().copied());
        for k in keys {
            self.handle_key(k);
        }
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.handle_key(Key::Esc);
        }
    }

    /// Repeats a Visual mode operator on an area of the same size at the
    /// cursor (Vim's `redo_VIsual_busy`).
    fn dot_visual(&mut self, r: &Redo, vr: VisualRedo) {
        let start = self.cursor;
        let vcol_here = self.cursor_vcol();
        self.cursor.line = (self.cursor.line + vr.line_count - 1).min(self.buf.last_line());
        self.mode = Mode::Visual(vr.mode);
        self.vstart = start;
        if vr.vcol == usize::MAX || vr.mode == VisualMode::Char {
            if vr.mode == VisualMode::Char {
                self.curswant = if vr.line_count <= 1 {
                    vcol_here + vr.vcol - 1
                } else {
                    vr.vcol
                };
            } else {
                self.curswant = usize::MAX;
            }
            self.coladvance(self.curswant);
        }
        self.replaying_visual = Some(vr);
        let mut keys = Vec::new();
        if let Some(c) = r.reg {
            keys.push(Key::Char('"'));
            keys.push(Key::Char(c));
        }
        if let Some(n) = vr.count {
            keys.extend(n.to_string().chars().map(Key::Char));
        }
        keys.extend(r.keys.iter().copied());
        for k in keys {
            self.handle_key(k);
        }
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.handle_key(Key::Esc);
        }
        self.replaying_visual = None;
    }
}
