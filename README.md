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

Prebuilt binaries for Windows, Linux and macOS are attached to the
[releases](https://github.com/axel0806/rvim/releases).

## What works

- Normal, Insert, Replace, Visual (`v V <C-v>`) and Command-line modes.
- Motions with counts: `h j k l w W b B e E ge 0 ^ $ gg G f F t T ; , % { } ( )`
  and more; text objects (`iw aw ip ap i" a" i( a(` ...).
- Operators `d c y > < ~ g~ gu gU J gJ` with motions and objects, registers
  (`"a`-`"z`, `"0`-`"9`, `"-`, `"_`), `p P`, `.` repeat, `u <C-r> U` with an undo
  tree, `r R s S C D Y x X`.
- Search: `/ ? n N * # g* g#` with offsets and `;`, on a Vim-compatible regex
  engine (`\v \m \M \V`, `\{}`, look-around, `\zs`, `\_x`, collections, ...).
- Ex commands: ranges (`%`, `.`, `$`, `'<,'>`, `/pat/`, `;`, offsets), `:d :y
  :pu :m :t :j :> :< :s :g :v :normal :undo :redo :set :w :q :wq :x :e`.
- Wrapping display, ruler, bracketed paste.

## Where rvim differs from Vim

rvim is not a drop-in replacement yet. The differences that matter in daily
use:

**Missing features**

- One buffer and one window: no `:split`, `:vsplit`, tabs, `:ls`, `:b`,
  marks (`m'`, `` ` ``), the jumplist, macros (`q`, `@`), or `:r`.
- No scrolling commands (`<C-f> <C-b> <C-d> <C-u> <C-e> <C-y> zt zz zb`), no
  `H M L`, no `scrolloff`.
- No command-line editing keys (`<Left>`, `<C-b>`, `<C-r>`) and no history
  (`<Up>` in `:` or `/`).
- No syntax highlighting, `hlsearch`, `incsearch`, or line numbers. The
  Visual area is drawn (reverse video).
- No `autoindent`, `expandtab`, `shiftwidth` and other options: only
  `ignorecase`, `smartcase` and `wrapscan` can be changed with `:set`; the
  others are fixed to Vim's `-N` defaults (`tabstop=8`,
  `backspace=indent,eol,start`, `whichwrap=b,s`, `startofline`, `wrap`).
- No config file (`vimrc`), plugins, Vimscript, `:help`, folds, completion,
  spell checking, `:terminal`, or `:!` shell commands.
- `:s`: no `c` flag (confirm) and no `\=` expressions; the `\n` (NUL)
  replacement is not implemented. `:p`, `:nu` and `:=` only show a message.
- Emoji use approximate ranges for word motions, and combining characters are
  separate characters.

**Behaviour that differs**

- Vim started with a vimrc is not the reference; rvim always behaves like
  `vim -N -u NONE`.
- `n` after `/pat/e+1` with the cursor on the last character of the buffer
  wraps in Vim and not in rvim.
- After `:%s/\n//` Vim leaves the cursor at the last join point; rvim puts it
  on the first non-blank.
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

rvim is a modified version of Vim: parts of it (for example the word
motions, the word-class table, the search and `:s` code) are translated from
Vim's C source. It is therefore distributed under the [Vim license](LICENSE),
which also applies to the rest of the code. As the license requires,
`:version` and the startup message say that this is a modified Vim, not the
official one.

Vim is by Bram Moolenaar and the Vim contributors (https://www.vim.org).

Contact for this modified version: open an issue at
https://github.com/axel0806/rvim/issues.
