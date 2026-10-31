//! Keys as the editor core sees them, independent of any terminal library.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// Control + a lowercase ASCII letter or symbol. Aliases that Vim treats
    /// as other keys (`<C-[>`, `<C-m>`, `<C-h>`, `<C-i>`) are normalised by
    /// [`Key::ctrl`].
    Ctrl(char),
    Esc,
    Enter,
    Backspace,
    Tab,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    /// Shift-Tab.
    BackTab,
    /// Function key `F1`..`F12`.
    F(u8),
    /// Alt + a character. Terminal Vim receives this as `<Esc>` followed by
    /// the character when nothing is mapped; the editor does the same.
    Alt(char),
}

impl Key {
    pub fn ctrl(c: char) -> Key {
        match c.to_ascii_lowercase() {
            '[' => Key::Esc,
            'm' => Key::Enter,
            'h' => Key::Backspace,
            'i' => Key::Tab,
            c => Key::Ctrl(c),
        }
    }
}

/// Parses Vim key notation (`"ihello<Esc>:wq<CR>"`). Unknown `<...>` groups
/// are taken literally, as Vim does.
pub fn parse_keys(s: &str) -> Vec<Key> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<'
            && let Some(len) = chars[i + 1..].iter().position(|&c| c == '>')
        {
            let name: String = chars[i + 1..i + 1 + len].iter().collect();
            if let Some(key) = named_key(&name) {
                out.push(key);
                i += len + 2;
                continue;
            }
        }
        out.push(Key::Char(chars[i]));
        i += 1;
    }
    out
}

fn named_key(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "esc" => Key::Esc,
        "cr" | "enter" | "return" => Key::Enter,
        "bs" => Key::Backspace,
        "tab" => Key::Tab,
        "del" => Key::Delete,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "insert" => Key::Insert,
        "s-tab" => Key::BackTab,
        "space" => Key::Char(' '),
        "lt" => Key::Char('<'),
        "bar" => Key::Char('|'),
        "bslash" => Key::Char('\\'),
        _ => {
            if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
                && (1..=12).contains(&n)
            {
                return Some(Key::F(n));
            }
            if let Some(rest) = lower
                .strip_prefix("a-")
                .or_else(|| lower.strip_prefix("m-"))
            {
                let mut it = name[2..].chars();
                let c = it.next()?;
                if rest.chars().count() != 1 {
                    return None;
                }
                return Some(Key::Alt(c));
            }
            let rest = lower.strip_prefix("c-")?;
            let mut it = rest.chars();
            let c = it.next()?;
            if it.next().is_some() {
                return None;
            }
            Key::ctrl(c)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notation() {
        assert_eq!(
            parse_keys("a<Esc><C-r><lt>x<nope>"),
            vec![
                Key::Char('a'),
                Key::Esc,
                Key::Ctrl('r'),
                Key::Char('<'),
                Key::Char('x'),
                Key::Char('<'),
                Key::Char('n'),
                Key::Char('o'),
                Key::Char('p'),
                Key::Char('e'),
                Key::Char('>'),
            ]
        );
        assert_eq!(parse_keys("<C-[><C-M>"), vec![Key::Esc, Key::Enter]);
    }
}
