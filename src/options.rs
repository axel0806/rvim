// SPDX-License-Identifier: Apache-2.0
//! Typed option registry. Every option is one table entry; `:set`,
//! `:setlocal` and `:setglobal` work from the table.
//!
//! Each option has a scope, as Vim's `:help local-options` says (probed with
//! Vim 9.1, see "Options registry" in PLAN.md). Values live in three places:
//!
//! - the global value, in `Options` (owned by `Editor`);
//! - the local value of a document (`Document::local`), for buffer-local and
//!   global-local buffer options;
//! - the local value of a window (`Window::local`), for window-local and
//!   global-local window options.
//!
//! A local value of a buffer-local or window-local option is always set; it
//! starts as a copy of the global value. A global-local option has no local
//! value until one is set (`None`), and then the global value is used.
//! Every read goes through `Eff`, which gives the effective value for one
//! document and window.

use crate::document::Document;
use crate::editor::Editor;
use crate::window::Window;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One value, shared by everything.
    Global,
    /// A global value, and a local value per buffer that wins when set.
    GlobalOrBuffer,
    /// A local value per buffer, copied from the global value for each new
    /// buffer.
    Buffer,
    /// A global value, and a local value per window that wins when set.
    GlobalOrWindow,
    /// A local value per window, copied from the global value for each new
    /// window.
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

/// The scope table. Defaults are Vim's `-N -u NONE` defaults; the scopes are
/// Vim's (`:help local-options` in Vim 9.1).
pub const TABLE: &[Def] = &[
    b("ignorecase", "ic", Scope::Global, false),
    b("smartcase", "scs", Scope::Global, false),
    b("wrapscan", "ws", Scope::Global, true),
    b("autoindent", "ai", Scope::Buffer, false),
    b("expandtab", "et", Scope::Buffer, false),
    n("shiftwidth", "sw", Scope::Buffer, 8),
    n("tabstop", "ts", Scope::Buffer, 8),
    n("softtabstop", "sts", Scope::Buffer, 0),
    b("smarttab", "sta", Scope::Global, false),
    b("shiftround", "sr", Scope::Global, false),
    n("scrolloff", "so", Scope::GlobalOrWindow, 0),
    b("hlsearch", "hls", Scope::Global, false),
    b("incsearch", "is", Scope::Global, false),
    b("number", "nu", Scope::Window, false),
    b("relativenumber", "rnu", Scope::Window, false),
    b("wrap", "wrap", Scope::Window, true),
    b("autoread", "ar", Scope::GlobalOrBuffer, false),
    // Not a Vim option: turns on the start menu (see startmenu.rs). Off by
    // default, so `:set` behaves as in Vim while it is off.
    b("startmenu", "startmenu", Scope::Global, false),
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

impl Scope {
    /// The option can have a local value (of a buffer or window).
    pub fn has_local(self) -> bool {
        self != Scope::Global
    }

    /// A global-local option: the local value may be unset, and then the
    /// global value is used (Vim's `-1`).
    pub fn is_global_local(self) -> bool {
        matches!(self, Scope::GlobalOrBuffer | Scope::GlobalOrWindow)
    }
}

/// Which value a `:set`-family command writes (Vim's `OPT_LOCAL` and
/// `OPT_GLOBAL` flags).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// `:set`: the global value, and the local value when there is one.
    Both,
    /// `:setlocal`: the local value. A global-only option has one value,
    /// so it is set.
    Local,
    /// `:setglobal`: the global value only.
    Global,
}

/// Whether a command with `target` writes the global value and whether it
/// writes the local value (which only options with a local value have).
fn targets(scope: Scope, target: Target) -> (bool, bool) {
    if !scope.has_local() {
        return (true, false);
    }
    (target != Target::Local, target != Target::Global)
}

/// The local values of the document or window that an option of `scope`
/// keeps its local value in.
fn local_of<'a>(scope: Scope, doc: &'a mut Local, win: &'a mut Local) -> &'a mut Local {
    match scope {
        Scope::Buffer | Scope::GlobalOrBuffer => doc,
        _ => win,
    }
}

/// The global values, one per table entry.
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
}

/// The local values of one document or one window. `None` is an unset
/// global-local value; for the other options the value is always set.
#[derive(Clone, Debug)]
pub struct Local {
    vals: Vec<Option<Val>>,
}

impl Local {
    /// The local values of a new buffer or window: a copy of the global
    /// values for the options that always have a local value, unset for the
    /// global-local ones.
    pub fn new(global: &Options) -> Self {
        Local {
            vals: TABLE
                .iter()
                .enumerate()
                .map(|(i, d)| match d.scope {
                    Scope::Buffer | Scope::Window => Some(global.get(i).clone()),
                    _ => None,
                })
                .collect(),
        }
    }

    /// The local value of option `i`, `None` when it is unset.
    pub fn get(&self, i: usize) -> Option<&Val> {
        self.vals[i].as_ref()
    }

    fn set(&mut self, i: usize, v: Option<Val>) {
        self.vals[i] = v;
    }
}

/// The effective values for one document and one window: the local value
/// where there is one, else the global value.
#[derive(Clone, Copy)]
pub struct Eff<'a> {
    global: &'a Options,
    doc: &'a Local,
    win: &'a Local,
}

impl<'a> Eff<'a> {
    pub fn new(global: &'a Options, doc: &'a Local, win: &'a Local) -> Self {
        Eff { global, doc, win }
    }

    /// The effective value of option `i`.
    pub fn value(&self, i: usize) -> &'a Val {
        let local = match TABLE[i].scope {
            Scope::Global => None,
            Scope::Buffer | Scope::GlobalOrBuffer => self.doc.get(i),
            Scope::Window | Scope::GlobalOrWindow => self.win.get(i),
        };
        local.unwrap_or_else(|| self.global.get(i))
    }

    fn bool_of(&self, name: &str) -> bool {
        match Options::find(name).map(|i| self.value(i)) {
            Some(Val::Bool(v)) => *v,
            _ => false,
        }
    }

    fn num_of(&self, name: &str) -> i64 {
        match Options::find(name).map(|i| self.value(i)) {
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
    /// `'scrolloff'`: lines kept above and below the cursor.
    pub fn scrolloff(&self) -> usize {
        self.num_of("scrolloff").max(0) as usize
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
    /// The value of `'softtabstop'` in use (Vim's `get_sts_value()`): a
    /// negative setting means 'shiftwidth', zero means off.
    pub fn softtabstop(&self) -> usize {
        match self.num_of("softtabstop") {
            v if v < 0 => self.shiftwidth(),
            v => v as usize,
        }
    }
    pub fn smarttab(&self) -> bool {
        self.bool_of("smarttab")
    }
    pub fn shiftround(&self) -> bool {
        self.bool_of("shiftround")
    }
    pub fn startmenu(&self) -> bool {
        self.bool_of("startmenu")
    }
}

impl Editor {
    /// The effective options of the current document and window.
    pub(crate) fn eff(&self) -> Eff<'_> {
        let win: &Window = self.win();
        let doc: &Document = self.doc();
        Eff::new(&self.opts, &doc.local, &win.local)
    }

    /// Writes option `i` for a command with `target`: the global value, and
    /// the current document's or window's local value, as `targets` says.
    pub(crate) fn write_option(&mut self, i: usize, target: Target, v: Val) {
        let scope = TABLE[i].scope;
        let (to_global, to_local) = targets(scope, target);
        if to_local {
            let (doc, win) = self.parts_mut();
            local_of(scope, &mut doc.local, &mut win.local).set(i, Some(v.clone()));
        }
        if to_global {
            self.opts.set(i, v);
        }
    }

    /// The `<` form: the local value becomes the global one, or, for
    /// `:setlocal` on a global-local option, is unset. `:setglobal` changes
    /// nothing.
    pub(crate) fn reset_option(&mut self, i: usize, target: Target) {
        let scope = TABLE[i].scope;
        if !scope.has_local() || target == Target::Global {
            return;
        }
        let v = if target == Target::Local && scope.is_global_local() {
            None
        } else {
            Some(self.opts.get(i).clone())
        };
        let (doc, win) = self.parts_mut();
        local_of(scope, &mut doc.local, &mut win.local).set(i, v);
    }

    /// The value option `i` starts from for `+=`, `-=` and `^=`: the global
    /// value for `:setglobal`, otherwise the effective one.
    pub(crate) fn option_base(&self, i: usize, target: Target) -> Val {
        match target {
            Target::Global => self.opts.get(i).clone(),
            _ => self.eff().value(i).clone(),
        }
    }

    /// What `:set opt?`, `:setlocal opt?` and `:setglobal opt?` show: the
    /// effective value for `:set`, the local value for `:setlocal` (an unset
    /// global-local value shows as Vim does: `--opt` for a boolean, `opt=-1`
    /// for a number), and the global value for `:setglobal`.
    pub(crate) fn option_shown(&self, i: usize, target: Target) -> String {
        let def = &TABLE[i];
        match target {
            Target::Both => describe(i, self.eff().value(i)),
            Target::Global => describe(i, self.opts.get(i)),
            Target::Local if !def.scope.has_local() => describe(i, self.opts.get(i)),
            Target::Local => {
                let local = match def.scope {
                    Scope::Buffer | Scope::GlobalOrBuffer => self.doc().local.get(i),
                    _ => self.win().local.get(i),
                };
                match (local, &def.default) {
                    (Some(v), _) => describe(i, v),
                    (None, Val::Bool(_)) => format!("--{}", def.name),
                    (None, Val::Num(_)) => format!("  {}=-1", def.name),
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::document::DocId;

    fn idx(name: &str) -> usize {
        Options::find(name).expect(name)
    }

    /// The scope table as Vim 9.1's `:help local-options` gives it.
    #[test]
    fn scope_table_matches_vim() {
        use Scope::*;
        let want = [
            ("ignorecase", Global),
            ("smartcase", Global),
            ("wrapscan", Global),
            ("smarttab", Global),
            ("shiftround", Global),
            ("hlsearch", Global),
            ("incsearch", Global),
            ("startmenu", Global),
            ("autoindent", Buffer),
            ("expandtab", Buffer),
            ("shiftwidth", Buffer),
            ("tabstop", Buffer),
            ("softtabstop", Buffer),
            ("autoread", GlobalOrBuffer),
            ("scrolloff", GlobalOrWindow),
            ("number", Window),
            ("relativenumber", Window),
            ("wrap", Window),
        ];
        assert_eq!(want.len(), TABLE.len(), "every option has a row here");
        for (name, s) in want {
            assert_eq!(TABLE[idx(name)].scope, s, "scope of {name}");
        }
    }

    #[test]
    fn short_names_resolve() {
        assert_eq!(Options::find("sw"), Options::find("shiftwidth"));
        assert_eq!(Options::find("so"), Options::find("scrolloff"));
        assert_eq!(Options::find("nope"), None);
    }

    /// Two documents and two windows built directly, as the Vecs of step 2
    /// hold them: window `w` shows document `w`. The write rules are the
    /// ones `Editor::write_option` and `reset_option` use (`targets`,
    /// `local_of`).
    struct Two {
        global: Options,
        docs: Vec<Document>,
        wins: Vec<Window>,
    }

    impl Two {
        fn new() -> Self {
            let global = Options::default();
            Two {
                docs: vec![
                    Document::new(Buffer::new(), &global),
                    Document::new(Buffer::new(), &global),
                ],
                wins: vec![
                    Window::new(DocId(0), &global),
                    Window::new(DocId(1), &global),
                ],
                global,
            }
        }

        fn eff(&self, w: usize) -> Eff<'_> {
            Eff::new(&self.global, &self.docs[w].local, &self.wins[w].local)
        }

        fn write(&mut self, w: usize, name: &str, target: Target, v: Val) {
            let i = idx(name);
            let scope = TABLE[i].scope;
            let (to_global, to_local) = targets(scope, target);
            if to_local {
                let local = local_of(scope, &mut self.docs[w].local, &mut self.wins[w].local);
                local.set(i, Some(v.clone()));
            }
            if to_global {
                self.global.set(i, v);
            }
        }
    }

    #[test]
    fn buffer_local_set_writes_both_and_setlocal_only_local() {
        let mut t = Two::new();
        let sw = idx("sw");
        t.write(0, "sw", Target::Local, Val::Num(2));
        assert_eq!(t.eff(0).shiftwidth(), 2);
        // Document 1 keeps the copy it was made with.
        assert_eq!(t.eff(1).shiftwidth(), 8);
        t.write(0, "sw", Target::Global, Val::Num(4));
        assert_eq!(t.eff(0).shiftwidth(), 2, "the local value stays");
        assert_eq!(t.eff(1).shiftwidth(), 8, "document 1 is not affected");
        t.write(1, "sw", Target::Both, Val::Num(3));
        assert_eq!(t.eff(1).shiftwidth(), 3);
        assert_eq!(t.eff(0).shiftwidth(), 2);
        assert_eq!(t.global.get(sw), &Val::Num(3));
    }

    #[test]
    fn global_local_unset_uses_global_until_set() {
        let mut t = Two::new();
        let ar = idx("ar");
        assert!(t.docs[0].local.get(ar).is_none(), "unset at creation");
        t.write(0, "ar", Target::Global, Val::Bool(true));
        assert_eq!(t.eff(0).value(ar), &Val::Bool(true));
        assert_eq!(t.eff(1).value(ar), &Val::Bool(true));
        t.write(0, "ar", Target::Local, Val::Bool(false));
        assert_eq!(t.eff(0).value(ar), &Val::Bool(false));
        assert_eq!(
            t.eff(1).value(ar),
            &Val::Bool(true),
            "document 1 uses global"
        );
        assert_eq!(t.global.get(ar), &Val::Bool(true));
    }

    #[test]
    fn window_local_scrolloff_and_number() {
        let mut t = Two::new();
        let so = idx("so");
        let nu = idx("nu");
        t.write(0, "so", Target::Local, Val::Num(3));
        assert_eq!(t.eff(0).scrolloff(), 3);
        assert_eq!(t.eff(1).scrolloff(), 0);
        t.write(1, "so", Target::Global, Val::Num(7));
        assert_eq!(t.eff(1).value(so), &Val::Num(7));
        assert_eq!(t.eff(0).value(so), &Val::Num(3));
        // Window-local number: window 0 changes, window 1 does not.
        t.write(0, "nu", Target::Local, Val::Bool(true));
        assert_eq!(t.eff(0).value(nu), &Val::Bool(true));
        assert_eq!(t.eff(1).value(nu), &Val::Bool(false));
    }

    #[test]
    fn targets_follow_the_scope_and_target() {
        use Scope::*;
        assert_eq!(targets(Global, Target::Local), (true, false));
        assert_eq!(targets(Global, Target::Both), (true, false));
        assert_eq!(targets(Buffer, Target::Both), (true, true));
        assert_eq!(targets(Buffer, Target::Local), (false, true));
        assert_eq!(targets(GlobalOrWindow, Target::Global), (true, false));
        assert_eq!(targets(Window, Target::Both), (true, true));
    }

    #[test]
    fn reset_local_copies_or_unsets() {
        let mut t = Two::new();
        let sw = idx("sw");
        let so = idx("so");
        t.write(0, "sw", Target::Global, Val::Num(6));
        t.write(0, "sw", Target::Local, Val::Num(2));
        // `:setlocal opt<` copies the global value (what `reset_option` does).
        let g = t.global.get(sw).clone();
        t.docs[0].local.set(sw, Some(g));
        assert_eq!(t.docs[0].local.get(sw), Some(&Val::Num(6)));
        // A global-local option is unset by `:setlocal opt<`.
        t.write(0, "so", Target::Local, Val::Num(3));
        t.wins[0].local.set(so, None);
        assert!(t.wins[0].local.get(so).is_none());
        assert_eq!(t.eff(0).scrolloff(), 0);
    }

    #[test]
    fn new_documents_copy_globals_at_creation() {
        let mut t = Two::new();
        let sw = idx("sw");
        t.write(0, "sw", Target::Global, Val::Num(4));
        // Document 1 was made before the global changed: it keeps 8.
        assert_eq!(t.docs[1].local.get(sw), Some(&Val::Num(8)));
        // A document made now copies the new global value.
        let doc = Document::new(Buffer::new(), &t.global);
        assert_eq!(doc.local.get(sw), Some(&Val::Num(4)));
        assert!(doc.local.get(idx("ar")).is_none());
    }

    #[test]
    fn shown_values_follow_the_target() {
        let ar = idx("ar");
        let so = idx("so");
        let mut ed = Editor::from_text("x\n");
        assert_eq!(ed.option_shown(ar, Target::Both), "noautoread");
        assert_eq!(ed.option_shown(so, Target::Both), "  scrolloff=0");
        assert_eq!(ed.option_shown(ar, Target::Local), "--autoread");
        assert_eq!(ed.option_shown(so, Target::Local), "  scrolloff=-1");
        ed.write_option(so, Target::Local, Val::Num(5));
        ed.write_option(so, Target::Global, Val::Num(9));
        assert_eq!(ed.option_shown(so, Target::Local), "  scrolloff=5");
        assert_eq!(ed.option_shown(so, Target::Global), "  scrolloff=9");
        assert_eq!(ed.option_shown(so, Target::Both), "  scrolloff=5");
    }
}
