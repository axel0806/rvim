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
//! Local values are kept raw, as Vim keeps them: a boolean is 0 or 1 and a
//! number is its value. A local value of a buffer-local or window-local
//! option is always set; it starts as a copy of the global value. A
//! global-local option's local value is unset while it is negative (Vim's
//! `-1`), and then the global value is used. Arithmetic in `:setlocal` starts
//! from the raw local value, so `:setlocal so+=2` on an unset value gives 1.
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
    // Not a Vim option: polls the open files for changes made outside rvim
    // (see watch.rs). Off by default: Vim does not poll, so with it off no
    // file is read in the background.
    b("filewatch", "filewatch", Scope::Global, false),
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

/// The raw local value of a global-local option that is not set (Vim's -1).
const UNSET: i64 = -1;

impl Scope {
    /// The option can have a local value (of a buffer or window).
    pub fn has_local(self) -> bool {
        self != Scope::Global
    }

    /// A global-local option: the local value may be unset, and then the
    /// global value is used.
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

/// Whether a write with `target` sets the global value.
fn writes_global(target: Target) -> bool {
    target != Target::Local
}

/// Whether a write with `target` sets the local value of an option that has
/// one. `:set` does not set an unset local value of a global-local option:
/// Vim leaves it unset (`:set so=5` after `:setglobal so=3` keeps -1).
fn writes_local(scope: Scope, target: Target, local_is_set: bool) -> bool {
    match target {
        Target::Global => false,
        Target::Local => true,
        Target::Both => !scope.is_global_local() || local_is_set,
    }
}

/// The raw value of a global value.
fn to_raw(v: &Val) -> i64 {
    match v {
        Val::Bool(b) => i64::from(*b),
        Val::Num(n) => *n,
    }
}

/// The value of an option with the type of `default`, from a raw value.
fn from_raw(default: &Val, raw: i64) -> Val {
    match default {
        Val::Bool(_) => Val::Bool(raw != 0),
        Val::Num(_) => Val::Num(raw),
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

/// The raw local values of one document or one window, one per table entry.
/// For a global-local option a negative value means unset.
#[derive(Clone, Debug)]
pub struct Local {
    raw: Vec<i64>,
}

impl Local {
    /// The local values of a new buffer or window: a copy of the global
    /// values for the options that always have a local value, unset for the
    /// global-local ones.
    pub fn new(global: &Options) -> Self {
        Local {
            raw: TABLE
                .iter()
                .enumerate()
                .map(|(i, d)| match d.scope {
                    Scope::Buffer | Scope::Window => to_raw(global.get(i)),
                    _ => UNSET,
                })
                .collect(),
        }
    }

    /// The raw local value of option `i` (`-1` when a global-local option is
    /// unset).
    pub fn raw(&self, i: usize) -> i64 {
        self.raw[i]
    }

    /// Whether the local value of option `i` is set.
    fn is_set(&self, i: usize) -> bool {
        self.raw[i] >= 0
    }

    fn set_raw(&mut self, i: usize, raw: i64) {
        self.raw[i] = raw;
    }
}

/// The local values of the document and window an option of `scope` keeps
/// its local value in.
fn local_of<'a>(scope: Scope, doc: &'a mut Local, win: &'a mut Local) -> &'a mut Local {
    match scope {
        Scope::Buffer | Scope::GlobalOrBuffer => doc,
        _ => win,
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
    pub fn value(&self, i: usize) -> Val {
        let d = &TABLE[i];
        let raw = match d.scope {
            Scope::Global => None,
            Scope::Buffer => Some(self.doc.raw(i)),
            Scope::Window => Some(self.win.raw(i)),
            Scope::GlobalOrBuffer => Some(self.doc.raw(i)).filter(|r| *r >= 0),
            Scope::GlobalOrWindow => Some(self.win.raw(i)).filter(|r| *r >= 0),
        };
        match raw {
            Some(r) => from_raw(&d.default, r),
            None => self.global.get(i).clone(),
        }
    }

    fn bool_of(&self, name: &str) -> bool {
        matches!(
            Options::find(name).map(|i| self.value(i)),
            Some(Val::Bool(true))
        )
    }

    fn num_of(&self, name: &str) -> i64 {
        match Options::find(name).map(|i| self.value(i)) {
            Some(Val::Num(v)) => v,
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
    pub fn filewatch(&self) -> bool {
        self.bool_of("filewatch")
    }
}

impl Editor {
    /// The effective options of the current document and window.
    pub(crate) fn eff(&self) -> Eff<'_> {
        let win: &Window = self.win();
        let doc: &Document = self.doc();
        Eff::new(&self.opts, &doc.local, &win.local)
    }

    /// The raw value option `i` starts from for `+=`, `-=`, `^=` and `!`:
    /// the global value for `:setglobal`, the raw local value for
    /// `:setlocal` (-1 when a global-local option is unset), and the
    /// effective value for `:set`.
    pub(crate) fn option_base(&self, i: usize, target: Target) -> i64 {
        let scope = TABLE[i].scope;
        match target {
            Target::Global => to_raw(self.opts.get(i)),
            Target::Local if scope.has_local() => match scope {
                Scope::Buffer | Scope::GlobalOrBuffer => self.doc().local.raw(i),
                _ => self.win().local.raw(i),
            },
            _ => to_raw(&self.eff().value(i)),
        }
    }

    /// Writes the raw value `raw` of option `i` as `target` says: the global
    /// value, and the current document's or window's local value.
    pub(crate) fn write_option(&mut self, i: usize, target: Target, raw: i64) {
        let scope = TABLE[i].scope;
        if !scope.has_local() {
            self.opts.set(i, from_raw(&TABLE[i].default, raw));
            return;
        }
        let (doc, win) = self.parts_mut();
        let local = local_of(scope, &mut doc.local, &mut win.local);
        if writes_local(scope, target, local.is_set(i)) {
            local.set_raw(i, raw);
        }
        if writes_global(target) {
            self.opts.set(i, from_raw(&TABLE[i].default, raw));
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
        let global = to_raw(self.opts.get(i));
        let raw = if target == Target::Local && scope.is_global_local() {
            UNSET
        } else {
            global
        };
        let (doc, win) = self.parts_mut();
        let local = local_of(scope, &mut doc.local, &mut win.local);
        if writes_local(scope, target, local.is_set(i)) {
            local.set_raw(i, raw);
        }
    }

    /// What `:set opt?`, `:setlocal opt?` and `:setglobal opt?` show: the
    /// effective value for `:set`, the raw local value for `:setlocal` (for
    /// a boolean that is negative, `--opt`, as Vim shows an unset one), and
    /// the global value for `:setglobal`.
    pub(crate) fn option_shown(&self, i: usize, target: Target) -> String {
        let def = &TABLE[i];
        match target {
            Target::Both => describe(i, &self.eff().value(i)),
            Target::Global => describe(i, self.opts.get(i)),
            Target::Local if !def.scope.has_local() => describe(i, self.opts.get(i)),
            Target::Local => {
                let raw = match def.scope {
                    Scope::Buffer | Scope::GlobalOrBuffer => self.doc().local.raw(i),
                    _ => self.win().local.raw(i),
                };
                match def.default {
                    Val::Num(_) => describe(i, &Val::Num(raw)),
                    Val::Bool(_) if raw < 0 => format!("--{}", def.name),
                    Val::Bool(_) => describe(i, &Val::Bool(raw != 0)),
                }
            }
        }
    }
}

impl Editor {
    /// Applies a parsed config (`config.rs`) through the `:set` path
    /// (`Target::Both`, as `:set opt=val` does): the `[options]` entries in
    /// file order, then `[start] menu`, then `start_menu_flag` (`--start-menu`
    /// or `--no-start-menu`) when the command line gave one. Returns the
    /// messages of the values the editor refused, as `:set` would show them.
    pub fn apply_config(
        &mut self,
        cfg: &crate::config::Config,
        start_menu_flag: Option<bool>,
    ) -> Vec<String> {
        use crate::config::OptValue;
        self.agent_opt_in(cfg.agent.enabled);
        let mut items: Vec<String> = cfg
            .options
            .iter()
            .map(|(name, v)| set_arg(name, v))
            .collect();
        if let Some(on) = cfg.start_menu {
            items.push(set_arg("startmenu", &OptValue::Bool(on)));
        }
        if let Some(on) = start_menu_flag {
            items.push(set_arg("startmenu", &OptValue::Bool(on)));
        }
        items
            .iter()
            .filter_map(|item| self.set_one(item, Target::Both).err())
            .collect()
    }

    /// Keeps the config's problems for `:ConfigWarnings`. Call it before the
    /// start menu or intro message is set, so that [`Self::show_config_notice`]
    /// can add to them.
    pub fn set_config_warnings(&mut self, warnings: Vec<String>) {
        self.config_warnings = warnings;
    }

    /// Adds `[N config warnings]` to the startup message when the config had
    /// problems. The message is the intro (the short modified-version notice,
    /// which the Vim license asks for) or the file's info message; the note is
    /// appended, so the notice text is never replaced. With the start menu
    /// shown there is no message, and the note is the message. Does nothing
    /// when there are no problems. `:ConfigWarnings` lists the problems.
    pub fn show_config_notice(&mut self) {
        let n = self.config_warnings.len();
        if n == 0 {
            return;
        }
        let note = format!("[{n} config warning{}]", if n == 1 { "" } else { "s" });
        let text = match self.message() {
            Some(m) => format!("{} {note}", m.text),
            None => note,
        };
        self.info(text);
    }

    /// `:ConfigWarnings`: every problem found in the config file at startup,
    /// joined with ` | ` (as `:jumps` joins its items).
    pub fn show_config_warnings(&mut self) {
        if self.config_warnings.is_empty() {
            self.info("No config warnings");
        } else {
            let text = self.config_warnings.join(" | ");
            self.info(text);
        }
    }
}

/// A `:set` argument that sets `name` to `v`.
fn set_arg(name: &str, v: &crate::config::OptValue) -> String {
    use crate::config::OptValue;
    match v {
        OptValue::Bool(true) => name.to_string(),
        OptValue::Bool(false) => format!("no{name}"),
        OptValue::Num(n) => format!("{name}={n}"),
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
            ("filewatch", Global),
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
    fn filewatch_is_global_and_off_by_default() {
        // Off: Vim does not poll files, so rvim does not either.
        let mut ed = crate::editor::Editor::from_text("x\n");
        assert!(!ed.eff().filewatch());
        ed.feed(":set filewatch<CR>");
        assert!(ed.eff().filewatch());
        assert_eq!(ed.message().map(|m| m.error), None, "no E518: rvim-only");
        ed.feed(":set nofilewatch<CR>");
        assert!(!ed.eff().filewatch());
    }

    #[test]
    fn short_names_resolve() {
        assert_eq!(Options::find("sw"), Options::find("shiftwidth"));
        assert_eq!(Options::find("so"), Options::find("scrolloff"));
        assert_eq!(Options::find("nope"), None);
    }

    /// Two documents and two windows built directly, as the Vecs of step 2
    /// hold them: window `w` shows document `w`. The writes follow the same
    /// rules as `Editor::write_option` (`writes_local`, `writes_global`).
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

        fn raw(&self, w: usize, name: &str) -> i64 {
            let i = idx(name);
            match TABLE[i].scope {
                Scope::Buffer | Scope::GlobalOrBuffer => self.docs[w].local.raw(i),
                _ => self.wins[w].local.raw(i),
            }
        }

        fn write(&mut self, w: usize, name: &str, target: Target, raw: i64) {
            let i = idx(name);
            let scope = TABLE[i].scope;
            if !scope.has_local() {
                self.global.set(i, from_raw(&TABLE[i].default, raw));
                return;
            }
            let local = local_of(scope, &mut self.docs[w].local, &mut self.wins[w].local);
            if writes_local(scope, target, local.is_set(i)) {
                local.set_raw(i, raw);
            }
            if writes_global(target) {
                self.global.set(i, from_raw(&TABLE[i].default, raw));
            }
        }
    }

    #[test]
    fn buffer_local_set_writes_both_and_setlocal_only_local() {
        let mut t = Two::new();
        let sw = idx("sw");
        t.write(0, "sw", Target::Local, 2);
        assert_eq!(t.eff(0).shiftwidth(), 2);
        // Document 1 keeps the copy it was made with.
        assert_eq!(t.eff(1).shiftwidth(), 8);
        t.write(0, "sw", Target::Global, 4);
        assert_eq!(t.eff(0).shiftwidth(), 2, "the local value stays");
        assert_eq!(t.eff(1).shiftwidth(), 8, "document 1 is not affected");
        t.write(1, "sw", Target::Both, 3);
        assert_eq!(t.eff(1).shiftwidth(), 3);
        assert_eq!(t.eff(0).shiftwidth(), 2);
        assert_eq!(t.global.get(sw), &Val::Num(3));
    }

    #[test]
    fn global_local_unset_uses_global_until_set() {
        let mut t = Two::new();
        let ar = idx("ar");
        assert_eq!(t.raw(0, "ar"), UNSET, "unset at creation");
        t.write(0, "ar", Target::Global, 1);
        assert_eq!(t.eff(0).value(ar), Val::Bool(true));
        assert_eq!(t.eff(1).value(ar), Val::Bool(true));
        t.write(0, "ar", Target::Local, 0);
        assert_eq!(t.eff(0).value(ar), Val::Bool(false));
        assert_eq!(
            t.eff(1).value(ar),
            Val::Bool(true),
            "document 1 uses global"
        );
        assert_eq!(t.global.get(ar), &Val::Bool(true));
    }

    /// Vim's rules for `:set` on an unset global-local option (probed with
    /// `vim -N -u NONE -es`): only the global value changes.
    #[test]
    fn set_leaves_an_unset_global_local_local_alone() {
        let mut t = Two::new();
        t.write(0, "so", Target::Global, 3);
        t.write(0, "so", Target::Both, 5);
        assert_eq!(t.raw(0, "so"), UNSET, "set so=5 keeps the local unset");
        assert_eq!(t.eff(0).scrolloff(), 5);
        // Once set, `:set` changes the local value too.
        t.write(0, "so", Target::Local, 2);
        t.write(0, "so", Target::Both, 4);
        assert_eq!(t.raw(0, "so"), 4);
    }

    /// `:setlocal` arithmetic starts from the raw local value: -1 when
    /// unset (probed: `:setglobal so=3 | :setlocal so+=2` gives local 1,
    /// `so-=1` gives -2 with effective 3, and `ar!` gives -2).
    #[test]
    fn arithmetic_on_an_unset_local_starts_from_minus_one() {
        let mut ed = Editor::from_text("x\n");
        let so = idx("so");
        ed.opts.set(so, Val::Num(3));
        assert_eq!(ed.option_base(so, Target::Local), UNSET);
        ed.write_option(so, Target::Local, UNSET + 2);
        assert_eq!(ed.eff().scrolloff(), 1);
        ed.write_option(so, Target::Local, UNSET);
        ed.write_option(so, Target::Local, UNSET - 1);
        assert_eq!(ed.option_shown(so, Target::Local), "  scrolloff=-2");
        assert_eq!(ed.eff().scrolloff(), 3, "a negative local uses the global");
        let ar = idx("ar");
        ed.opts.set(ar, Val::Bool(true));
        ed.write_option(ar, Target::Local, UNSET ^ 1);
        assert_eq!(ed.option_shown(ar, Target::Local), "--autoread");
        assert_eq!(ed.eff().value(ar), Val::Bool(true));
        ed.write_option(ar, Target::Local, UNSET ^ 1 ^ 1);
        assert_eq!(ed.option_shown(ar, Target::Local), "--autoread");
    }

    #[test]
    fn window_local_scrolloff_and_number() {
        let mut t = Two::new();
        let so = idx("so");
        let nu = idx("nu");
        t.write(0, "so", Target::Local, 3);
        assert_eq!(t.eff(0).scrolloff(), 3);
        assert_eq!(t.eff(1).scrolloff(), 0);
        t.write(1, "so", Target::Global, 7);
        assert_eq!(t.eff(1).value(so), Val::Num(7));
        assert_eq!(t.eff(0).value(so), Val::Num(3));
        // Window-local number: window 0 changes, window 1 does not.
        t.write(0, "nu", Target::Local, 1);
        assert_eq!(t.eff(0).value(nu), Val::Bool(true));
        assert_eq!(t.eff(1).value(nu), Val::Bool(false));
    }

    #[test]
    fn write_rules_follow_the_scope_and_target() {
        use Scope::*;
        assert!(writes_global(Target::Both));
        assert!(writes_global(Target::Global));
        assert!(!writes_global(Target::Local));
        assert!(writes_local(Buffer, Target::Both, false));
        assert!(writes_local(Window, Target::Both, false));
        assert!(!writes_local(GlobalOrBuffer, Target::Both, false));
        assert!(writes_local(GlobalOrBuffer, Target::Both, true));
        assert!(writes_local(GlobalOrWindow, Target::Local, false));
        assert!(!writes_local(GlobalOrWindow, Target::Global, true));
    }

    #[test]
    fn reset_local_copies_or_unsets() {
        let mut ed = Editor::from_text("x\n");
        let sw = idx("sw");
        let so = idx("so");
        ed.write_option(sw, Target::Global, 6);
        ed.write_option(sw, Target::Local, 2);
        ed.reset_option(sw, Target::Local);
        assert_eq!(ed.doc().local.raw(sw), 6);
        ed.write_option(sw, Target::Local, 2);
        ed.reset_option(sw, Target::Both);
        assert_eq!(ed.doc().local.raw(sw), 6);
        // `:setlocal` unsets a global-local option.
        ed.write_option(so, Target::Local, 3);
        ed.reset_option(so, Target::Local);
        assert_eq!(ed.win().local.raw(so), UNSET);
        // `:set so<` on an unset global-local option does not set it.
        ed.reset_option(so, Target::Both);
        assert_eq!(ed.win().local.raw(so), UNSET);
        // `:setglobal opt<` changes nothing.
        ed.write_option(sw, Target::Local, 2);
        ed.reset_option(sw, Target::Global);
        assert_eq!(ed.doc().local.raw(sw), 2);
    }

    #[test]
    fn new_documents_copy_globals_at_creation() {
        let mut t = Two::new();
        let sw = idx("sw");
        t.write(0, "sw", Target::Global, 4);
        // Document 1 was made before the global changed: it keeps 8.
        assert_eq!(t.docs[1].local.raw(sw), 8);
        // A document made now copies the new global value.
        let doc = Document::new(Buffer::new(), &t.global);
        assert_eq!(doc.local.raw(sw), 4);
        assert_eq!(doc.local.raw(idx("ar")), UNSET);
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
        ed.write_option(so, Target::Local, 5);
        ed.write_option(so, Target::Global, 9);
        assert_eq!(ed.option_shown(so, Target::Local), "  scrolloff=5");
        assert_eq!(ed.option_shown(so, Target::Global), "  scrolloff=9");
        assert_eq!(ed.option_shown(so, Target::Both), "  scrolloff=5");
    }
}
