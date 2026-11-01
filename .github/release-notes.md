# rvim v0.1.0-alpha.3

rvim is a Vim-compatible modal text editor written in Rust. It is an alpha:
usable for editing files, but large parts of Vim are still missing. The goal is
that the same keys on the same text give the same buffer and cursor as Vim 9
started with `vim -N -u NONE`. The test suite checks this against a real Vim.

## What works

- Normal, Insert, Replace, Visual (`v`, `V`, `<C-v>`) and Command-line modes.
- Motions with counts, text objects, and the operators `d c y > < ~ g~ gu gU J gJ`.
- Registers (`"a`-`"z`, `"0`-`"9`, `"-`, `"_`), `p`, `P`, `.` repeat, and undo
  and redo with an undo tree.
- Search with `/`, `?`, `n`, `N`, `*`, `#`, `gn` and `gN`, with a Vim-compatible
  regex engine.
- Ex commands with ranges, including `:d :y :pu :m :t :j :> :< :k :mark :s :g
  :v :normal :undo :redo :set :r :w :q :wq :x :e`. `:s` has the `c` (confirm)
  flag and `\=` expressions (a subset of Vim's expression language: numbers,
  strings, lists, operators and about 25 common functions such as `submatch()`
  and `printf()`; no floats, dictionaries or user functions), and `&` and `g&`
  repeat a substitution.
- Macros: `q`, `@`, `@@`, `@:`.
- Lowercase marks `a`-`z`, and the jumplist with `<C-o>` and `<Tab>` and the
  context mark.
- Scrolling with `<C-e> <C-y> <C-f> <C-b> <C-d> <C-u>`, `zt zz zb z<CR> z. z-`,
  `H M L`, with counts and `scrolloff`.
- Indent options: `autoindent`, `expandtab`, `shiftwidth`, `tabstop`,
  `softtabstop`, `smarttab` and `shiftround`.
- Editing and history in the `:`, `/` and `?` command lines.
- Bracketed paste: pasted text is inserted as text and never run as commands.
- Recovery files: if rvim panics, unsaved text is written to a recovery file.
- An opt-in start menu: `rvim --start-menu`. It is off by default.

## What is missing

- No windows or splits, no tabs, and no buffer list or `:b`. There is one buffer.
- Marks: no uppercase (file) marks, no `'[`, `']`, `'<` or `'>` motions, and no
  `:marks`.
- `:s` has no `\n` (NUL) replacement. `:r !cmd` and `:!` shell commands do not
  work.
- No syntax highlighting, `hlsearch`, `incsearch` or line numbers.
- No config file (`vimrc`), no plugins, no Vimscript, no `:help`, no folds, no
  completion and no spell checking.
- Options are global: `:setglobal` does not exist, and `:setlocal` is only an
  alias for `:set`. `hlsearch`,
  `incsearch`, `number`, `relativenumber`, `wrap` and `autoread` are stored but
  have no effect yet.
- No agent integration and no inline code prediction yet. Both are planned and
  will stay off unless you configure them.

See [PLAN.md](https://github.com/axel0806/rvim/blob/main/PLAN.md) for the full
list of known differences from Vim.

## Install

Each archive holds one folder with the `rvim` binary (`rvim.exe` on Windows),
`LICENSE*` and `README*`. Installation steps are in
[INSTALL.md](https://github.com/axel0806/rvim/blob/main/INSTALL.md), including
building from source with `cargo install --git`.

## Licensing

rvim's own code is under the Apache License 2.0 ([LICENSE](https://github.com/axel0806/rvim/blob/main/LICENSE)).
Files derived from Vim are under the Vim license
([LICENSE-VIM](https://github.com/axel0806/rvim/blob/main/LICENSE-VIM)). Each
source file's first line says which license applies, and
[NOTICE](https://github.com/axel0806/rvim/blob/main/NOTICE) lists the
Vim-derived files. As the Vim license requires, `:version` and the startup
message say that this is a modified Vim.

## Known caveats

- This is an alpha. Expect bugs and missing Vim features. Report problems at
  https://github.com/axel0806/rvim/issues.
- The owner-only permissions for recovery files on Linux and macOS are not yet
  tested: the tests for them have not run on those systems. On Windows no
  permissions are set on recovery files.
