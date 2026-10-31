# rvim plan

Goal: a Vim-compatible modal editor in Rust. "Compatible" means: the same keys
on the same text give the same buffer and cursor as Vim 9 started with
`vim -N -u NONE` (Vim defaults, no vimrc). Every behaviour is checked against
a real Vim by the differential harness in `tests/common/mod.rs`.

## Architecture

| Module | Role |
|---|---|
| `buffer.rs` | Lines in a `ropey` rope (LF-only line breaks), file load/save, CRLF and missing-EOL detection |
| `text.rs` | Display widths and virtual columns (tabstop 8, `^X` control chars, wide chars) |
| `motion.rs` | Positions and motions; word motions are ports of Vim's `fwd_word`/`bck_word`/`end_word` with its `inc`/`dec` cursor walk and `utf_class` table |
| `editor.rs` | Mode state machine (Normal, Insert, Command-line), counts, desired column (`curswant`) |
| `ex.rs` | Ex commands and line addresses |
| `view.rs` | Scrolling, wrapping, `~`/`@` rows, message line and ruler as plain strings |
| `main.rs` | crossterm terminal front end (only file that touches the terminal) |

The library has no terminal dependency: tests drive `Editor::feed("keys")`.

## Milestones

1. **Core (done).** Open/write files; Normal, Insert, Command-line modes;
   `h j k l w W b B e E 0 ^ $ gg G + - <CR> <Space> <BS>` with counts and the
   arrow/Home/End keys; `i a I A o O` with counts; `x <Del>`; Insert-mode
   editing (`<BS>` over indent/eol/start, `<Del>`, `<CR>`, arrows);
   `:w :q :wq :x :e` (with `!` and file arguments), `:N`, `:$`, `:.+N`;
   `ZZ ZQ <C-g>`. Wrapping display, ruler, error messages.
2. **Vim grammar.** Operators `d c y > < ~ g~ gu gU` with motion or
   text object (`iw aw ip ap i" a" i( a(` ...), linewise/charwise/blockwise
   ranges, registers (unnamed, `"a-z`, `"0`, `"1-9`, `"-`, `"_`), `p P`,
   `.` repeat, undo/redo (`u <C-r> U`) with the change model needed for an
   undo tree, `r R J s S C D Y`, `f F t T ; ,`, `%`, Visual modes
   (`v V <C-v>`).
3. **Search and Ex.** `/ ? n N * #`, Vim regex (magic modes, `\v`), Ex
   ranges and `:s`, `:g`, `:d`, `:m`, `:t`, `:normal`, command-line
   editing and history, `:set` for the options used so far.
4. **Windows and buffers.** Splits, buffer list, marks, jumplist, macros
   (`q @`), `<C-f> <C-b> <C-d> <C-u> zt zz zb H M L`, 'scrolloff'.
5. **Extras (optional).** Syntax highlighting (tree-sitter), a config file,
   a Vimscript subset.

## Differential testing

`cargo test` runs `tests/compat_*.rs`. Each case is (text, keys). All cases of
a test run in one `vim -N -u NONE -es` process, fed with `feedkeys(keys,
'tx')` so they behave as typed. Lines and the cursor (`line('.')`,
`charcol('.')`) are compared with rvim. Set `RVIM_REFERENCE_VIM` to choose the
Vim binary and `RVIM_REQUIRE_VIM=1` to fail instead of skip when it is
missing. Add a case for every behaviour you implement, and when Vim and rvim
differ, Vim is right.

Things the harness cannot see: screen layout, messages, and file I/O. Those
have unit tests.

## Known differences from Vim

- Emoji use approximate block ranges instead of Vim's full `emoji_all` table
  for word motions; composing characters are separate characters.
- Options are fixed to Vim's `-N` defaults (`tabstop=8`,
  `backspace=indent,eol,start`, `whichwrap=b,s`, `startofline`, `wrap`, no
  `autoindent`). The ruler is always shown.
- An error does not flush pending typeahead (only matters for pasted key
  sequences).
- No undo, registers or `.` yet (milestone 2); `x` does not yank.
