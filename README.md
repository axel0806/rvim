# rvim

A Vim-compatible modal text editor written in Rust, checked keystroke by
keystroke against a real Vim. The goal is that the same keys on the same text
give the same buffer and cursor as Vim 9 started with `vim -N -u NONE`, plus a
few things Vim does not do (see [Where rvim differs from Vim](#where-rvim-differs-from-vim)).

**Status: alpha.** It is usable for editing files, but large parts of Vim are
missing. [PLAN.md](PLAN.md) has the architecture, the milestone list and the
full list of known differences.

```
cargo run --release -- path/to/file
```

`rvim --start-menu` (no file) shows a start menu with New file, Open file and
Quit, opt-in and off by default. Arrows and Enter choose, Esc closes; any other
key closes it and works as usual. See [PLAN.md](PLAN.md) ("Start menu") for
what is still to do.

## Star History

<a href="https://www.star-history.com/?repos=axel0806%2Frvim&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=axel0806/rvim&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=axel0806/rvim&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=axel0806/rvim&type=date&legend=top-left" />
 </picture>
</a>

Prebuilt binaries for Windows, Linux and macOS are attached to the
[releases](https://github.com/axel0806/rvim/releases).

## What works

- Normal, Insert, Replace, Visual (`v V <C-v>`) and Command-line modes.
- Motions with counts: `h j k l w W b B e E ge 0 ^ $ gg G f F t T ; , % { } ( )`
  and more; text objects (`iw aw ip ap i" a" i( a(` ...).
- Operators `d c y > < ~ g~ gu gU J gJ` with motions and objects, registers
  (`"a`-`"z`, `"0`-`"9`, `"-`, `"_`), `p P`, `.` repeat, `u <C-r> U` with an undo
  tree, `r R s S C D Y x X`.
- Search: `/ ? n N * # g* g# gn gN` with offsets and `;`, on a Vim-compatible regex
  engine (`\v \m \M \V`, `\{}`, look-around, `\zs`, `\_x`, collections, ...).
- Ex commands: ranges (`%`, `.`, `$`, `'<,'>`, `'a`, `/pat/`, `;`, offsets), `:d :y
  :pu :m :t :j :> :< :k :mark :s :g :v :normal :undo :redo :set :r :w :q :wq :x :e`.
- Lowercase marks `a`-`z` (`m{a-z}`, `'a`, `` `a ``).
- Jumplist and context mark: `<C-o>` and `<Tab>` with counts, `''`, `` `` ``,
  `m'`, `` m` ``, and `:jumps` (a one-line message). `G`, `gg`, `/`, `?`, `n`,
  `N`, `%`, `{`, `}`, `(`, `)`, marks, `:N`, `:s`, `:g` and undo set the mark.
- Wrapping display, ruler, bracketed paste.

## Where rvim differs from Vim

rvim is not a drop-in replacement yet. The differences that matter in daily
use:

**Missing features**

- One buffer and one window: no `:split`, `:vsplit`, tabs, `:ls` or `:b`.
  `:r file` works, but `:r !cmd` does not. Macros (`q`, `@`) work; an
  error ends a running macro as in Vim.
- Marks: lowercase marks work (`ma`, `'a`, `` `a ``, as operator motions, `:k`,
  `:mark`, `:'a` addresses, and they follow edits and undo). Missing: uppercase
  marks, `m[` `m]` `m<` `m>`, `'[` `']` `` `[ `] ``, `'<` and `'>` as motions,
  and `:marks`.
- Scrolling: `<C-f> <C-b> <C-d> <C-u> <C-e> <C-y> zt zz zb z<CR> z. z- H M L`
  work with counts and `scrolloff`. Not there: `z+ z^ z{height}<CR>`,
  `smoothscroll`, and the `<PageUp>`/`<PageDown>`/`<S-Up>` aliases.
- The `:`, `/` and `?` command lines have Vim's editing keys (`<Left> <Right>
  <Home> <End> <C-b> <C-e> <C-w> <C-u> <Del> <C-r>{reg}` and `<Up>`/`<Down>`/
  `<C-p>`/`<C-n>` history). There is no command-line window (`q:`, `q/`) or
  completion.
- No syntax highlighting, `hlsearch`, `incsearch`, or line numbers. The
  Visual area is drawn (reverse video).
- Few options. `:set` supports `ignorecase`, `smartcase`, `wrapscan`,
  `autoindent`, `expandtab`, `shiftwidth`, `tabstop`, `softtabstop`,
  `smarttab` and `shiftround`, and stores `hlsearch`, `incsearch`, `number`,
  `relativenumber`, `wrap` and `autoread` without using them yet
  (`scrolloff` works). Tab handling is Vim's for the common cases, but some
  combinations with `softtabstop` and `tabstop` 2 or 4, Replace-mode Tab with
  `softtabstop`, and block `<` or block put with tabs still differ from Vim
  (see PLAN.md, "Known differences"). `smartindent` does not exist, and
  `backspace=indent,eol,start`, `whichwrap=b,s` and `startofline` are
  fixed. Options are global: `:setglobal` does not exist, and `:setlocal` is only an
  alias for `:set`.
- No config file (`vimrc`), plugins, Vimscript, `:help`, folds, completion,
  spell checking, `:terminal`, or `:!` shell commands.
- `:s`: `\=` expressions are a subset (no floats, dictionaries, user functions
  or `v:` variables; see PLAN.md, Known differences); the `\n` (NUL)
  replacement is not implemented.
  The `c` flag asks like Vim, and after a declined match over a line break
  the next match can differ from Vim. `:p`, `:nu` and `:=` only show a message.
- Emoji use approximate ranges for word motions, and combining characters are
  separate characters.

**Behaviour that differs**

- Vim started with a vimrc is not the reference; rvim always behaves like
  `vim -N -u NONE`.
- `n` after `/pat/e+1` with the cursor on the last character of the buffer
  wraps in Vim and not in rvim.
- After `:%s/\n//` Vim leaves the cursor at the last join point; rvim puts it
  on the first non-blank. Vim also sets the context mark on the joined line in
  that case, which rvim does not.
- `:jumps` prints one line, not Vim's table. `[[` and `]]` are not implemented
  yet; when they are, they must set the context mark.
- An error does not flush pending typeahead.

**Intentional differences**

- **Bracketed paste is data.** A paste from the terminal is never run as
  commands: in Insert mode it is typed literally and undone as one step; in
  Normal mode it is inserted as if you had pressed `i`; in Visual mode it
  replaces the selection; on the command line newlines are dropped. (Vim
  does something similar only when the terminal and `t_BE` are configured.)
- **Alt+key** is sent as `<Esc>` followed by the key, like terminal Vim
  without mappings. Function keys insert their name in Insert mode (as Vim
  does) and do nothing elsewhere. `<Insert>` works as in Vim.
- Startup and `:version` say this is a modified Vim, as the licence requires.

**Planned additions beyond Vim** (see PLAN.md): a styled renderer with Visual
selection, hlsearch and syntax highlighting; automatic reload of files
changed by a coding agent (as an undoable step) with a diff view; detection
of and connection to coding agents (Claude Code, Codex, Cursor, Kimi Code)
over the Agent Client Protocol; and optional inline code prediction. All of
them will be off unless you configure them, so rvim stays `vim -N -u NONE`
by default.

## Documentation

- [INSTALL.md](INSTALL.md): installing from a release or from source, and the
  files rvim writes.
- [CONTRIBUTING.md](CONTRIBUTING.md): building, testing and sending changes.
- [SECURITY.md](SECURITY.md): how to report a vulnerability.
- [AGENTS.md](AGENTS.md): entry point for coding agents working on rvim.
- [MAINTAIN.md](MAINTAIN.md): releases, triage and review for maintainers.

## Tests

```
cargo test                      # unit tests + Vim differential tests
RVIM_REQUIRE_VIM=1 cargo test   # fail instead of skip if `vim` is not runnable
RVIM_REFERENCE_VIM=/path/to/vim cargo test
```

The differential tests need Vim 9 (`charcol()`) on PATH or in
`RVIM_REFERENCE_VIM`. They run each (text, keys) case in Vim and in rvim and
compare the lines, cursor, modified flag and registers.

## License

rvim has two licenses, and each source file says which one applies in its
first line (`// SPDX-License-Identifier: ...`):

- [Apache License 2.0](LICENSE) for rvim's own code. Files marked
  `Apache-2.0` are under it.
- [Vim license](LICENSE-VIM) for the parts derived from Vim. Files marked
  `Vim` are under it. They are translations of Vim's C source or algorithms,
  for example the word motions, text objects, operators, normal-mode
  commands, search, the jumplist, the `:s` loops and the register and undo
  code. [NOTICE](NOTICE) lists every such file and the reason.

A file that contains code derived from Vim is under the Vim license as a
whole. The conditions for modified versions are in LICENSE-VIM. As that
license requires, `:version` and the startup message say that this is a
modified Vim, not the official one.

Vim is by Bram Moolenaar and the Vim contributors (https://www.vim.org).

Contact for this modified version: open an issue at
https://github.com/axel0806/rvim/issues.
