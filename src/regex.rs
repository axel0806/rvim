// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! Vim regular expressions: a backtracking matcher for Vim's pattern syntax
//! (`\v \m \M \V` modes, `\{}`, `\(\)`, `\%(\)`, `\|`, `\&`, `\zs`, `\ze`,
//! look-around, character classes, `\_x` multi-line forms, `\n`).
//! Text is addressed as (line, char column); a match may span lines.
//! Semantics follow Vim's backtracking engine (first alternative wins,
//! greedy by default), not POSIX leftmost-longest.

use crate::buffer::Buffer;
use crate::motion::char_class;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// A (line, char column) position.
pub type P = (usize, usize);

/// Line source for matching.
pub trait Lines {
    fn line_count(&self) -> usize;
    fn chars(&self, l: usize) -> Vec<char>;
}

impl Lines for Buffer {
    fn line_count(&self) -> usize {
        Buffer::line_count(self)
    }
    fn chars(&self, l: usize) -> Vec<char> {
        self.line(l).chars().collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Opts {
    pub ignorecase: bool,
    pub smartcase: bool,
    /// The previous substitute string, for `~`.
    pub last_sub: String,
    /// 'tabstop', for `\%v` (0 is taken as 1).
    pub tabstop: usize,
}

#[derive(Clone, Debug)]
enum Item {
    Ch(char),
    Range(char, char),
    /// A `[:name:]` class.
    Named(&'static str),
    /// A backslash class inside a collection is not supported by Vim; the
    /// only escapes are single chars, so there is no such item.
    Nl,
}

#[derive(Clone, Debug)]
enum Cmp {
    Eq,
    Lt,
    Gt,
}

#[derive(Clone, Debug)]
enum Node {
    Empty,
    Never,
    Char(char),
    Any {
        nl: bool,
    },
    Set {
        items: Vec<Item>,
        neg: bool,
        nl: bool,
    },
    Class {
        k: char,
        nl: bool,
    },
    Bol,
    Eol,
    Bof,
    Eof,
    Bow,
    Eow,
    Nl,
    Cat(Vec<Node>),
    Alt(Vec<Node>),
    Rep {
        n: Box<Node>,
        min: usize,
        max: usize,
        greedy: bool,
    },
    Group(Box<Node>, Option<usize>),
    Backref(usize),
    Zs,
    Ze,
    Look {
        n: Box<Node>,
        behind: bool,
        neg: bool,
    },
    Atomic(Box<Node>),
    Line(usize, Cmp),
    Col(usize, Cmp),
    VCol(usize, Cmp),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Magic {
    /// `\v`
    All,
    /// `\m`
    On,
    /// `\M`
    Off,
    /// `\V`
    None,
}

enum Tok {
    Lit(char),
    /// An operator in its magic meaning (`*`, `(`, `|`, ...).
    Op(char),
    /// A backslash followed by an ordinary char (`\s`, `\1`, `\_`, ...).
    Cls(char),
}

const OPS: &str = "^$.*[~()|&+=?{@<>%";

struct Parser {
    p: Vec<char>,
    i: usize,
    magic: Magic,
    ngroups: usize,
    depth: usize,
    ic: Option<bool>,
    at_start: bool,
    last_sub: Vec<char>,
}

type PRes<T> = Result<T, String>;

impl Parser {
    fn unesc_magic(&self, c: char) -> bool {
        match self.magic {
            Magic::All => OPS.contains(c),
            Magic::On => "^$.*[~".contains(c),
            Magic::Off => "^$".contains(c),
            Magic::None => false,
        }
    }

    /// True if a `$` at index `at` ends the pattern or a branch.
    fn dollar_is_eol(&self, at: usize) -> bool {
        if self.magic == Magic::All {
            return true;
        }
        let mut j = at + 1;
        while self.p.get(j) == Some(&'\\')
            && matches!(
                self.p.get(j + 1),
                Some('c' | 'C' | 'm' | 'M' | 'v' | 'V' | 'Z')
            )
        {
            j += 2;
        }
        match self.p.get(j) {
            None => true,
            Some('\\') => matches!(self.p.get(j + 1), Some('|' | '&' | ')' | 'n')),
            _ => false,
        }
    }

    /// The token at the current index and its length in chars.
    fn tok(&self) -> Option<(Tok, usize)> {
        let c = *self.p.get(self.i)?;
        if c == '\\' {
            let Some(&d) = self.p.get(self.i + 1) else {
                return Some((Tok::Lit('\\'), 1));
            };
            if OPS.contains(d) {
                if d == '^' || d == '$' {
                    return Some(if self.magic == Magic::None {
                        (Tok::Op(d), 2)
                    } else {
                        (Tok::Lit(d), 2)
                    });
                }
                return Some(if self.unesc_magic(d) {
                    (Tok::Lit(d), 2)
                } else {
                    (Tok::Op(d), 2)
                });
            }
            if d.is_ascii_alphanumeric() || d == '_' {
                return Some((Tok::Cls(d), 2));
            }
            return Some((Tok::Lit(d), 2));
        }
        if OPS.contains(c) && self.unesc_magic(c) {
            if c == '^' {
                let ok = self.magic == Magic::All || self.at_start;
                return Some(if ok {
                    (Tok::Op('^'), 1)
                } else {
                    (Tok::Lit('^'), 1)
                });
            }
            if c == '$' {
                return Some(if self.dollar_is_eol(self.i) {
                    (Tok::Op('$'), 1)
                } else {
                    (Tok::Lit('$'), 1)
                });
            }
            return Some((Tok::Op(c), 1));
        }
        Some((Tok::Lit(c), 1))
    }

    fn parse_alt(&mut self) -> PRes<Node> {
        let mut branches = vec![self.parse_and()?];
        while let Some((Tok::Op('|'), n)) = self.tok() {
            self.i += n;
            branches.push(self.parse_and()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            Node::Alt(branches)
        })
    }

    fn parse_and(&mut self) -> PRes<Node> {
        let mut parts = vec![self.parse_concat()?];
        while let Some((Tok::Op('&'), n)) = self.tok() {
            self.i += n;
            parts.push(self.parse_concat()?);
        }
        if parts.len() == 1 {
            return Ok(parts.pop().unwrap());
        }
        let last = parts.pop().unwrap();
        let mut seq: Vec<Node> = parts
            .into_iter()
            .map(|n| Node::Look {
                n: Box::new(n),
                behind: false,
                neg: false,
            })
            .collect();
        seq.push(last);
        Ok(Node::Cat(seq))
    }

    fn parse_concat(&mut self) -> PRes<Node> {
        self.at_start = true;
        let mut seq = Vec::new();
        loop {
            match self.tok() {
                None => break,
                Some((Tok::Op('|' | '&'), _)) => break,
                Some((Tok::Op(')'), _)) => {
                    if self.depth == 0 {
                        return Err("E55: Unmatched \\)".into());
                    }
                    break;
                }
                _ => {}
            }
            let was_start = self.at_start;
            let Some(piece) = self.parse_piece(was_start)? else {
                continue;
            };
            seq.push(piece);
        }
        Ok(if seq.len() == 1 {
            seq.pop().unwrap()
        } else {
            Node::Cat(seq)
        })
    }

    fn parse_piece(&mut self, start: bool) -> PRes<Option<Node>> {
        let after_bol = matches!(self.tok(), Some((Tok::Op('^'), _)));
        let Some(mut atom) = self.parse_atom()? else {
            return Ok(None);
        };
        // `^` keeps "at start" so that `^*` is a literal star.
        self.at_start =
            after_bol || matches!(atom, Node::Nl) || (start && matches!(atom, Node::Empty));
        let is_bol = matches!(atom, Node::Bol);
        while let Some((t, n)) = self.tok() {
            match t {
                Tok::Op('*') if !is_bol => {
                    self.i += n;
                    atom = rep(atom, 0, usize::MAX, true);
                }
                Tok::Op('+') => {
                    self.i += n;
                    atom = rep(atom, 1, usize::MAX, true);
                }
                Tok::Op('=' | '?') => {
                    self.i += n;
                    atom = rep(atom, 0, 1, true);
                }
                Tok::Op('{') => {
                    self.i += n;
                    atom = self.parse_braces(atom)?;
                }
                Tok::Op('@') => {
                    self.i += n;
                    atom = self.parse_look(atom)?;
                }
                _ => break,
            }
        }
        Ok(Some(atom))
    }

    fn parse_braces(&mut self, atom: Node) -> PRes<Node> {
        let mut s = String::new();
        loop {
            match self.p.get(self.i) {
                None => return Err("E554: Syntax error in \\{...}".into()),
                Some('\\') if self.p.get(self.i + 1) == Some(&'}') => {
                    self.i += 2;
                    break;
                }
                Some('}') => {
                    self.i += 1;
                    break;
                }
                Some(&c) => {
                    s.push(c);
                    self.i += 1;
                }
            }
        }
        let (greedy, body) = match s.strip_prefix('-') {
            Some(b) => (false, b),
            None => (true, s.as_str()),
        };
        let num = |t: &str| -> PRes<Option<usize>> {
            if t.is_empty() {
                Ok(None)
            } else {
                t.parse()
                    .map(Some)
                    .map_err(|_| "E554: Syntax error in \\{...}".to_string())
            }
        };
        let (min, max) = match body.split_once(',') {
            None => match num(body)? {
                None => (0, usize::MAX),
                Some(n) => (n, n),
            },
            Some((a, b)) => (num(a)?.unwrap_or(0), num(b)?.unwrap_or(usize::MAX)),
        };
        let (min, max) = if min > max { (max, min) } else { (min, max) };
        Ok(rep(atom, min, max, greedy))
    }

    fn parse_look(&mut self, atom: Node) -> PRes<Node> {
        while self.p.get(self.i).is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        let c = self.p.get(self.i).copied();
        self.i += 1;
        let (behind, neg, atomic) = match c {
            Some('=') => (false, false, false),
            Some('!') => (false, true, false),
            Some('>') => (false, false, true),
            Some('<') => {
                let d = self.p.get(self.i).copied();
                self.i += 1;
                match d {
                    Some('=') => (true, false, false),
                    Some('!') => (true, true, false),
                    _ => return Err("E59: Invalid character after \\@".into()),
                }
            }
            _ => return Err("E59: Invalid character after \\@".into()),
        };
        Ok(if atomic {
            Node::Atomic(Box::new(atom))
        } else {
            Node::Look {
                n: Box::new(atom),
                behind,
                neg,
            }
        })
    }

    fn parse_group(&mut self, capture: bool) -> PRes<Node> {
        let idx = if capture {
            self.ngroups += 1;
            if self.ngroups > 9 {
                return Err("E51: Too many \\(".into());
            }
            Some(self.ngroups)
        } else {
            None
        };
        self.depth += 1;
        let inner = self.parse_alt()?;
        self.depth -= 1;
        match self.tok() {
            Some((Tok::Op(')'), n)) => self.i += n,
            _ => {
                return Err(if capture {
                    "E54: Unmatched \\(".into()
                } else {
                    "E53: Unmatched \\%(".into()
                });
            }
        }
        Ok(Node::Group(Box::new(inner), idx))
    }

    fn parse_atom(&mut self) -> PRes<Option<Node>> {
        let Some((t, n)) = self.tok() else {
            return Ok(None);
        };
        self.i += n;
        self.at_start = false;
        let node = match t {
            Tok::Lit(c) => Node::Char(c),
            Tok::Op(c) => match c {
                '^' => {
                    self.at_start = true;
                    Node::Bol
                }
                '$' => Node::Eol,
                '.' => Node::Any { nl: false },
                '~' => Node::Cat(self.last_sub.iter().map(|&c| Node::Char(c)).collect()),
                '<' => Node::Bow,
                '>' => Node::Eow,
                '[' => match self.parse_collection(false) {
                    Some(n) => n,
                    None => Node::Char('['),
                },
                '(' => {
                    let g = self.parse_group(true)?;
                    self.at_start = false;
                    g
                }
                '*' => Node::Char('*'),
                '+' | '=' | '?' | '{' | '@' => {
                    return Err(format!("E64: {} follows nothing", op_name(c)));
                }
                '%' => self.parse_percent()?,
                ')' | '|' | '&' => unreachable!(),
                _ => Node::Char(c),
            },
            Tok::Cls(c) => match c {
                's' | 'S' | 'd' | 'D' | 'w' | 'W' | 'a' | 'A' | 'l' | 'L' | 'u' | 'U' | 'x'
                | 'X' | 'o' | 'O' | 'h' | 'H' | 'i' | 'I' | 'k' | 'K' | 'f' | 'F' | 'p' | 'P' => {
                    Node::Class { k: c, nl: false }
                }
                'e' => Node::Char('\x1b'),
                't' => Node::Char('\t'),
                'r' => Node::Char('\r'),
                'b' => Node::Char('\x08'),
                'n' => {
                    self.at_start = true;
                    Node::Nl
                }
                '1'..='9' => Node::Backref(c as usize - '0' as usize),
                'z' => {
                    let d = self.p.get(self.i).copied();
                    self.i += 1;
                    match d {
                        Some('s') => Node::Zs,
                        Some('e') => Node::Ze,
                        _ => return Err("E68: Invalid character after \\z".into()),
                    }
                }
                'c' => {
                    self.ic = Some(true);
                    Node::Empty
                }
                'C' => {
                    self.ic = Some(false);
                    Node::Empty
                }
                'Z' => Node::Empty,
                'm' => {
                    self.magic = Magic::On;
                    Node::Empty
                }
                'M' => {
                    self.magic = Magic::Off;
                    Node::Empty
                }
                'v' => {
                    self.magic = Magic::All;
                    Node::Empty
                }
                'V' => {
                    self.magic = Magic::None;
                    Node::Empty
                }
                '_' => {
                    let d = self.p.get(self.i).copied();
                    self.i += 1;
                    match d {
                        Some('^') => Node::Bol,
                        Some('$') => Node::Eol,
                        Some('.') => Node::Any { nl: true },
                        Some('[') => match self.parse_collection(true) {
                            Some(n) => n,
                            None => return Err("E769: Missing ] after \\_[".into()),
                        },
                        Some(
                            k @ ('s' | 'S' | 'd' | 'D' | 'w' | 'W' | 'a' | 'A' | 'l' | 'L' | 'u'
                            | 'U' | 'x' | 'X' | 'o' | 'O' | 'h' | 'H' | 'i' | 'I' | 'k' | 'K'
                            | 'f' | 'F' | 'p' | 'P'),
                        ) => Node::Class { k, nl: true },
                        _ => return Err("E63: Invalid use of \\_".into()),
                    }
                }
                _ => Node::Char(c),
            },
        };
        Ok(Some(node))
    }

    fn parse_percent(&mut self) -> PRes<Node> {
        let Some(&c) = self.p.get(self.i) else {
            return Err("E71: Invalid character after \\%".into());
        };
        self.i += 1;
        let hex = |s: &mut Self, max: usize, radix: u32| -> PRes<Node> {
            let mut v: u32 = 0;
            let mut n = 0;
            while n < max
                && let Some(d) = s.p.get(s.i).and_then(|c| c.to_digit(radix))
            {
                v = v * radix + d;
                s.i += 1;
                n += 1;
            }
            if n == 0 {
                return Err("E678: Invalid character after \\%[dxouU]".into());
            }
            Ok(Node::Char(char::from_u32(v).unwrap_or('\u{fffd}')))
        };
        match c {
            '(' => self.parse_group(false),
            '^' => Ok(Node::Bof),
            '$' => Ok(Node::Eof),
            'V' | '#' | 'C' => Ok(if c == 'C' { Node::Empty } else { Node::Never }),
            'd' => hex(self, 10, 10),
            'x' => hex(self, 2, 16),
            'u' => hex(self, 4, 16),
            'U' => hex(self, 8, 16),
            'o' => hex(self, 4, 8),
            '[' => {
                let mut chars = Vec::new();
                loop {
                    match self.p.get(self.i) {
                        None => return Err("E69: Missing ] after \\%[".into()),
                        Some(']') => {
                            self.i += 1;
                            break;
                        }
                        Some(&c) => {
                            chars.push(c);
                            self.i += 1;
                        }
                    }
                }
                if chars.is_empty() {
                    return Err("E70: Empty \\%[]".into());
                }
                let mut node: Option<Node> = None;
                for &c in chars.iter().rev() {
                    let seq = match node.take() {
                        None => Node::Char(c),
                        Some(rest) => Node::Cat(vec![Node::Char(c), rest]),
                    };
                    node = Some(rep(seq, 0, 1, true));
                }
                Ok(node.unwrap())
            }
            '\'' => {
                self.i += 1;
                Ok(Node::Never)
            }
            '<' | '>' | '0'..='9' | '.' => {
                let cmp = match c {
                    '<' => Cmp::Lt,
                    '>' => Cmp::Gt,
                    _ => Cmp::Eq,
                };
                if matches!(c, '<' | '>') && self.p.get(self.i) == Some(&'\'') {
                    self.i += 2;
                    return Ok(Node::Never);
                }
                if !matches!(c, '<' | '>') {
                    self.i -= 1;
                }
                let mut v = 0usize;
                let mut any = false;
                while let Some(d) = self.p.get(self.i).and_then(|c| c.to_digit(10)) {
                    v = v.saturating_mul(10).saturating_add(d as usize);
                    self.i += 1;
                    any = true;
                }
                if !any && self.p.get(self.i) == Some(&'.') {
                    self.i += 1;
                    return Ok(Node::Never);
                }
                let kind = self.p.get(self.i).copied();
                self.i += 1;
                match kind {
                    Some('l') => Ok(Node::Line(v, cmp)),
                    Some('c') => Ok(Node::Col(v, cmp)),
                    Some('v') => Ok(Node::VCol(v, cmp)),
                    _ => Err("E71: Invalid character after \\%".into()),
                }
            }
            _ => Err("E71: Invalid character after \\%".into()),
        }
    }

    /// Parses `[...]` after its `[`. `None` if there is no closing `]`
    /// (the `[` is then literal).
    fn parse_collection(&mut self, nl_all: bool) -> Option<Node> {
        let save = self.i;
        let mut j = self.i;
        let mut neg = false;
        if self.p.get(j) == Some(&'^') {
            neg = true;
            j += 1;
        }
        let mut items = Vec::new();
        let mut nl = nl_all;
        let mut first = true;
        loop {
            let c = *self.p.get(j)?;
            if c == ']' && !first {
                j += 1;
                break;
            }
            first = false;
            // [:name:]
            if c == '[' && self.p.get(j + 1) == Some(&':') {
                let rest: String = self.p[j + 2..].iter().collect();
                if let Some(end) = rest.find(":]") {
                    let name = &rest[..end];
                    const NAMES: [&str; 14] = [
                        "alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print",
                        "punct", "space", "upper", "xdigit", "return", "tab",
                    ];
                    const MORE: [&str; 4] = ["escape", "backspace", "ident", "keyword"];
                    if let Some(nm) = NAMES.iter().chain(MORE.iter()).find(|&&n| n == name) {
                        items.push(Item::Named(nm));
                        j += 2 + name.chars().count() + 2;
                        continue;
                    }
                }
            }
            let (lo, len) = if c == '\\' {
                self.coll_escape(j)
            } else {
                (Some(c), 1)
            };
            j += len;
            let Some(lo) = lo else {
                // \n
                nl = true;
                items.push(Item::Nl);
                continue;
            };
            // range?
            if self.p.get(j) == Some(&'-') && self.p.get(j + 1).is_some_and(|&d| d != ']') {
                let (hi, len2) = if self.p[j + 1] == '\\' {
                    self.coll_escape(j + 1)
                } else {
                    (Some(self.p[j + 1]), 1)
                };
                if let Some(hi) = hi {
                    j += 1 + len2;
                    items.push(Item::Range(lo, hi));
                    continue;
                }
            }
            items.push(Item::Ch(lo));
        }
        self.i = j;
        let _ = save;
        if neg && !nl_all {
            nl = false;
        }
        Some(Node::Set { items, neg, nl })
    }

    /// An escape inside `[]` at index `j` (a backslash). Returns the char
    /// (`None` for `\n`) and the length used.
    fn coll_escape(&self, j: usize) -> (Option<char>, usize) {
        let Some(&d) = self.p.get(j + 1) else {
            return (Some('\\'), 1);
        };
        let num = |radix: u32, max: usize| -> Option<(char, usize)> {
            let mut v = 0u32;
            let mut n = 0;
            while n < max
                && let Some(x) = self.p.get(j + 2 + n).and_then(|c| c.to_digit(radix))
            {
                v = v * radix + x;
                n += 1;
            }
            (n > 0).then(|| (char::from_u32(v).unwrap_or('\u{fffd}'), 2 + n))
        };
        match d {
            'e' => (Some('\x1b'), 2),
            't' => (Some('\t'), 2),
            'r' => (Some('\r'), 2),
            'b' => (Some('\x08'), 2),
            'n' => (None, 2),
            '\\' | ']' | '^' | '-' => (Some(d), 2),
            'd' => num(10, 10).map_or((Some('\\'), 1), |(c, n)| (Some(c), n)),
            'o' => num(8, 4).map_or((Some('\\'), 1), |(c, n)| (Some(c), n)),
            'x' => num(16, 2).map_or((Some('\\'), 1), |(c, n)| (Some(c), n)),
            'u' => num(16, 4).map_or((Some('\\'), 1), |(c, n)| (Some(c), n)),
            'U' => num(16, 8).map_or((Some('\\'), 1), |(c, n)| (Some(c), n)),
            _ => (Some('\\'), 1),
        }
    }
}

fn op_name(c: char) -> &'static str {
    match c {
        '+' => "\\+",
        '=' => "\\=",
        '?' => "\\?",
        '{' => "\\{",
        '@' => "\\@",
        _ => "*",
    }
}

fn rep(n: Node, min: usize, max: usize, greedy: bool) -> Node {
    Node::Rep {
        n: Box::new(n),
        min,
        max,
        greedy,
    }
}

// ---- character classes ----------------------------------------------------

fn is_word(c: char) -> bool {
    let u = c as u32;
    if u < 0x100 {
        c.is_ascii_alphanumeric() || c == '_' || u >= 192
    } else {
        char_class(c) >= 2
    }
}

fn is_ident(c: char) -> bool {
    let u = c as u32;
    u < 0x100 && (c.is_ascii_alphanumeric() || c == '_' || u >= 192)
}

fn is_fname(c: char) -> bool {
    (c as u32) >= 0x100
        || c.is_ascii_alphanumeric()
        || "/.-_+,#$%~=".contains(c)
        || (c as u32) >= 192
}

fn is_print(c: char) -> bool {
    let u = c as u32;
    if u < 0x100 {
        (0x20..0x7f).contains(&u) || u >= 0xa0
    } else {
        !c.is_control()
    }
}

fn class_match(k: char, c: char) -> bool {
    let digit = c.is_ascii_digit();
    match k {
        's' => c == ' ' || c == '\t',
        'S' => !(c == ' ' || c == '\t'),
        'd' => digit,
        'D' => !digit,
        'w' => c.is_ascii_alphanumeric() || c == '_',
        'W' => !(c.is_ascii_alphanumeric() || c == '_'),
        'a' => c.is_ascii_alphabetic(),
        'A' => !c.is_ascii_alphabetic(),
        'l' => c.is_ascii_lowercase(),
        'L' => !c.is_ascii_lowercase(),
        'u' => c.is_ascii_uppercase(),
        'U' => !c.is_ascii_uppercase(),
        'x' => c.is_ascii_hexdigit(),
        'X' => !c.is_ascii_hexdigit(),
        'o' => ('0'..='7').contains(&c),
        'O' => !('0'..='7').contains(&c),
        'h' => c.is_ascii_alphabetic() || c == '_',
        'H' => !(c.is_ascii_alphabetic() || c == '_'),
        'i' => is_ident(c),
        'I' => is_ident(c) && !digit,
        'k' => is_word(c),
        'K' => is_word(c) && !digit,
        'f' => is_fname(c),
        'F' => is_fname(c) && !digit,
        'p' => is_print(c),
        'P' => is_print(c) && !digit,
        _ => false,
    }
}

fn named_match(name: &str, c: char) -> bool {
    match name {
        "alnum" => c.is_ascii_alphanumeric(),
        "alpha" => c.is_ascii_alphabetic(),
        "blank" => c == ' ' || c == '\t',
        "cntrl" => c.is_ascii_control(),
        "digit" => c.is_ascii_digit(),
        "graph" => c.is_ascii_graphic(),
        "lower" => c.is_lowercase(),
        "print" => is_print(c),
        "punct" => c.is_ascii_punctuation(),
        "space" => matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'),
        "upper" => c.is_uppercase(),
        "xdigit" => c.is_ascii_hexdigit(),
        "return" => c == '\r',
        "tab" => c == '\t',
        "escape" => c == '\x1b',
        "backspace" => c == '\x08',
        "ident" => is_ident(c),
        "keyword" => is_word(c),
        _ => false,
    }
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

// ---- matcher ----------------------------------------------------------------

#[derive(Clone, Default)]
struct St {
    caps: [Option<(P, P)>; 10],
    zs: Option<P>,
    ze: Option<P>,
}

/// A successful match: start and end (after `\zs`/`\ze`), and groups.
#[derive(Clone, Debug)]
pub struct Match {
    pub start: P,
    pub end: P,
    pub groups: [Option<(P, P)>; 10],
}

pub struct Regex {
    prog: Node,
    ic: bool,
    ts: usize,
}

struct Matcher<'a> {
    src: &'a dyn Lines,
    ic: bool,
    ts: usize,
    cache: RefCell<HashMap<usize, Rc<Vec<char>>>>,
    steps: Cell<usize>,
}

const STEP_LIMIT: usize = 20_000_000;

type K<'k> = &'k mut dyn FnMut(P, &mut St) -> bool;

impl Regex {
    pub fn new(pat: &str, opts: &Opts) -> Result<Regex, String> {
        let mut p = Parser {
            p: pat.chars().collect(),
            i: 0,
            magic: Magic::On,
            ngroups: 0,
            depth: 0,
            ic: None,
            at_start: true,
            last_sub: opts.last_sub.chars().collect(),
        };
        let prog = p.parse_alt()?;
        if p.i < p.p.len() {
            return Err("E55: Unmatched \\)".into());
        }
        let ic = match p.ic {
            Some(v) => v,
            None => {
                opts.ignorecase
                    && !(opts.smartcase
                        && pat.chars().enumerate().any(|(i, c)| {
                            c.is_uppercase() && (i == 0 || pat.chars().nth(i - 1) != Some('\\'))
                        }))
            }
        };
        Ok(Regex {
            prog,
            ic,
            ts: opts.tabstop.max(1),
        })
    }

    /// The first match in `lnum` whose attempt starts at char column `col`
    /// or later (up to and including the end-of-line position).
    pub fn match_line(&self, src: &dyn Lines, lnum: usize, col: usize) -> Option<Match> {
        let m = Matcher {
            src,
            ic: self.ic,
            ts: self.ts,
            cache: RefCell::new(HashMap::new()),
            steps: Cell::new(0),
        };
        let len = m.line(lnum).len();
        for c in col..=len {
            if let Some(r) = m.attempt(&self.prog, (lnum, c)) {
                return Some(r);
            }
            if m.steps.get() > STEP_LIMIT {
                return None;
            }
        }
        None
    }

    /// True if the pattern matches somewhere in line `lnum`.
    pub fn is_match(&self, src: &dyn Lines, lnum: usize) -> bool {
        self.match_line(src, lnum, 0).is_some()
    }
}

impl<'a> Matcher<'a> {
    fn line(&self, l: usize) -> Rc<Vec<char>> {
        if let Some(v) = self.cache.borrow().get(&l) {
            return v.clone();
        }
        let v = Rc::new(if l < self.src.line_count() {
            self.src.chars(l)
        } else {
            Vec::new()
        });
        self.cache.borrow_mut().insert(l, v.clone());
        v
    }

    fn attempt(&self, prog: &Node, p: P) -> Option<Match> {
        let mut st = St::default();
        let mut out: Option<(P, St)> = None;
        self.m(prog, p, &mut st, &mut |e, s| {
            out = Some((e, s.clone()));
            true
        });
        let (end, s) = out?;
        let start = s.zs.unwrap_or(p);
        let mut e = s.ze.unwrap_or(end);
        if e < start {
            e = start;
        }
        let mut groups = s.caps;
        groups[0] = Some((start, e));
        Some(Match {
            start,
            end: e,
            groups,
        })
    }

    fn eq(&self, a: char, b: char) -> bool {
        a == b || (self.ic && fold(a) == fold(b))
    }

    fn at(&self, p: P) -> Option<char> {
        self.line(p.0).get(p.1).copied()
    }

    fn set_has(&self, items: &[Item], neg: bool, c: char) -> bool {
        let mut hit = false;
        for it in items {
            hit = match it {
                Item::Ch(x) => self.eq(*x, c),
                Item::Range(a, b) => {
                    (*a <= c && c <= *b)
                        || (self.ic && {
                            let lo = fold(c);
                            let up = c.to_uppercase().next().unwrap_or(c);
                            (*a <= lo && lo <= *b) || (*a <= up && up <= *b)
                        })
                }
                Item::Named(n) => {
                    named_match(n, c)
                        || (self.ic && matches!(*n, "lower" | "upper") && c.is_alphabetic())
                }
                Item::Nl => false,
            };
            if hit {
                break;
            }
        }
        hit != neg
    }

    /// One step over a single-width node.
    fn one(&self, n: &Node, p: P) -> Option<P> {
        let line = self.line(p.0);
        let c = line.get(p.1).copied();
        let nl_step = |nl: bool| -> Option<P> {
            (nl && c.is_none() && p.0 < self.src.line_count()).then_some((p.0 + 1, 0))
        };
        match n {
            Node::Char(x) => match c {
                Some(c) if self.eq(*x, c) => Some((p.0, p.1 + 1)),
                _ => None,
            },
            Node::Any { nl } => match c {
                Some(_) => Some((p.0, p.1 + 1)),
                None => nl_step(*nl),
            },
            Node::Set { items, neg, nl } => match c {
                Some(c) if self.set_has(items, *neg, c) => Some((p.0, p.1 + 1)),
                Some(_) => None,
                None => nl_step(*nl),
            },
            Node::Class { k, nl } => match c {
                Some(c) if class_match(*k, c) => Some((p.0, p.1 + 1)),
                Some(_) => None,
                None => nl_step(*nl),
            },
            Node::Nl => nl_step(true),
            _ => None,
        }
    }

    fn is_simple(n: &Node) -> bool {
        matches!(
            n,
            Node::Char(_) | Node::Any { .. } | Node::Set { .. } | Node::Class { .. } | Node::Nl
        )
    }

    fn text(&self, a: P, b: P) -> Vec<char> {
        let mut out = Vec::new();
        let mut p = a;
        while p < b {
            let line = self.line(p.0);
            if p.1 < line.len() {
                out.push(line[p.1]);
                p.1 += 1;
            } else {
                out.push('\n');
                p = (p.0 + 1, 0);
            }
        }
        out
    }

    fn seq(&self, nodes: &[Node], p: P, st: &mut St, k: K) -> bool {
        match nodes.split_first() {
            None => k(p, st),
            Some((first, rest)) => self.m(first, p, st, &mut |q, s| self.seq(rest, q, s, k)),
        }
    }

    fn m(&self, n: &Node, p: P, st: &mut St, k: K) -> bool {
        let steps = self.steps.get() + 1;
        self.steps.set(steps);
        if steps > STEP_LIMIT {
            return false;
        }
        match n {
            Node::Empty => k(p, st),
            Node::Never => false,
            Node::Char(_) | Node::Any { .. } | Node::Set { .. } | Node::Class { .. } | Node::Nl => {
                match self.one(n, p) {
                    Some(q) => k(q, st),
                    None => false,
                }
            }
            Node::Bol => p.1 == 0 && k(p, st),
            Node::Eol => p.1 >= self.line(p.0).len() && k(p, st),
            Node::Bof => p == (0, 0) && k(p, st),
            Node::Eof => {
                p.0 + 1 == self.src.line_count() && p.1 >= self.line(p.0).len() && k(p, st)
            }
            Node::Bow => {
                let line = self.line(p.0);
                let cur = line.get(p.1).is_some_and(|&c| is_word(c));
                let prev = p.1 > 0 && is_word(line[p.1 - 1]);
                cur && !prev && k(p, st)
            }
            Node::Eow => {
                let line = self.line(p.0);
                let cur = line.get(p.1).is_some_and(|&c| is_word(c));
                let prev = p.1 > 0 && is_word(line[p.1 - 1]);
                prev && !cur && k(p, st)
            }
            Node::Line(v, cmp) => cmp_ok(p.0 + 1, *v, cmp) && k(p, st),
            Node::Col(v, cmp) => {
                let line = self.line(p.0);
                let bytes: usize = line.iter().take(p.1).map(|c| c.len_utf8()).sum();
                cmp_ok(bytes + 1, *v, cmp) && k(p, st)
            }
            Node::VCol(v, cmp) => {
                let line = self.line(p.0);
                let mut vc = 0;
                for &c in line.iter().take(p.1) {
                    vc += crate::text::char_width(c, vc, self.ts);
                }
                cmp_ok(vc + 1, *v, cmp) && k(p, st)
            }
            Node::Cat(nodes) => self.seq(nodes, p, st, k),
            Node::Alt(branches) => {
                for b in branches {
                    if self.m(b, p, st, k) {
                        return true;
                    }
                }
                false
            }
            Node::Group(inner, idx) => match idx {
                None => self.m(inner, p, st, k),
                Some(i) => {
                    let i = *i;
                    self.m(inner, p, st, &mut |q, s| {
                        let old = s.caps[i];
                        s.caps[i] = Some((p, q));
                        if k(q, s) {
                            true
                        } else {
                            s.caps[i] = old;
                            false
                        }
                    })
                }
            },
            Node::Backref(i) => match st.caps[*i] {
                None => k(p, st),
                Some((a, b)) => {
                    let s = self.text(a, b);
                    let mut q = p;
                    for ch in s {
                        if ch == '\n' {
                            match self.one(&Node::Nl, q) {
                                Some(r) => q = r,
                                None => return false,
                            }
                        } else {
                            match self.at(q) {
                                Some(c) if self.eq(ch, c) => q.1 += 1,
                                _ => return false,
                            }
                        }
                    }
                    k(q, st)
                }
            },
            Node::Zs => {
                let old = st.zs;
                st.zs = Some(p);
                if k(p, st) {
                    true
                } else {
                    st.zs = old;
                    false
                }
            }
            Node::Ze => {
                let old = st.ze;
                st.ze = Some(p);
                if k(p, st) {
                    true
                } else {
                    st.ze = old;
                    false
                }
            }
            Node::Look { n, behind, neg } => {
                let found = if *behind {
                    self.look_behind(n, p, st)
                } else {
                    let mut st2 = st.clone();
                    self.m(n, p, &mut st2, &mut |_, _| true)
                };
                found != *neg && k(p, st)
            }
            Node::Atomic(inner) => {
                let mut st2 = st.clone();
                let mut end = None;
                if !self.m(inner, p, &mut st2, &mut |e, _| {
                    end = Some(e);
                    true
                }) {
                    return false;
                }
                k(end.unwrap(), st)
            }
            Node::Rep {
                n,
                min,
                max,
                greedy,
            } => {
                if Self::is_simple(n) {
                    let mut ps = vec![p];
                    let mut cur = p;
                    while ps.len() - 1 < *max {
                        match self.one(n, cur) {
                            Some(q) => {
                                ps.push(q);
                                cur = q;
                            }
                            None => break,
                        }
                    }
                    let cnt = ps.len() - 1;
                    if cnt < *min {
                        return false;
                    }
                    if *greedy {
                        for i in (*min..=cnt).rev() {
                            if k(ps[i], st) {
                                return true;
                            }
                        }
                    } else {
                        for q in &ps[*min..=cnt] {
                            if k(*q, st) {
                                return true;
                            }
                        }
                    }
                    false
                } else {
                    self.rep(n, *min, *max, *greedy, 0, p, st, k)
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rep(
        &self,
        n: &Node,
        min: usize,
        max: usize,
        greedy: bool,
        cnt: usize,
        p: P,
        st: &mut St,
        k: K,
    ) -> bool {
        if greedy {
            if cnt < max
                && self.m(n, p, st, &mut |q, s| {
                    if q == p && cnt >= min {
                        return false;
                    }
                    self.rep(n, min, max, greedy, cnt + 1, q, s, k)
                })
            {
                return true;
            }
            cnt >= min && k(p, st)
        } else {
            if cnt >= min && k(p, st) {
                return true;
            }
            cnt < max
                && self.m(n, p, st, &mut |q, s| {
                    if q == p && cnt >= min {
                        return false;
                    }
                    self.rep(n, min, max, greedy, cnt + 1, q, s, k)
                })
        }
    }

    fn look_behind(&self, n: &Node, p: P, st: &St) -> bool {
        let mut starts: Vec<P> = (0..=p.1).rev().map(|c| (p.0, c)).collect();
        if p.0 > 0 {
            let plen = self.line(p.0 - 1).len();
            starts.extend((0..=plen).rev().map(|c| (p.0 - 1, c)));
        }
        for q in starts {
            let mut st2 = st.clone();
            if self.m(n, q, &mut st2, &mut |e, _| e == p) {
                return true;
            }
        }
        false
    }
}

fn cmp_ok(have: usize, want: usize, cmp: &Cmp) -> bool {
    match cmp {
        Cmp::Eq => have == want,
        Cmp::Lt => have < want,
        Cmp::Gt => have > want,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(pat: &str, text: &str) -> Option<(P, P)> {
        let buf = Buffer::from_text(text);
        let re = Regex::new(pat, &Opts::default()).unwrap();
        (0..buf.line_count()).find_map(|l| re.match_line(&buf, l, 0).map(|m| (m.start, m.end)))
    }

    #[test]
    fn basics() {
        assert_eq!(find("b*c", "aabbbc"), Some(((0, 2), (0, 6))));
        assert_eq!(find("\\vb+(c|d)", "abbd"), Some(((0, 1), (0, 4))));
        assert_eq!(find("a\\{2}", "aaa"), Some(((0, 0), (0, 2))));
        assert_eq!(find("foo\\zsbar", "foobar"), Some(((0, 3), (0, 6))));
        assert_eq!(find("\\<bar\\>", "foobar bar"), Some(((0, 7), (0, 10))));
        assert_eq!(find("a\\nb", "a\nb"), Some(((0, 0), (1, 1))));
        assert_eq!(find("\\(a\\)\\1", "xaa"), Some(((0, 1), (0, 3))));
        assert_eq!(find("[0-9]\\+", "ab123"), Some(((0, 2), (0, 5))));
        assert_eq!(
            find("foo\\(bar\\)\\@!", "foobar foobaz"),
            Some(((0, 7), (0, 10)))
        );
        assert_eq!(
            find("\\(foo\\)\\@<=bar", "xbar foobar"),
            Some(((0, 8), (0, 11)))
        );
        assert_eq!(find("^$", "a\n\nb"), Some(((1, 0), (1, 0))));
        assert_eq!(
            find("\\Vfoo.bar", "fooXbar foo.bar"),
            Some(((0, 8), (0, 15)))
        );
    }

    #[test]
    fn errors() {
        assert!(Regex::new("\\(a", &Opts::default()).is_err());
        assert!(Regex::new("a\\)", &Opts::default()).is_err());
    }
}
