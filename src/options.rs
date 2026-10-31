//! Typed option registry. Every option is one table entry; `:set` works from
//! the table. Scopes are recorded for the Document/Window split (milestone 4a);
//! until then all values live in one `Options`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Buffer,
    Window,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Val {
    Bool(bool),
    Num(i64),
}

pub struct Def {
    pub name: &'static str,
    pub short: &'static str,
    pub scope: Scope,
    pub default: Val,
}

const fn b(name: &'static str, short: &'static str, scope: Scope, v: bool) -> Def {
    Def {
        name,
        short,
        scope,
        default: Val::Bool(v),
    }
}

const fn n(name: &'static str, short: &'static str, scope: Scope, v: i64) -> Def {
    Def {
        name,
        short,
        scope,
        default: Val::Num(v),
    }
}

/// Defaults are Vim's `-N -u NONE` defaults.
pub const TABLE: &[Def] = &[
    b("ignorecase", "ic", Scope::Global, false),
    b("smartcase", "scs", Scope::Global, false),
    b("wrapscan", "ws", Scope::Global, true),
    b("autoindent", "ai", Scope::Buffer, false),
    b("expandtab", "et", Scope::Buffer, false),
    n("shiftwidth", "sw", Scope::Buffer, 8),
    n("tabstop", "ts", Scope::Buffer, 8),
    n("scrolloff", "so", Scope::Global, 0),
    b("hlsearch", "hls", Scope::Global, false),
    b("incsearch", "is", Scope::Global, false),
    b("number", "nu", Scope::Window, false),
    b("relativenumber", "rnu", Scope::Window, false),
    b("wrap", "wrap", Scope::Window, true),
    b("autoread", "ar", Scope::Global, false),
];

/// Options Vim has that rvim accepts in `:set` and ignores.
pub const INERT: &[&str] = &[
    "list",
    "magic",
    "ru",
    "ruler",
    "sc",
    "showcmd",
    "sol",
    "startofline",
    "js",
    "joinspaces",
    "hid",
    "hidden",
    "si",
    "smartindent",
    "cindent",
    "cin",
];

#[derive(Clone, Debug)]
pub struct Options {
    vals: Vec<Val>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            vals: TABLE.iter().map(|d| d.default.clone()).collect(),
        }
    }
}

impl Options {
    pub fn find(name: &str) -> Option<usize> {
        TABLE.iter().position(|d| d.name == name || d.short == name)
    }

    pub fn get(&self, i: usize) -> &Val {
        &self.vals[i]
    }

    pub fn set(&mut self, i: usize, v: Val) {
        self.vals[i] = v;
    }

    fn bool_of(&self, name: &str) -> bool {
        match Self::find(name).map(|i| &self.vals[i]) {
            Some(Val::Bool(v)) => *v,
            _ => false,
        }
    }

    fn num_of(&self, name: &str) -> i64 {
        match Self::find(name).map(|i| &self.vals[i]) {
            Some(Val::Num(v)) => *v,
            _ => 0,
        }
    }

    pub fn ignorecase(&self) -> bool {
        self.bool_of("ignorecase")
    }
    pub fn smartcase(&self) -> bool {
        self.bool_of("smartcase")
    }
    pub fn wrapscan(&self) -> bool {
        self.bool_of("wrapscan")
    }
    pub fn autoindent(&self) -> bool {
        self.bool_of("autoindent")
    }
    pub fn expandtab(&self) -> bool {
        self.bool_of("expandtab")
    }
    pub fn tabstop(&self) -> usize {
        self.num_of("tabstop").max(1) as usize
    }
    /// `'shiftwidth'`, or `'tabstop'` when it is zero.
    pub fn shiftwidth(&self) -> usize {
        match self.num_of("shiftwidth") {
            0 => self.tabstop(),
            v => v.max(1) as usize,
        }
    }
}

/// What `:set opt?` shows.
pub fn describe(i: usize, v: &Val) -> String {
    let name = TABLE[i].name;
    match v {
        Val::Bool(true) => format!("  {name}"),
        Val::Bool(false) => format!("no{name}"),
        Val::Num(n) => format!("  {name}={n}"),
    }
}
