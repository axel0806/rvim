// SPDX-License-Identifier: Apache-2.0
//! Vim's expression language, as used by `:s/pat/\=expr/`: numbers, strings,
//! lists, the operators, `?:`, and the common built-in functions. The module
//! is pure: the caller says what the expression may read (an [`Env`]) and gets
//! a [`Value`] or an [`ExprError`] with Vim's message. Nothing here touches a
//! buffer or a terminal, so a later Vimscript subset can reuse it.
//!
//! Two kinds of error, as in Vim. An [`ExprError`] fails the whole expression.
//! An error inside a built-in function (`submatch(10)`, `range(2, 0)`) is
//! reported in the list returned with the value, and the function returns a
//! default value (0, "" or []); the evaluation goes on.
//!
//! Limits, so that typed input cannot exhaust the stack or the memory: the
//! nesting of expressions ([`MAX_DEPTH`], E1169 as in Vim), the number of
//! items of a list or range ([`MAX_ITEMS`]), the size of a built string
//! ([`MAX_BYTES`]) and the width of a `printf()` field ([`MAX_WIDTH`]). Going
//! over one is an error (E342 or E1510).
//!
//! Not supported: floats, dictionaries, funcrefs, lambdas, `v:`/`g:` and other
//! scoped variables, user functions and Vim9 syntax. Strings are byte-indexed
//! as in Vim; a cut inside a multi-byte character gives U+FFFD.

use crate::regex::{Lines, Match, Opts, Regex};
use std::cmp::Ordering;
use std::rc::Rc;

/// Nesting of expressions (parentheses, lists, arguments). Vim accepts 200
/// levels and fails at 1000. Here the count is 100, and the stack used by
/// the parser is limited to [`STACK_LIMIT`] as well: one level of nesting
/// takes many kilobytes in a debug build.
pub const MAX_DEPTH: usize = 100;
/// Stack bytes the parser may use, measured from the start of `eval()`. It
/// leaves most of a 1 MB stack to the editor.
pub const STACK_LIMIT: usize = 512 << 10;
/// Items of a list, of a range or of a split. Above it the result is E342.
pub const MAX_ITEMS: usize = 1_000_000;
/// Bytes of a string built by repeat(), join(), `.` or substitute().
pub const MAX_BYTES: usize = 64 << 20;
/// Width of a printf() field.
pub const MAX_WIDTH: usize = 1_000_000;
/// Bytes of all the values one expression may keep: each result of a
/// function counts, so lists of big results cannot exhaust the memory.
pub const MAX_TOTAL: usize = 256 << 20;

/// The value of the digits `digits` in `radix`, saturated at 2^64 (which is
/// above every limit used here, so the callers clamp it).
fn magnitude(digits: &str, radix: u32) -> u128 {
    let mut v: u128 = 0;
    for c in digits.chars() {
        if let Some(d) = c.to_digit(radix) {
            v = (v * radix as u128 + d as u128).min(1 << 64);
        }
    }
    v
}

/// The bytes a value takes, for the limit on the total.
fn value_size(v: &Value) -> usize {
    match v {
        Value::Num(_) => 8,
        Value::Str(s) => s.len(),
        Value::List(items) => 8 + items.iter().map(value_size).sum::<usize>(),
    }
}

/// A value as a string, taking a string without copying it.
fn into_str(v: Value) -> R<String> {
    match v {
        Value::Num(n) => Ok(n.to_string()),
        Value::Str(s) => Ok(s),
        Value::List(_) => err("E730: Using a List as a String"),
    }
}

/// A value of the expression language.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(i64),
    Str(String),
    List(Vec<Value>),
}

/// What an expression may read: the groups of the current match, the cursor
/// and the size of the buffer.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// `submatch(0)` to `submatch(9)`: "" for a group that did not match. A
    /// line break in a match is "\n".
    pub groups: [String; 10],
    /// `line('.')`: the cursor line, 1-based.
    pub line: i64,
    /// `col('.')`: the cursor column, 1-based and in bytes.
    pub col: i64,
    /// `line('$')`: the number of lines.
    pub last_line: i64,
    /// 'ignorecase', for the comparisons without `#` or `?`.
    pub ignorecase: bool,
}

/// An error with Vim's message, such as `E121: Undefined variable: x`. It
/// fails the whole expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError {
    pub msg: String,
}

type R<T> = Result<T, ExprError>;

/// A failure inside a helper: a regex error, which the function reports and
/// answers with its default value, or a size limit, which fails the expression.
enum Fail {
    Regex(String),
    Limit,
}

fn err<T>(msg: impl Into<String>) -> R<T> {
    Err(ExprError { msg: msg.into() })
}

/// Vim's message when a result would need too much memory.
fn too_big<T>() -> R<T> {
    err("E342: Out of memory!")
}

fn syntax_error(src: &str) -> ExprError {
    ExprError {
        msg: format!("E15: Invalid expression: \"{src}\""),
    }
}

/// A function error: the message is kept, and the function returns `default`.
fn soft(errs: &mut Vec<String>, msg: impl Into<String>, default: Value) -> R<Value> {
    errs.push(msg.into());
    Ok(default)
}

/// Evaluates `src`, the text after `\=`. The second value holds the messages
/// of errors inside built-in functions, which did not stop the evaluation.
pub fn eval(src: &str, env: &Env) -> R<(Value, Vec<String>)> {
    let toks = lex(src)?;
    let marker = 0u8;
    let mut p = Parser {
        src,
        toks,
        i: 0,
        env,
        errs: Vec::new(),
        depth: 0,
        stack_base: stack_address(&marker),
        used: 0,
    };
    let v = p.expr1(true)?;
    if !matches!(p.tok(), Tok::End) {
        return err(format!("E488: Trailing characters: {}", &src[p.pos()..]));
    }
    Ok((v, p.errs))
}

/// The text a `:s` replacement `\=expr` gives, with the messages of errors in
/// built-in functions. A number or a string is used as it is. A list gives
/// each item followed by a line break, as Vim's do_sub() does.
pub fn eval_text(src: &str, env: &Env) -> R<(String, Vec<String>)> {
    let (value, errs) = eval(src, env)?;
    let text = match value {
        Value::List(items) => {
            let mut out = String::new();
            for item in &items {
                out.push_str(&item_text(item));
                out.push('\n');
                if out.len() > MAX_BYTES {
                    return too_big();
                }
            }
            out
        }
        v => item_text(&v),
    };
    if text.len() > MAX_BYTES {
        return too_big();
    }
    Ok((text, errs))
}

fn item_text(v: &Value) -> String {
    match v {
        Value::Num(n) => n.to_string(),
        Value::Str(s) => s.clone(),
        Value::List(_) => repr(v),
    }
}

// ---- lexer -----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(i64),
    Str(String),
    Name(String),
    Op(&'static str),
    End,
}

/// Operators. Where one is a prefix of another, the longer one comes first.
const OPS: &[&str] = &[
    "==#", "==?", "!=#", "!=?", ">=#", ">=?", "<=#", "<=?", "=~#", "=~?", "!~#", "!~?", ">#", ">?",
    "<#", "<?", "==", "!=", ">=", "<=", "=~", "!~", "..", "||", "&&", "+", "-", "*", "/", "%", "!",
    ".", ">", "<", "(", ")", "[", "]", ",", ":", "?",
];

/// The tokens of `src`, each with its byte offset.
fn lex(src: &str) -> R<Vec<(Tok, usize)>> {
    let mut toks = Vec::new();
    let mut rest = src;
    loop {
        rest = rest.trim_start_matches([' ', '\t']);
        let pos = src.len() - rest.len();
        let Some(c) = rest.chars().next() else {
            toks.push((Tok::End, pos));
            return Ok(toks);
        };
        let (tok, len) = if c.is_ascii_digit() {
            let (n, len) = lex_number(rest).ok_or_else(|| syntax_error(src))?;
            (Tok::Num(n), len)
        } else if c == '\'' {
            let (s, len) = lex_single(rest).ok_or_else(|| syntax_error(src))?;
            (Tok::Str(s), len)
        } else if c == '"' {
            let (s, len) = lex_double(rest).ok_or_else(|| syntax_error(src))?;
            (Tok::Str(s), len)
        } else if c.is_alphabetic() || c == '_' {
            let len = rest
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .unwrap_or(rest.len());
            (Tok::Name(rest[..len].to_string()), len)
        } else if let Some(op) = OPS.iter().find(|op| rest.starts_with(**op)) {
            (Tok::Op(op), op.len())
        } else {
            return Err(syntax_error(src));
        };
        toks.push((tok, pos));
        rest = &rest[len..];
    }
}

/// A decimal, hexadecimal (`0x`) or octal (leading zero) number. A float is
/// not supported and is an error, not a concatenation.
fn lex_number(s: &str) -> Option<(i64, usize)> {
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        let n = h.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(h.len());
        if n == 0 {
            return None;
        }
        let v = magnitude(&h[..n], 16).min(i64::MAX as u128) as i64;
        return Some((v, 2 + n));
    }
    let n = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if s[n..].starts_with('.') && s[n + 1..].starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let digits = &s[..n];
    let octal = digits.len() > 1
        && digits.starts_with('0')
        && digits.bytes().all(|b| (b'0'..=b'7').contains(&b));
    let radix = if octal { 8 } else { 10 };
    let v = magnitude(digits, radix).min(i64::MAX as u128) as i64;
    Some((v, n))
}

/// The characters after an opening quote, with their byte offsets. The text
/// is read as it is needed: a string token does not copy the rest of the
/// expression (that would make lexing quadratic).
type Chars<'a> = std::iter::Peekable<std::str::CharIndices<'a>>;

/// A single-quoted string: `''` is one quote, nothing else is special.
fn lex_single(s: &str) -> Option<(String, usize)> {
    let mut it: Chars = s[1..].char_indices().peekable();
    let mut out = String::new();
    while let Some((i, c)) = it.next() {
        if c == '\'' {
            if it.peek().map(|&(_, n)| n) == Some('\'') {
                it.next();
                out.push('\'');
                continue;
            }
            return Some((out, 1 + i + 1));
        }
        out.push(c);
    }
    None
}

/// The digits in `radix` taken from `it` (at most `max`), added to `first`.
/// Returns the value and the number of digits taken.
fn take_digits(it: &mut Chars, max: usize, radix: u32, first: u32) -> (u32, usize) {
    let mut v = first;
    let mut n = 0;
    while n < max {
        match it.peek().and_then(|&(_, c)| c.to_digit(radix)) {
            Some(d) => {
                v = v.wrapping_mul(radix).wrapping_add(d);
                it.next();
                n += 1;
            }
            None => break,
        }
    }
    (v, n)
}

/// The character of a `\<Name>` key in a double-quoted string.
fn key_code(name: &str) -> Option<char> {
    Some(match name.to_ascii_lowercase().as_str() {
        "esc" => '\u{1b}',
        "cr" | "enter" | "return" => '\r',
        "nl" => '\n',
        "tab" => '\t',
        "bs" => '\u{8}',
        "space" => ' ',
        "lt" => '<',
        "bslash" => '\\',
        "bar" => '|',
        "nul" => '\0',
        _ => return None,
    })
}

/// A double-quoted string with backslash escapes. An unknown escape gives the
/// character after the backslash.
fn lex_double(s: &str) -> Option<(String, usize)> {
    let mut it: Chars = s[1..].char_indices().peekable();
    let mut out = String::new();
    while let Some((i, c)) = it.next() {
        if c == '"' {
            return Some((out, 1 + i + 1));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let (j, d) = it.next()?;
        match d {
            'b' => out.push('\u{8}'),
            'e' => out.push('\u{1b}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'x' | 'X' | 'u' | 'U' => {
                let max = match d {
                    'x' | 'X' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let (v, n) = take_digits(&mut it, max, 16, 0);
                if n == 0 {
                    out.push(d);
                } else {
                    out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                }
            }
            '0'..='7' => {
                let first = d.to_digit(8).unwrap_or(0);
                let (v, _) = take_digits(&mut it, 2, 8, first);
                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            '<' => {
                // A key name is short: look at most 16 characters ahead.
                let ahead = &s[1 + j + 1..];
                let name: String = ahead.chars().take_while(|&c| c != '>').take(16).collect();
                let len = name.chars().count();
                let closed = ahead[name.len()..].starts_with('>');
                match key_code(&name) {
                    Some(c) if closed => {
                        out.push(c);
                        // The name and the `>` are consumed.
                        for _ in 0..=len {
                            it.next();
                        }
                    }
                    _ => out.push('<'),
                }
            }
            other => out.push(other),
        }
    }
    None
}

// ---- parser and evaluator --------------------------------------------------

struct Parser<'a> {
    src: &'a str,
    toks: Vec<(Tok, usize)>,
    i: usize,
    env: &'a Env,
    /// Messages of errors in built-in functions.
    errs: Vec<String>,
    /// Levels of nesting of `expr1()` now active.
    depth: usize,
    /// Address of a local in `eval()`: the stack used is measured from it.
    stack_base: usize,
    /// Bytes of the function results kept so far (see [`MAX_TOTAL`]).
    used: usize,
}

/// The address of `marker`, as a number (a local of the caller).
fn stack_address(marker: &u8) -> usize {
    marker as *const u8 as usize
}

/// A comparison: the relation.
#[derive(Clone, Copy, PartialEq)]
enum Rel {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Match,
    NoMatch,
}

/// A comparison operator: its relation, and the case rule (`None` means
/// 'ignorecase').
fn cmp_op(op: &str) -> Option<(Rel, Option<bool>)> {
    let (name, ic) = if let Some(b) = op.strip_suffix('#') {
        (b, Some(false))
    } else if let Some(b) = op.strip_suffix('?') {
        (b, Some(true))
    } else {
        (op, None)
    };
    let rel = match name {
        "==" => Rel::Eq,
        "!=" => Rel::Ne,
        ">" => Rel::Gt,
        ">=" => Rel::Ge,
        "<" => Rel::Lt,
        "<=" => Rel::Le,
        "=~" => Rel::Match,
        "!~" => Rel::NoMatch,
        _ => return None,
    };
    Some((rel, ic))
}

// The grammar, lowest precedence first. `ev` is false inside a branch that is
// not taken: it is parsed (for its syntax) but not evaluated, as in Vim.
impl Parser<'_> {
    fn tok(&self) -> &Tok {
        &self.toks[self.i].0
    }

    fn pos(&self) -> usize {
        self.toks[self.i].1
    }

    fn next(&mut self) {
        if self.i + 1 < self.toks.len() {
            self.i += 1;
        }
    }

    fn op(&self) -> Option<&'static str> {
        match self.tok() {
            Tok::Op(o) => Some(*o),
            _ => None,
        }
    }

    fn is_op(&self, want: &str) -> bool {
        self.op() == Some(want)
    }

    fn expect(&mut self, want: &str) -> R<()> {
        if self.is_op(want) {
            self.next();
            Ok(())
        } else {
            Err(syntax_error(self.src))
        }
    }

    /// Counts a function result against [`MAX_TOTAL`].
    fn charge(&mut self, v: &Value) -> R<()> {
        self.used = self.used.saturating_add(value_size(v));
        if self.used > MAX_TOTAL {
            return too_big();
        }
        Ok(())
    }

    /// One level of nesting: every parenthesis, list and argument comes
    /// through here, so the depth is counted here.
    fn expr1(&mut self, ev: bool) -> R<Value> {
        let marker = 0u8;
        let stack_used = stack_address(&marker).abs_diff(self.stack_base);
        if self.depth >= MAX_DEPTH || stack_used > STACK_LIMIT {
            return err(format!(
                "E1169: Expression too recursive: {}",
                &self.src[self.pos()..]
            ));
        }
        self.depth += 1;
        let v = self.ternary(ev);
        self.depth -= 1;
        v
    }

    /// `a ? b : c`
    fn ternary(&mut self, ev: bool) -> R<Value> {
        let cond = self.expr2(ev)?;
        if !self.is_op("?") {
            return Ok(cond);
        }
        self.next();
        let c = if ev { truthy(&cond)? } else { false };
        let a = self.expr1(ev && c)?;
        if !self.is_op(":") {
            return err("E109: Missing ':' after '?'");
        }
        self.next();
        let b = self.expr1(ev && !c)?;
        Ok(if !ev {
            Value::Num(0)
        } else if c {
            a
        } else {
            b
        })
    }

    /// `a || b`, short-circuit.
    fn expr2(&mut self, ev: bool) -> R<Value> {
        let mut left = self.expr3(ev)?;
        while self.is_op("||") {
            self.next();
            let l = if ev { truthy(&left)? } else { false };
            let right = self.expr3(ev && !l)?;
            let v = if !ev {
                0
            } else if l {
                1
            } else {
                truthy(&right)? as i64
            };
            left = Value::Num(v);
        }
        Ok(left)
    }

    /// `a && b`, short-circuit.
    fn expr3(&mut self, ev: bool) -> R<Value> {
        let mut left = self.expr4(ev)?;
        while self.is_op("&&") {
            self.next();
            let l = if ev { truthy(&left)? } else { true };
            let right = self.expr4(ev && l)?;
            let v = if !ev || !l { 0 } else { truthy(&right)? as i64 };
            left = Value::Num(v);
        }
        Ok(left)
    }

    /// `a == b`, `a =~ b`, `a < b` and their `#` and `?` forms. Comparisons do
    /// not chain: `a < b < c` is a syntax error, as in Vim.
    fn expr4(&mut self, ev: bool) -> R<Value> {
        let left = self.expr5(ev)?;
        let Some((rel, ic)) = self.op().and_then(cmp_op) else {
            return Ok(left);
        };
        self.next();
        let right = self.expr5(ev)?;
        if ev {
            compare(rel, ic, &left, &right, self.env)
        } else {
            Ok(Value::Num(0))
        }
    }

    /// `a + b`, `a - b`, `a . b` and `a .. b`: one level, left to right.
    fn expr5(&mut self, ev: bool) -> R<Value> {
        let mut left = self.expr6(ev)?;
        while let Some(op) = self.op()
            && matches!(op, "+" | "-" | "." | "..")
        {
            self.next();
            let right = self.expr6(ev)?;
            left = if ev {
                arith(op, std::mem::replace(&mut left, Value::Num(0)), right)?
            } else {
                Value::Num(0)
            };
        }
        Ok(left)
    }

    /// `a * b`, `a / b`, `a % b`.
    fn expr6(&mut self, ev: bool) -> R<Value> {
        let mut left = self.expr7(ev)?;
        while let Some(op) = self.op()
            && matches!(op, "*" | "/" | "%")
        {
            self.next();
            let right = self.expr7(ev)?;
            left = if ev {
                arith(op, std::mem::replace(&mut left, Value::Num(0)), right)?
            } else {
                Value::Num(0)
            };
        }
        Ok(left)
    }

    /// Prefix `!`, `-` and `+`. A chain of them is a loop, not recursion.
    fn expr7(&mut self, ev: bool) -> R<Value> {
        let mut ops = Vec::new();
        while let Some(op) = self.op()
            && matches!(op, "!" | "-" | "+")
        {
            ops.push(op);
            self.next();
        }
        let mut v = self.expr8(ev)?;
        for op in ops.into_iter().rev() {
            if ev {
                v = unary(op, &v)?;
            }
        }
        Ok(v)
    }

    /// Indexing and slicing: `x[i]`, `x[a:b]`, `x[:b]`, `x[a:]`.
    fn expr8(&mut self, ev: bool) -> R<Value> {
        let mut v = self.primary(ev)?;
        while self.is_op("[") {
            self.next();
            let lo = if self.is_op(":") {
                None
            } else {
                Some(self.expr1(ev)?)
            };
            if self.is_op(":") {
                self.next();
                let hi = if self.is_op("]") {
                    None
                } else {
                    Some(self.expr1(ev)?)
                };
                self.expect("]")?;
                if ev {
                    v = slice(&v, lo.as_ref(), hi.as_ref())?;
                }
            } else {
                self.expect("]")?;
                if ev {
                    v = match lo {
                        Some(i) => index(&v, &i)?,
                        None => return Err(syntax_error(self.src)),
                    };
                }
            }
        }
        Ok(v)
    }

    /// Numbers, strings, lists, `( expr )`, variables and function calls.
    fn primary(&mut self, ev: bool) -> R<Value> {
        match self.tok().clone() {
            Tok::Num(n) => {
                self.next();
                Ok(Value::Num(n))
            }
            Tok::Str(s) => {
                self.next();
                Ok(Value::Str(s))
            }
            Tok::Op("[") => {
                self.next();
                let mut items = Vec::new();
                while !self.is_op("]") {
                    items.push(self.expr1(ev)?);
                    if !self.is_op(",") {
                        break;
                    }
                    self.next();
                }
                self.expect("]")?;
                Ok(Value::List(items))
            }
            Tok::Op("(") => {
                self.next();
                let v = self.expr1(ev)?;
                if !self.is_op(")") {
                    return err("E110: Missing ')'");
                }
                self.next();
                Ok(v)
            }
            Tok::Name(name) => {
                self.next();
                if !self.is_op("(") {
                    return if ev {
                        err(format!("E121: Undefined variable: {name}"))
                    } else {
                        Ok(Value::Num(0))
                    };
                }
                self.next();
                let mut args = Vec::new();
                while !self.is_op(")") {
                    args.push(self.expr1(ev)?);
                    if !self.is_op(",") {
                        break;
                    }
                    self.next();
                }
                if !self.is_op(")") {
                    return err("E110: Missing ')'");
                }
                self.next();
                if !ev {
                    return Ok(Value::Num(0));
                }
                let v = call(&name, &args, self.env, &mut self.errs)?;
                self.charge(&v)?;
                Ok(v)
            }
            _ => Err(syntax_error(self.src)),
        }
    }
}

// ---- values and operators --------------------------------------------------

/// A string as a number: optional white space and sign, then digits. Anything
/// else gives 0 ("3x" is 3, "x3" is 0).
fn str_to_num(s: &str) -> i64 {
    let t = s.trim_start_matches([' ', '\t']);
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    // Clamped as Vim's string-to-number does: a negative one to the smallest.
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let m = magnitude(&digits, 10);
    if neg {
        if m > i64::MAX as u128 {
            i64::MIN
        } else {
            -(m as i64)
        }
    } else {
        m.min(i64::MAX as u128) as i64
    }
}

fn to_num(v: &Value) -> R<i64> {
    match v {
        Value::Num(n) => Ok(*n),
        Value::Str(s) => Ok(str_to_num(s)),
        Value::List(_) => err("E745: Using a List as a Number"),
    }
}

fn truthy(v: &Value) -> R<bool> {
    Ok(to_num(v)? != 0)
}

fn to_str(v: &Value) -> R<String> {
    match v {
        Value::Num(n) => Ok(n.to_string()),
        Value::Str(s) => Ok(s.clone()),
        Value::List(_) => err("E730: Using a List as a String"),
    }
}

/// The text of `v` as `string()` gives it: numbers, quoted strings, lists.
fn repr(v: &Value) -> String {
    match v {
        Value::Num(n) => n.to_string(),
        Value::Str(s) => format!("'{}'", s.replace('\'', "''")),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(repr).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

/// Orders two values that are not both lists: strings compare as strings
/// (without case if `ic`), anything else as numbers.
fn ordering(a: &Value, b: &Value, ic: bool) -> R<Ordering> {
    match (a, b) {
        (Value::List(_), _) | (_, Value::List(_)) => err("E692: Invalid operation for List"),
        (Value::Str(x), Value::Str(y)) => Ok(if ic {
            x.to_lowercase().cmp(&y.to_lowercase())
        } else {
            x.cmp(y)
        }),
        _ => Ok(to_num(a)?.cmp(&to_num(b)?)),
    }
}

/// Equality: lists are equal when their items are.
fn equal(a: &Value, b: &Value, ic: bool) -> R<bool> {
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            if x.len() != y.len() {
                return Ok(false);
            }
            for (p, q) in x.iter().zip(y) {
                if !equal(p, q, ic)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(ordering(a, b, ic)? == Ordering::Equal),
    }
}

fn compare(rel: Rel, ic: Option<bool>, a: &Value, b: &Value, env: &Env) -> R<Value> {
    let ic = ic.unwrap_or(env.ignorecase);
    let hit = match rel {
        Rel::Match | Rel::NoMatch => {
            let found = regex_matches(&to_str(a)?, &to_str(b)?, ic).map_err(eval_err)?;
            found == (rel == Rel::Match)
        }
        Rel::Eq => equal(a, b, ic)?,
        Rel::Ne => !equal(a, b, ic)?,
        Rel::Gt => ordering(a, b, ic)? == Ordering::Greater,
        Rel::Ge => ordering(a, b, ic)? != Ordering::Less,
        Rel::Lt => ordering(a, b, ic)? == Ordering::Less,
        Rel::Le => ordering(a, b, ic)? != Ordering::Greater,
    };
    Ok(Value::Num(hit as i64))
}

/// Integer division as Vim does it: by zero the result is the largest number
/// (0 gives the smallest).
fn divide(x: i64, y: i64) -> i64 {
    if y == 0 {
        return match x {
            0 => i64::MIN,
            x if x > 0 => i64::MAX,
            _ => -i64::MAX,
        };
    }
    x.wrapping_div(y)
}

fn arith(op: &str, a: Value, b: Value) -> R<Value> {
    match op {
        // The left string is taken, not copied, so a chain of `.` appends in
        // place.
        "." | ".." => {
            let mut s = into_str(a)?;
            let rhs = into_str(b)?;
            if s.len() + rhs.len() > MAX_BYTES {
                return too_big();
            }
            s.push_str(&rhs);
            Ok(Value::Str(s))
        }
        _ => {
            let (x, y) = (to_num(&a)?, to_num(&b)?);
            Ok(Value::Num(match op {
                "+" => x.wrapping_add(y),
                "-" => x.wrapping_sub(y),
                "*" => x.wrapping_mul(y),
                "/" => divide(x, y),
                _ => {
                    if y == 0 {
                        0
                    } else {
                        x.wrapping_rem(y)
                    }
                }
            }))
        }
    }
}

fn unary(op: &str, v: &Value) -> R<Value> {
    Ok(Value::Num(match op {
        "!" => !truthy(v)? as i64,
        "-" => to_num(v)?.wrapping_neg(),
        _ => to_num(v)?,
    }))
}

/// `x[i]`: an item of a list (negative counts from the end), or a byte of a
/// string ("" when out of range). A number is indexed as its text.
fn index(v: &Value, i: &Value) -> R<Value> {
    match v {
        Value::List(items) => {
            let n = to_num(i)?;
            let len = items.len() as i64;
            let at = if n < 0 { n.saturating_add(len) } else { n };
            if at < 0 || at >= len {
                return err(format!("E684: List index out of range: {n}"));
            }
            Ok(items[at as usize].clone())
        }
        Value::Num(n) => index(&Value::Str(n.to_string()), i),
        Value::Str(s) => {
            let b = s.as_bytes();
            let n = to_num(i)?;
            if n < 0 || n as usize >= b.len() {
                return Ok(Value::Str(String::new()));
            }
            Ok(Value::Str(lossy(&b[n as usize..n as usize + 1])))
        }
    }
}

/// `x[lo:hi]`, both ends inclusive and optional, negative from the end.
fn slice(v: &Value, lo: Option<&Value>, hi: Option<&Value>) -> R<Value> {
    if let Value::Num(n) = v {
        return slice(&Value::Str(n.to_string()), lo, hi);
    }
    let len = (match v {
        Value::List(items) => items.len(),
        Value::Str(s) => s.len(),
        Value::Num(_) => 0,
    }) as i64;
    let lo = match lo {
        Some(x) => to_num(x)?,
        None => 0,
    };
    let hi = match hi {
        Some(x) => to_num(x)?,
        None => len - 1,
    };
    let a = (if lo < 0 { lo.saturating_add(len) } else { lo }).max(0);
    let b = (if hi < 0 { hi.saturating_add(len) } else { hi }).min(len - 1);
    Ok(match v {
        Value::List(items) => Value::List(if a > b {
            Vec::new()
        } else {
            items[a as usize..=b as usize].to_vec()
        }),
        Value::Str(s) => Value::Str(if a > b {
            String::new()
        } else {
            lossy(&s.as_bytes()[a as usize..=b as usize])
        }),
        Value::Num(_) => Value::Str(String::new()),
    })
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

// ---- regular expressions on strings ------------------------------------------

/// One line of text, for the regex engine.
struct Text(Rc<Vec<char>>);

impl Lines for Text {
    fn line_count(&self) -> usize {
        1
    }

    fn chars(&self, _line: usize) -> Vec<char> {
        self.0.as_ref().clone()
    }

    fn shared_chars(&self, _line: usize) -> Rc<Vec<char>> {
        self.0.clone()
    }
}

fn eval_err(msg: String) -> ExprError {
    ExprError { msg }
}

fn compile(pat: &str, ic: bool) -> Result<Regex, String> {
    Regex::new(
        pat,
        &Opts {
            ignorecase: ic,
            ..Opts::default()
        },
    )
}

fn regex_matches(s: &str, pat: &str, ic: bool) -> Result<bool, String> {
    let re = compile(pat, ic)?;
    Ok(re.is_match(&Text(Rc::new(s.chars().collect())), 0))
}

/// `substitute()`: the replacement expands `&`, `\&`, `\0` to `\9` and
/// `\t`, `\n`, `\r`, `\\`. The case escapes (`\u`, `\U` and so on) and `~`
/// are not supported here.
fn substitute_text(s: &str, pat: &str, rep: &str, global: bool, ic: bool) -> Result<String, Fail> {
    let re = compile(pat, ic).map_err(Fail::Regex)?;
    let chars: Rc<Vec<char>> = Rc::new(s.chars().collect());
    let text = Text(chars.clone());
    let rep_chars: Vec<char> = rep.chars().collect();
    // Each `&` or `\` of the replacement can stand for the whole match.
    let refs = rep_chars.iter().filter(|c| matches!(c, '&' | '\\')).count();
    let mut out = String::new();
    let mut copy = 0;
    let mut col = 0;
    while col <= chars.len() {
        let Some(m) = re.match_line(&text, 0, col) else {
            break;
        };
        let (ms, me) = (m.start.1, m.end.1);
        // The projected size is checked before the match is appended (at
        // 4 bytes per character, which is an upper bound).
        if out
            .len()
            .saturating_add(rep.len())
            .saturating_add(refs * 4 * me.saturating_sub(ms))
            > MAX_BYTES
        {
            return Err(Fail::Limit);
        }
        out.extend(&chars[copy.min(ms)..ms]);
        if refs == 0 {
            out.extend(rep_chars.iter());
        } else {
            out.push_str(&expand_sub(&rep_chars, &m, &chars));
        }
        copy = me;
        if !global {
            break;
        }
        col = if me == ms { me + 1 } else { me };
    }
    out.extend(&chars[copy.min(chars.len())..]);
    if out.len() > MAX_BYTES {
        return Err(Fail::Limit);
    }
    Ok(out)
}

/// The replacement `rep` for match `m` of `chars`.
fn expand_sub(r: &[char], m: &Match, chars: &[char]) -> String {
    let group = |g: usize| -> String {
        let span = if g == 0 {
            Some((m.start, m.end))
        } else {
            m.groups[g]
        };
        match span {
            Some(((_, a), (_, b))) => {
                let (a, b) = (a.min(chars.len()), b.min(chars.len()));
                if a < b {
                    chars[a..b].iter().collect()
                } else {
                    String::new()
                }
            }
            None => String::new(),
        }
    };
    let mut out = String::new();
    let mut i = 0;
    while i < r.len() {
        match r[i] {
            '&' => out.push_str(&group(0)),
            '\\' if i + 1 < r.len() => {
                i += 1;
                match r[i] {
                    d @ '0'..='9' => out.push_str(&group(d as usize - '0' as usize)),
                    '&' => out.push('&'),
                    'n' => out.push('\0'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    c => out.push(c),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// `split()` on white space: runs of spaces and tabs separate the items.
fn split_white(s: &str) -> Result<Vec<String>, Fail> {
    let mut items = Vec::new();
    for w in s.split([' ', '\t']).filter(|w| !w.is_empty()) {
        if items.len() >= MAX_ITEMS {
            return Err(Fail::Limit);
        }
        items.push(w.to_string());
    }
    Ok(items)
}

/// `split()` with a pattern. Empty items are dropped at the start and at the
/// end (unless `keep`); a match of nothing does not split.
fn split_pattern(s: &str, pat: &str, ic: bool, keep: bool) -> Result<Vec<String>, Fail> {
    let re = compile(pat, ic).map_err(Fail::Regex)?;
    let chars: Rc<Vec<char>> = Rc::new(s.chars().collect());
    let text = Text(chars.clone());
    let mut items = Vec::new();
    let (mut start, mut col) = (0, 0);
    while col <= chars.len() {
        let Some(m) = re.match_line(&text, 0, col) else {
            break;
        };
        let (ms, me) = (m.start.1, m.end.1);
        if me == ms {
            col = ms + 1;
            continue;
        }
        if items.len() >= MAX_ITEMS {
            return Err(Fail::Limit);
        }
        items.push(chars[start..ms].iter().collect::<String>());
        start = me;
        col = me;
    }
    items.push(chars[start..].iter().collect());
    if !keep {
        while items.first().is_some_and(String::is_empty) {
            items.remove(0);
        }
        while items.last().is_some_and(String::is_empty) {
            items.pop();
        }
    }
    Ok(items)
}

// ---- built-in functions ------------------------------------------------------

/// The number of arguments of each built-in function, as (least, most).
fn arity(name: &str) -> Option<(usize, usize)> {
    Some(match name {
        "abs" | "char2nr" | "col" | "len" | "line" | "max" | "min" | "nr2char" | "reverse"
        | "string" | "strlen" | "submatch" | "tolower" | "toupper" => (1, 1),
        "escape" | "repeat" => (2, 2),
        "strpart" => (2, 3),
        "trim" | "str2nr" | "join" => (1, 2),
        "split" => (1, 3),
        "range" => (1, 3),
        "substitute" => (4, 4),
        "printf" => (1, usize::MAX),
        _ => return None,
    })
}

fn call(name: &str, args: &[Value], env: &Env, errs: &mut Vec<String>) -> R<Value> {
    let Some((least, most)) = arity(name) else {
        return err(format!("E117: Unknown function: {name}"));
    };
    if args.len() < least {
        return err(format!("E119: Not enough arguments for function: {name}"));
    }
    if args.len() > most {
        return err(format!("E118: Too many arguments for function: {name}"));
    }
    match name {
        "abs" => Ok(Value::Num(to_num(&args[0])?.wrapping_abs())),
        "char2nr" => Ok(Value::Num(
            to_str(&args[0])?.chars().next().map_or(0, |c| c as i64),
        )),
        "col" => Ok(Value::Num(if to_str(&args[0])? == "." {
            env.col
        } else {
            0
        })),
        "escape" => {
            let s = to_str(&args[0])?;
            let chars = to_str(&args[1])?;
            let mut out = String::new();
            for c in s.chars() {
                if chars.contains(c) {
                    out.push('\\');
                }
                out.push(c);
                if out.len() > MAX_BYTES {
                    return too_big();
                }
            }
            Ok(Value::Str(out))
        }
        "join" => {
            let Value::List(items) = &args[0] else {
                return soft(
                    errs,
                    "E1529: List or Tuple required for argument 1",
                    Value::Str(String::new()),
                );
            };
            let sep = match args.get(1) {
                Some(s) => to_str(s)?,
                None => " ".to_string(),
            };
            let parts: Vec<String> = items.iter().map(to_str).collect::<R<_>>()?;
            let total: usize = parts.iter().map(|p| p.len() + sep.len()).sum();
            if total > MAX_BYTES {
                return too_big();
            }
            Ok(Value::Str(parts.join(sep.as_str())))
        }
        "len" => Ok(Value::Num(match &args[0] {
            Value::List(items) => items.len() as i64,
            v => to_str(v)?.len() as i64,
        })),
        "line" => Ok(Value::Num(match to_str(&args[0])?.as_str() {
            "." => env.line,
            "$" => env.last_line,
            _ => 0,
        })),
        "max" | "min" => {
            let Value::List(items) = &args[0] else {
                return soft(
                    errs,
                    format!("E712: Argument of {name}() must be a List or Dictionary"),
                    Value::Num(0),
                );
            };
            let nums: Vec<i64> = items.iter().map(to_num).collect::<R<_>>()?;
            let pick = if name == "max" {
                nums.iter().max()
            } else {
                nums.iter().min()
            };
            Ok(Value::Num(pick.copied().unwrap_or(0)))
        }
        "nr2char" => {
            let n = to_num(&args[0])?;
            Ok(Value::Str(
                char::from_u32(n as u32)
                    .map(String::from)
                    .unwrap_or_default(),
            ))
        }
        "printf" => printf(args, errs),
        "range" => range(args, errs),
        "repeat" => {
            let n = to_num(&args[1])?.max(0) as usize;
            match &args[0] {
                Value::List(items) => {
                    if items.is_empty() || n == 0 {
                        return Ok(Value::List(Vec::new()));
                    }
                    if items.len().saturating_mul(n) > MAX_ITEMS
                        || value_size(&args[0]).saturating_mul(n) > MAX_BYTES
                    {
                        return too_big();
                    }
                    Ok(Value::List(
                        (0..n).flat_map(|_| items.iter().cloned()).collect(),
                    ))
                }
                v => {
                    let s = to_str(v)?;
                    if s.is_empty() || n == 0 {
                        return Ok(Value::Str(String::new()));
                    }
                    if s.len().saturating_mul(n) > MAX_BYTES {
                        return too_big();
                    }
                    Ok(Value::Str(s.repeat(n)))
                }
            }
        }
        "reverse" => match &args[0] {
            Value::List(items) => Ok(Value::List(items.iter().rev().cloned().collect())),
            Value::Str(s) => Ok(Value::Str(s.chars().rev().collect())),
            _ => soft(
                errs,
                "E1253: String, List, Tuple or Blob required for argument 1",
                Value::Num(0),
            ),
        },
        "split" => {
            let s = to_str(&args[0])?;
            let keep = match args.get(2) {
                Some(k) => to_num(k)? != 0,
                None => false,
            };
            let found = match args.get(1) {
                None => split_white(&s),
                Some(p) => split_pattern(&s, &to_str(p)?, env.ignorecase, keep),
            };
            match found {
                Ok(items) => Ok(Value::List(items.into_iter().map(Value::Str).collect())),
                Err(Fail::Regex(msg)) => soft(errs, msg, Value::List(Vec::new())),
                Err(Fail::Limit) => too_big(),
            }
        }
        "str2nr" => {
            let base = match args.get(1) {
                Some(b) => to_num(b)?,
                None => 10,
            };
            if !matches!(base, 2 | 8 | 10 | 16) {
                return soft(errs, "E474: Invalid argument", Value::Num(0));
            }
            let s = to_str(&args[0])?;
            let t = s.trim_start();
            let (neg, t) = match t.strip_prefix('-') {
                Some(r) => (true, r),
                None => (false, t.strip_prefix('+').unwrap_or(t)),
            };
            let t = match base {
                16 => t
                    .strip_prefix("0x")
                    .or_else(|| t.strip_prefix("0X"))
                    .unwrap_or(t),
                2 => t
                    .strip_prefix("0b")
                    .or_else(|| t.strip_prefix("0B"))
                    .unwrap_or(t),
                _ => t,
            };
            let radix = base as u32;
            let digits: String = t.chars().take_while(|c| c.is_digit(radix)).collect();
            // Clamped like Vim's str2nr(): a negative number stops at -MAX.
            let v = magnitude(&digits, radix).min(i64::MAX as u128) as i64;
            Ok(Value::Num(if neg { -v } else { v }))
        }
        "string" => Ok(Value::Str(repr(&args[0]))),
        "strlen" => Ok(Value::Num(to_str(&args[0])?.len() as i64)),
        "strpart" => {
            let s = to_str(&args[0])?;
            let b = s.as_bytes();
            let mut start = to_num(&args[1])?;
            let mut len = match args.get(2) {
                Some(l) => to_num(l)?,
                None => b.len() as i64,
            };
            if start < 0 {
                len = len.saturating_add(start);
                start = 0;
            }
            if len <= 0 || start >= b.len() as i64 {
                return Ok(Value::Str(String::new()));
            }
            let end = start.saturating_add(len).min(b.len() as i64);
            Ok(Value::Str(lossy(&b[start as usize..end as usize])))
        }
        "submatch" => {
            let n = to_num(&args[0])?;
            if !(0..=9).contains(&n) {
                return soft(
                    errs,
                    format!("E935: Invalid submatch number: {n}"),
                    Value::Num(0),
                );
            }
            Ok(Value::Str(env.groups[n as usize].clone()))
        }
        "substitute" => {
            let s = to_str(&args[0])?;
            let pat = to_str(&args[1])?;
            let rep = to_str(&args[2])?;
            let flags = to_str(&args[3])?;
            match substitute_text(&s, &pat, &rep, flags.contains('g'), env.ignorecase) {
                Ok(text) => Ok(Value::Str(text)),
                Err(Fail::Regex(msg)) => soft(errs, msg, Value::Str(String::new())),
                Err(Fail::Limit) => too_big(),
            }
        }
        "tolower" | "toupper" => {
            let upper = name == "toupper";
            let mut out = String::new();
            for c in to_str(&args[0])?.chars() {
                out.push(one_char_case(c, upper));
                if out.len() > MAX_BYTES {
                    return too_big();
                }
            }
            Ok(Value::Str(out))
        }
        "trim" => {
            let mask = match args.get(1) {
                Some(m) => to_str(m)?,
                None => " \t\r\n".to_string(),
            };
            let s = to_str(&args[0])?;
            Ok(Value::Str(
                s.trim_matches(|c: char| mask.contains(c)).to_string(),
            ))
        }
        _ => unreachable!("arity() lists every built-in"),
    }
}

/// The case of `c`, when that is one character (as `towupper()` does);
/// otherwise `c` itself.
fn one_char_case(c: char, upper: bool) -> char {
    let mut it = if upper {
        c.to_uppercase().collect::<Vec<char>>()
    } else {
        c.to_lowercase().collect::<Vec<char>>()
    };
    if it.len() == 1 { it.remove(0) } else { c }
}

/// `range(end)`, `range(start, end)` or `range(start, end, step)`. The
/// arithmetic is done in 128 bits, so no argument can overflow.
fn range(args: &[Value], errs: &mut Vec<String>) -> R<Value> {
    let (start, end, step): (i128, i128, i128) = match args.len() {
        1 => (0, to_num(&args[0])? as i128 - 1, 1),
        2 => (to_num(&args[0])? as i128, to_num(&args[1])? as i128, 1),
        _ => (
            to_num(&args[0])? as i128,
            to_num(&args[1])? as i128,
            to_num(&args[2])? as i128,
        ),
    };
    if step == 0 {
        return soft(errs, "E726: Stride is zero", Value::List(Vec::new()));
    }
    if (step > 0 && end + 1 < start) || (step < 0 && end - 1 > start) {
        return soft(errs, "E727: Start past end", Value::List(Vec::new()));
    }
    let count = if (step > 0 && start <= end) || (step < 0 && start >= end) {
        (end - start) / step + 1
    } else {
        0
    };
    if count > MAX_ITEMS as i128 {
        return too_big();
    }
    Ok(Value::List(
        (0..count)
            .map(|k| Value::Num((start + k * step) as i64))
            .collect(),
    ))
}

/// `printf()` with `%d`, `%x`, `%X`, `%s` and `%%`, with the flags `-` and `0`
/// and a width. Too few arguments give 0 and too many are ignored, each with
/// its error message, as in Vim.
fn printf(args: &[Value], errs: &mut Vec<String>) -> R<Value> {
    let fmt: Vec<char> = to_str(&args[0])?.chars().collect();
    let mut out = String::new();
    let mut next = 1;
    let mut i = 0;
    while i < fmt.len() {
        if fmt[i] != '%' {
            out.push(fmt[i]);
            i += 1;
            continue;
        }
        i += 1;
        let (mut left, mut zero) = (false, false);
        while let Some(&c) = fmt.get(i) {
            match c {
                '-' => left = true,
                '0' => zero = true,
                _ => break,
            }
            i += 1;
        }
        let digits_at = i;
        let mut width = 0usize;
        while let Some(d) = fmt.get(i).and_then(|c| c.to_digit(10)) {
            width = width.saturating_mul(10).saturating_add(d as usize);
            i += 1;
        }
        if width > MAX_WIDTH {
            let digits: String = fmt[digits_at..i].iter().collect();
            return err(format!("E1510: Value too large: {digits}"));
        }
        let Some(&conv) = fmt.get(i) else {
            return err("E15: Invalid format string");
        };
        i += 1;
        let (sign, body) = if conv == '%' {
            ("", "%".to_string())
        } else {
            let Some(arg) = args.get(next) else {
                return soft(
                    errs,
                    "E766: Insufficient arguments for printf()",
                    Value::Num(0),
                );
            };
            next += 1;
            match conv {
                'd' => {
                    let n = to_num(arg)?;
                    (if n < 0 { "-" } else { "" }, n.unsigned_abs().to_string())
                }
                'x' => ("", format!("{:x}", to_num(arg)?)),
                'X' => ("", format!("{:X}", to_num(arg)?)),
                's' => match arg {
                    Value::List(_) => ("", repr(arg)),
                    v => ("", to_str(v)?),
                },
                _ => return err("E15: Invalid format string"),
            }
        };
        let pad = width.saturating_sub(sign.len() + body.chars().count());
        if left {
            out.push_str(sign);
            out.push_str(&body);
            out.push_str(&" ".repeat(pad));
        } else if zero && conv != 's' && conv != '%' {
            out.push_str(sign);
            out.push_str(&"0".repeat(pad));
            out.push_str(&body);
        } else {
            out.push_str(&" ".repeat(pad));
            out.push_str(sign);
            out.push_str(&body);
        }
        if out.len() > MAX_BYTES {
            return too_big();
        }
    }
    if next < args.len() {
        return soft(
            errs,
            "E767: Too many arguments for printf()",
            Value::Str(out),
        );
    }
    Ok(Value::Str(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(src: &str) -> i64 {
        match eval(src, &Env::default()) {
            Ok((Value::Num(n), errs)) if errs.is_empty() => n,
            other => panic!("{src}: {other:?}"),
        }
    }

    /// Runs `f` on a thread with a 1 MB stack, the size of the main thread's
    /// stack in the editor, so that stack use is checked in a debug build.
    fn on_small_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap();
    }

    fn fails_with(src: &str, prefix: &str) -> bool {
        matches!(eval(src, &Env::default()), Err(e) if e.msg.starts_with(prefix))
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(num("1+2*3"), 7);
        assert_eq!(num("-7/2"), -3);
        assert_eq!(num("-7%3"), -1);
        assert_eq!(num("7%0"), 0);
        assert_eq!(num("1/0"), i64::MAX);
        assert_eq!(num("0/0"), i64::MIN);
        assert_eq!(num("1 ? 2 : 3 ? 4 : 5"), 2);
        assert_eq!(num("0 ? 2 : 0 ? 4 : 5"), 5);
        assert_eq!(num("0 ? foo : 9"), 9);
    }

    #[test]
    fn errors_have_vim_numbers() {
        let e = eval("foo", &Env::default()).unwrap_err();
        assert_eq!(e.msg, "E121: Undefined variable: foo");
        let e = eval("1 +", &Env::default()).unwrap_err();
        assert_eq!(e.msg, "E15: Invalid expression: \"1 +\"");
        // A function error gives its default value and a message.
        let (v, errs) = eval("range(2,0)", &Env::default()).unwrap();
        assert_eq!(v, Value::List(Vec::new()));
        assert_eq!(errs, ["E727: Start past end"]);
    }

    #[test]
    fn nesting_is_limited_and_fits_a_small_stack() {
        on_small_stack(|| {
            let ok = format!("{}1{}", "(".repeat(10), ")".repeat(10));
            assert_eq!(num(&ok), 1);
            let deep = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
            assert!(fails_with(&deep, "E1169"));
            let lists = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
            assert!(fails_with(&lists, "E1169"));
            let calls = format!("{}''{}", "strlen(".repeat(10_000), ")".repeat(10_000));
            assert!(fails_with(&calls, "E1169"));
            let negs = format!("{}1{}", "-(".repeat(10_000), ")".repeat(10_000));
            assert!(fails_with(&negs, "E1169"));
            // Chains of prefix operators are loops, not recursion.
            assert_eq!(num(&format!("{}1", "!".repeat(100_000))), 1);
            assert_eq!(num(&format!("{}1", "-".repeat(100_000))), 1);
        });
    }

    #[test]
    fn printf_width_is_checked() {
        assert!(fails_with("printf('%99999999999999999999d', 1)", "E1510"));
        assert!(fails_with("printf('%1000001d', 1)", "E1510"));
        assert_eq!(
            eval("len(printf('%20000d', 1))", &Env::default())
                .unwrap()
                .0,
            Value::Num(20000)
        );
    }

    #[test]
    fn ranges_and_repeats_are_limited() {
        // The extremes of the integer arguments: no overflow, no hang.
        let (v, errs) = eval("range(-9223372036854775807-1)", &Env::default()).unwrap();
        assert_eq!(v, Value::List(Vec::new()));
        assert_eq!(errs, ["E727: Start past end"]);
        assert!(fails_with("range(0, 9223372036854775807)", "E342"));
        assert!(fails_with("range(1000000000)", "E342"));
        assert_eq!(num("len(range(0, 3))"), 4);
        assert!(fails_with("repeat('x', 9223372036854775807)", "E342"));
        assert!(fails_with("repeat(['a'], 1000000000)", "E342"));
        assert_eq!(
            eval("repeat('', 9223372036854775807)", &Env::default())
                .unwrap()
                .0,
            Value::Str(String::new())
        );
        assert_eq!(
            eval("repeat([], 9223372036854775807)", &Env::default())
                .unwrap()
                .0,
            Value::List(Vec::new())
        );
        // Each repeat is under the limit, the concatenation is over it.
        assert!(fails_with(
            "repeat('x', 33554433) . repeat('x', 33554433)",
            "E342"
        ));
        assert_eq!(
            eval("strpart('x', -5, -9223372036854775807)", &Env::default())
                .unwrap()
                .0,
            Value::Str(String::new())
        );
        assert_eq!(num("len(join(repeat(['ab'], 100000), ','))"), 299_999);
    }

    /// The large inputs that once hung: each must finish. The times are
    /// printed (run with `--nocapture`) for the record.
    #[test]
    fn big_inputs_finish() {
        use std::time::{Duration, Instant};
        let time = |label: &str, src: &str| {
            let t = Instant::now();
            let r = eval(src, &Env::default());
            let d = t.elapsed();
            eprintln!("{label}: {d:?}");
            assert!(d < Duration::from_secs(60), "{label} took {d:?}");
            r
        };
        let split = time(
            "split 200000 items",
            "len(split(repeat('a,', 200000), ','))",
        );
        assert_eq!(split.unwrap().0, Value::Num(200_000));
        let subst = time(
            "substitute 1000000 matches",
            "substitute(repeat('a',1000000), 'a', repeat('b',100), 'g')",
        );
        assert!(subst.is_err());
        let chain = vec!["'x'"; 100_000].join(" . ");
        let concat = time("concat 100000 terms", &format!("len({chain})"));
        assert_eq!(concat.unwrap().0, Value::Num(100_000));
        let index = time("[1][MIN]", "[1][-9223372036854775807-1]");
        assert!(index.is_err());
    }

    #[test]
    fn split_scales_linearly_with_the_items() {
        use std::time::{Duration, Instant};
        let time = |n: usize| {
            let src = format!("len(split(repeat('a,', {n}), ','))");
            let t = Instant::now();
            assert_eq!(num(&src), n as i64);
            t.elapsed()
        };
        let small = time(20_000);
        let big = time(40_000);
        assert!(small < Duration::from_secs(2), "20000 items took {small:?}");
        // Twice the items, about twice the time: a generous bound.
        assert!(
            big < small * 4 + Duration::from_millis(200),
            "{small:?} vs {big:?}"
        );
    }

    #[test]
    fn substitute_stops_at_the_size_limit_quickly() {
        let t = std::time::Instant::now();
        assert!(fails_with(
            "substitute(repeat('a',1000000), 'a', repeat('b',100), 'g')",
            "E342"
        ));
        assert!(t.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn integers_are_clamped_as_vim_does() {
        // str2nr() and literals: to the largest number, negatives to -MAX.
        assert_eq!(num("str2nr('99999999999999999999')"), i64::MAX);
        assert_eq!(num("str2nr('-99999999999999999999')"), -i64::MAX);
        assert_eq!(num("str2nr('-9223372036854775808')"), -i64::MAX);
        assert_eq!(num("str2nr('ffffffffffffffffff', 16)"), i64::MAX);
        assert_eq!(num("99999999999999999999"), i64::MAX);
        assert_eq!(num("-99999999999999999999"), -i64::MAX);
        assert_eq!(num("0xffffffffffffffffff"), i64::MAX);
        // String-to-number in arithmetic: a negative one to the smallest.
        assert_eq!(num("'99999999999999999999' + 0"), i64::MAX);
        assert_eq!(num("'-99999999999999999999' + 0"), i64::MIN);
    }

    #[test]
    fn concatenation_chains_append_in_place() {
        let t = std::time::Instant::now();
        let src = format!("len({})", vec!["'x'"; 20_000].join(" . "));
        assert_eq!(num(&src), 20_000);
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn total_size_of_kept_values_is_limited() {
        // Each repeat() is under its own limit; the list keeps them all.
        let big = "repeat('x', 60000000)";
        let src = format!("[{}]", [big; 6].join(", "));
        assert!(fails_with(&src, "E342"));
    }

    #[test]
    fn long_values_do_not_panic() {
        on_small_stack(|| {
            assert!(fails_with("[1][-9223372036854775807-1]", "E684"));
            assert_eq!(
                eval("[1,2,3][-9223372036854775807-1:]", &Env::default())
                    .unwrap()
                    .0,
                Value::List(vec![Value::Num(1), Value::Num(2), Value::Num(3)])
            );
            assert_eq!(
                eval(
                    "'abc'[-9223372036854775807-1:-9223372036854775807-1]",
                    &Env::default()
                )
                .unwrap()
                .0,
                Value::Str(String::new())
            );
        });
    }
}
