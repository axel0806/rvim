# rvim plan

Goal: a Vim-compatible modal editor in Rust. "Compatible" means: the same keys
on the same text give the same buffer and cursor as Vim 9 started with
`vim -N -u NONE` (Vim defaults, no vimrc). Every behaviour is checked against
a real Vim by the differential harness in `tests/common/mod.rs`.

## Status: read this first

Last updated 2026-11-01 (Makefile and CI workflow added; CI not yet run on GitHub). **rvim is an alpha, roughly 25-30% of this plan.**
As of 2026-11-01 milestones 1-3 are done; 4 and 5 are partly done; 6 has the start menu only; 7-9 are plans.
Released as `v0.1.0-alpha.1`, `v0.1.0-alpha.2` and `v0.1.0-alpha.3`. The next release is `v0.1.0-alpha.4`, not yet tagged. All tests pass against Vim 9.1 and clippy/fmt are
clean at the last commit.

| Milestone | State |
|---|---|
| 1 Core, 2 Vim grammar | Done |
| 3 Search and Ex | 100%. `:s` `\=` expressions done as the subset in `src/expr.rs` (see Known differences) |
| 4 Foundation | ~70%. Left: the Document/Window/Editor split (the biggest item; see "Foundation"), kitty keyboard protocol, cell-level diff redraw, real local/global option scopes (`:setlocal`/`:setglobal` semantics; `:setlocal` is accepted as an alias of `:set`, `:setglobal` does not exist), recovery on terminal close (SIGHUP) |
| 5 Windows and buffers | ~30%. Done: macros (`q`, `@`, `@@`, `@:`), lowercase marks (`m{a-z}`, `'a`, `` `a ``, `:k`, `:mark`, `'a` addresses), the jumplist (`<C-o>`, `<Tab>` with counts, `:jumps`) and the context mark (`''`, `` `` ``, `m'`, `` m` ``), scrolling (`<C-e> <C-y> <C-f> <C-b> <C-d> <C-u> zt zz zb z<CR> z. z- H M L`, counts, `scrolloff`, wrapped lines). Left: splits, buffer list, the other marks (uppercase, `'[`/`']`, `'<`/`'>` motions), swap files |
| 6 Modern editing experience | Started, not near done. Landed: the start menu (opt-in `--start-menu`, `startmenu` option). Left: hlsearch, incsearch, syntax, line numbers, config file, palette, mouse, `:Start`, the start menu's recent files, Config and agent entries, and its modern-profile default (see "Start menu") |
| 7 Agent integration | 0%. Not started (file-change reload as an undo step, diff view, ACP connector, agent detection). Research is in "Detecting and connecting to agents" |
| 8 Prediction | 0%. Not started |
| 9 Extras | 0%. Not started |

Since the alpha (all committed and pushed): `:s/pat/\=expr/` with the
expression subset in `src/expr.rs` (numbers, strings, lists, operators, `?:`,
indexing and slicing, the common built-in functions, Vim's error messages;
differential cases in `tests/compat_m3_expr.rs`), bracketed paste, Alt/F-keys/Insert/
Shift-Tab, Visual area drawn through `view::Hl` groups, row-level diff redraw,
the options registry (`options.rs`: `autoindent`, `expandtab`, `shiftwidth`,
`tabstop`, `softtabstop`, `smarttab`, `shiftround` work; `text.rs` takes the
tabstop as a parameter, Insert Tab and BS follow `ins_tab()`/`ins_bs()` for
softtabstop and smarttab), panic recovery files (`recovery.rs`), the channel-based event loop
(`event.rs`), `:` command-line editing and history (`cmdline.rs`), `&`/`g&`,
macros (`macros.rs`), `/` and `?` prompts with the same editing keys, `<C-r>`
and history (also in `cmdline.rs`), `gn`/`gN` (`current_search()` in
`search.rs`), `:r`, the `c` flag of `:s` (Confirm mode, `ex.rs`). A bare `|` in the pattern or
replacement of `:s` (or in a `/pat/` range address) is text, as in Vim; only
the part after the replacement's delimiter and the flags separates commands
(`ex.rs` `split_bar`, differential cases in `tests/compat_m3_bar.rs`). Search-prompt
editing and history landed in commit e556a0b. Lowercase marks `a`-`z` (kept
in `buffer.rs`, followed through edits by the buffer's line methods, saved and
restored by undo in `undo.rs`), and the scrolling commands with `scrolloff`
(`scroll.rs`; the window's top line now lives in `Editor`). Command-line
flags: `-h`/`--help` prints the usage and `-v`/`--version` prints
`VERSION_MESSAGE`, both to stdout with exit 0 and no terminal set-up
(`cli.rs`, `tests/cli_flags.rs`); `--` ends the options, as in Vim. Before
this, `--help` opened a buffer named `--help`.

**In progress** (as of 2026-11-01; maintained by the orchestrator, and an
entry is removed when its branch lands on main):

- Fix the first CI run: Linux apt Vim is older than 9.1.1914 (3 jump tests differ), Windows Vim path (branch `ci-fix-vim`)

**Suggested order for what is left:** (1) Document/Window split, (2) splits +
buffers + scrolling, (3) milestone 7, in the step order of "Agent experience:
how it fits together" (section 11). That order refines this line: the file
reload and the ACP connector need no splits, so they can land before (2)
finishes; only the side-by-side diff, the agent log split and the terminal
pane wait for splits. (4) hlsearch/incsearch/syntax, (5) prediction. Cut a new release (`git tag v0.1.0-alpha.N`) when a
milestone lands; `.github/workflows/release.yml` builds the binaries. Small items that can go at
any time: the stale `--notes` string in `release.yml`, to refresh before tagging
v0.1.0-alpha.3 (see "Repository and packaging").

**Planned additions from the owner's todo (2026-11-01).** These are plans
only. Apart from the License item and the repository documents, which are done,
nothing listed here is built, and the milestone table above is unchanged.

- Start menu at startup, off by default: see "Start menu" (milestone 6).
- License: Apache-2.0 for new code and the Vim license for Vim-derived code.
  Implemented 2026-11-01; see "Licensing".
- Neovim-inspired features, reimplemented from Neovim's documentation: see
  "Neovim-inspired features" (milestone 9 and later).
- Lua extension layer: see "Lua extensions" (milestone 9).
- Neovim plugin compatibility, in stages: see "Neovim plugin compatibility"
  (milestone 9).
- Makefile: done 2026-11-01 (`build`, `debug`, `test`, `test-vim`, `lint`, `fmt`, `check`, `run`, `install`, `clean`, `help`); see "Repository and packaging". `uninstall` and `release` are not in it.
- Repository documents (`SECURITY.md`, `CONTRIBUTING.md`, `AGENTS.md`,
  `MAINTAIN.md`, `INSTALL.md`): written 2026-11-01, done; see "Repository and
  packaging".
- Future goals (AUR packages, logo): see "Future goals".

Still to do from this list: the AUR packages, the logo, and the
plugin, Lua and Neovim items.

Agent experience (how the agent parts fit together, with the implementation
steps and test plan for them): see "Agent experience: how it fits together"
after milestone 7.

### Working notes (things that cost time)

- Scrolling and the harness (milestone 5). Vim run with `-es` has a 24x80
  screen (23 text rows) and `winheight()` is 23; `set lines=40` does not
  change it. The harness asserts `S 24 80` in its first output line and
  fails if Vim's screen differs. Vim never redraws here, so after a cursor
  move the window it reports is stale: `line('w0')` after `G` is the cursor
  line, not the screen's top, and `CTRL-F` or `H`/`M`/`L` after an earlier
  scroll see a wrong bottom line. The harness therefore compares only
  cursors (and lines) for scroll cases, and only cases whose cursor does
  not depend on that stale window (a fresh window at line 1, or a scroll
  command that sets the line itself). Top lines, wrapped-line heights and
  the 'scrolloff' margin are checked in `tests/scroll_window.rs` with the
  rules written from `scroll.txt`. Vim's C source was not available to
  port line by line (the sources reachable from here are summaries only),
  so `scroll.rs` follows the documented rules and the probed results.
  Checked against Vim: CTRL-F/CTRL-D/CTRL-U/CTRL-E/CTRL-Y cursors on a fresh
  window, the `halfpage()` line-by-line rule for CTRL-D on wrapped lines
  (3 whole lines for 11 rows of 3-row lines), `H`/`M`/`L` counts (clamped to
  the window, count and `'scrolloff'` combined with max). Pitfall: `set
  scroll&` sets 'scroll' to 0, and CTRL-D then scrolls nothing, so the
  harness resets it to half the window height instead.
- Windows Git Bash: put `~/.cargo/bin` on `PATH`. Never run `python`/`python3`
  (the Store stub hangs); use `perl -0pi` for multi-line edits. Shell
  heredocs and `sed` mangle backslashes (`\t`, `\n`, `\x16` turn into real
  control characters, and `"\<..>"` fails to compile): write Rust with the
  Write/Edit tools, and check `cargo build` right after any scripted edit.
- Differential harness artifacts (Vim side, not bugs in rvim): Vim runs in Ex
  mode (`-es`), so `<BS>` on an empty `:` line does not cancel and an empty
  `:<CR>` moves down a line; cases must not run `:x`/`:q` (they kill Vim);
  `silent! feedkeys` means errors never flush typeahead, so macro/typeahead
  abort-on-error cannot be compared (use unit tests, see `tests/macros.rs`);
  command history is cleared per case via `histdel`, options are reset per
  case in `SCRIPT_BODY`; Vim died on `qaf{char}x q @a` style cases (recording
  an `f` motion), so they are left out. Register lines are compared with
  trailing `\r` kept. Desired columns carry over between cases in Vim, and
  neither `cursor()` nor `normal! 0` clears them here, so `SCRIPT_BODY` uses
  `winrestview({'curswant': 0})` before each case (a `j` in a case would
  otherwise start at the previous case's column).
- `<Esc>` and `<C-c>` in a `:` or `/` prompt are not compared: in silent Ex
  mode Vim types the following keys into the prompt instead of cancelling
  (`/qux<Esc>x` gives the pattern `quxx`). Cancel behaviour is tested in
  `tests/search_prompt.rs`. `<Esc>` in Normal and Visual mode is compared.
- `:s///c` prompts in silent Ex mode (`-es`) read one line per answer: the
  first character counts and the rest of the line is dropped. So answers are
  sent with `<CR>` after each one (a terminal Vim ignores `<CR>` at the
  prompt too, and so does rvim). A `<CR>` after the answer that finishes the
  `:s` is a Normal key in rvim but consumed by Vim, so such cases end there.
  Vim treats end of input at a prompt as `q`, and `:g` then asks on the next
  line; the harness answers `<Esc>` to every prompt still waiting. Keys typed
  after a finished `:s` (`u`, `x`, `.`) are not compared: they are in
  `tests/confirm_prompt.rs`. Vim's cursor after `:s///c`: a substitution puts
  it at column 0 of its line, a declined match on its match, a quit keeps the
  prompt's position, and `a` (or no `c`) ends at the first non-blank.
  Errors do not stop the other `|` commands of a line (E486 included).
- Every behaviour needs cases in `tests/compat_*.rs`; when Vim and rvim
  differ, rvim is wrong. Verify with `RVIM_REQUIRE_VIM=1 cargo test`, then
  `cargo clippy --all-targets && cargo fmt --check`.
- Known gaps are listed under "Known differences" below and mirrored in
  README.md ("Where rvim differs from Vim"): keep both current.

## Direction

rvim stays Vim-compatible for everything Vim does, and adds three things on
top:

1. **A pleasant modern editing experience**: styled rendering, good defaults,
   discoverability.
2. **First-class use next to a coding agent** of the user's choice (Claude
   Code, Cursor's agent, anything that edits files on disk or speaks a
   protocol).
3. **Automatic prediction** of upcoming code: inline suggestions the user can
   accept.

The extensions are additive. With no config file, rvim behaves like
`vim -N -u NONE` and the differential tests apply unchanged (see "Extensions
and the Vim contract").

Two further goals from the owner's todo sit beside the three above. First,
rvim copies features of Neovim indirectly: each one is reimplemented from
Neovim's public documentation, never from its source (see "Neovim-inspired
features"). Second, rvim's start screen and its plugin story follow Neovim's
pattern, but the Vim contract still decides what happens without an opt-in.

## Architecture

| Module | Role |
|---|---|
| `buffer.rs` | Lines in a `ropey` rope (LF-only line breaks), file load/save, CRLF and missing-EOL detection |
| `text.rs` | Display widths and virtual columns (every function takes the tabstop; `^X` control chars, wide chars) |
| `motion.rs` | Positions and motions; word motions are ports of Vim's `fwd_word`/`bck_word`/`end_word` with its `inc`/`dec` cursor walk and `utf_class` table |
| `editor.rs` | Mode state machine (Normal, Insert, Command-line), counts, desired column (`curswant`) |
| `jumplist.rs` | The jumplist and the context mark (ports of Vim's `setpcmark`, `checkpcmark`, `movemark`). Window state, so it moves to `Window` with the split; the buffer logs its line edits (`PosEdit`) and the editor applies them in order |
| `ex.rs` | Ex commands: ranges (`%`, `.`, `$`, marks `'<` `'>` and `'a`, `/pat/`, `;`), file commands, `:k`/`:mark`, `:d :y :pu :m :t :j :> :<`, `:s` (port of `do_sub`; the `c` prompt keeps the loop state in `SubRun` between keys), `:g`/`:v` and command lines as queued `ExTask`s, `:normal`, `:set` |
| `normal.rs`, `ops.rs`, `textobject.rs`, `register.rs`, `undo.rs` | Normal/Visual parsing and motions, operators, text objects, registers, undo tree |
| `regex.rs` | Vim regex (`\v \m \M \V`, `\{}`, look-around, `\zs`, `\_x`, collections, `\%` items) as a backtracking matcher over buffer lines |
| `search.rs` | Port of `searchit()` / `do_search()` (offsets, `;`, `cpo-c`, wrapscan), `*`/`#` word lookup |
| `scroll.rs` | The window's top line and size (in `Editor`), `update_topline` with 'scrolloff' and wrapped lines, and the scroll commands (`CTRL-E/Y/F/B/D/U`, `z` commands, `H M L`) |
| `view.rs` | Layout as a pure function of the editor (`frame`): wrapping, `~`/`@` rows, message line and ruler as plain strings |
| `main.rs` | crossterm terminal front end (only file that touches the terminal) |

The library has no terminal dependency: tests drive `Editor::feed("keys")`.

Today `Editor` owns one `Buffer`, one cursor and one undo tree, and also the
window's top line and screen size (`top`, `screen_cols`, `screen_rows`),
which `main.rs` sets on each draw and on resize. `view::frame(&Editor)` only
lays out. Milestone 4 changes this; the target structure is described under
"Foundation".

## Gaps in the current code

What the revised milestones fix, as the code stands today:

- `view::Frame.rows` are plain `String`s with no styling. Visual selections
  are not drawn, and hlsearch, incsearch, paren matching and syntax
  highlighting are impossible. `main.rs` colours only the `~` filler and the
  status line.
- `main.rs` handles `Event::Paste` but never enables bracketed paste, so
  pastes arrive as keystrokes and can run as Normal mode commands. The paste
  handler also replays characters one by one (not one undo step).
- `convert()` drops Alt, F-keys, Shift-Tab, Insert and other modifiers, and
  `Key` cannot represent them.
- The panic hook restores the terminal but saves nothing.
- `Editor` holds a single buffer, cursor and undo tree, and `View` lives in
  `main.rs`.
- `event::read()` blocks, so nothing can happen except on a key press: no
  timers (`timeoutlen`), no file watching, no background results.
- `Options` is three fixed booleans (`ignorecase`, `smartcase`, `wrapscan`).
- `draw()` repaints every row on every key.

## Milestones

1. **Core (done).** Open/write files; Normal, Insert, Command-line modes;
   `h j k l w W b B e E 0 ^ $ gg G + - <CR> <Space> <BS>` with counts and the
   arrow/Home/End keys; `i a I A o O` with counts; `x <Del>`; Insert-mode
   editing (`<BS>` over indent/eol/start, `<Del>`, `<CR>`, arrows);
   `:w :q :wq :x :e` (with `!` and file arguments), `:N`, `:$`, `:.+N`;
   `ZZ ZQ <C-g>`. Wrapping display, ruler, error messages.
2. **Vim grammar (done).** Operators `d c y > < ~ g~ gu gU` with motion or
   text object (`iw aw ip ap i" a" i( a(` ...), linewise/charwise/blockwise
   ranges, registers (unnamed, `"a-z`, `"0`, `"1-9`, `"-`, `"_`), `p P`,
   `.` repeat, undo/redo (`u <C-r> U`) with the change model needed for an
   undo tree, `r R J s S C D Y`, `f F t T ; ,`, `%`, Visual modes
   (`v V <C-v>`).
3. **Search and Ex (mostly done).** Done: `/ ? n N * # g* g#` with
   offsets, Vim regex, Ex ranges, `:s :g :v :d :y :pu :m :t :j :> :<
   :normal :undo :redo :set` (ic/scs/ws), Visual `:` (`'<,'>`).
   Done since: `:` command-line editing keys, `<C-r>` insertion and history,
   `&`/`g&`, and the same for the `/` and `?` prompts (search history also
   takes the patterns of `:s`, `:g`, `:v`, `*`, `#` and Ex addresses, as in
   Vim). Done since: `gn`/`gN` in Normal, Visual and operator-pending mode
   (with `.` repeat as typed, like Vim's `prep_redo` for `gn`), and
   `:[range]r[ead] {file}` (`:r !cmd` is not done). Done since: the `c` flag
   of `:s`, `:&` and `:g` (Confirm mode: `y n a q l <Esc>`, one undo step,
   the match highlighted with `view::Hl::IncSearch`; the design is in the
   "Working notes"), and `<C-e>`/`<C-y>` in the prompt, which scroll the
   window one line and keep the match where it is, stopping short of the
   match (`scroll::scroll_window_for_prompt`; tests in `tests/confirm_prompt.rs`,
   not probed in Vim: `-es` does not show the window).
   Marks in Ex addresses (`'a`) are done with the marks (milestone 5).
4. **Foundation (in progress).** Done: bracketed paste, Alt/F-keys/Insert/Shift-Tab, Visual area drawn via highlight groups, row-level diff redraw, options registry (`autoindent`, `expandtab`, `shiftwidth`), panic recovery files, channel-based event loop with an input thread. Left: the Document/Window/Editor split, kitty keyboard protocol, cell-level diff, real option scopes (`:setlocal`/`:setglobal`; `:setlocal` is only an alias of `:set` for now), SIGHUP recovery. Original description: Restructure before adding more features, because windows,
   agents and prediction all depend on it. Order, each step landing with the
   differential tests green: (a) Document/Window/Editor split, (b) internal
   event loop, (c) styled frame with the Visual area drawn, (d) diff redraw,
   (e) input (bracketed paste, modifiers, kitty protocol), (f) options
   registry, (g) emergency recovery files. Details in "Foundation".
5. **Windows and buffers.** (Macros `q`/`@`, lowercase marks, the jumplist, the
   context mark and scrolling are done: `<C-e> <C-y> <C-f> <C-b> <C-d> <C-u> zt
   zz zb z<CR> z. z- H M L` with counts, 'scrolloff'.) Left: splits, buffer
   list, the remaining marks (uppercase, `'[`/`']`, `'<`/`'>` as motions); then
   swap files (see "Recovery"). Any jump command added later (`[[`, `]]`) must
   set the context mark.
6. **Modern editing experience.** hlsearch, incsearch, matching parens,
   syntax highlighting (tree-sitter), line numbers, a config file, a command
   palette, mouse, the start menu (see "Start menu"), and a `:Health` report
   (see "Neovim-inspired features"). Details in "Editing experience".
7. **Coding agent integration.** Automatic detection of installed agents
   (Claude Code, Codex, Cursor, Kimi Code, ...) and connection to them over
   ACP or the agent's own IDE integration, with a files-only fallback. File-change detection and reload as an
   undoable step, a conflict flow, a diff view with per-hunk accept/revert,
   editor context for the agent, a terminal pane. Details in "Agent
   integration".
8. **Prediction.** Inline suggestions (ghost text) from a pluggable provider,
   accepted with a key. Details in "Prediction".
9. **Extras (optional).** A built-in LSP client (see "Neovim-inspired
   features"), a Vimscript subset, Lua scripting (see "Lua extensions"), a
   plugin API that is compatible with Neovim plugins in stages (see "Neovim
   plugin compatibility"), and the other Neovim-inspired features listed in
   "Neovim-inspired features" that earlier milestones do not cover: the RPC
   server and `--remote`, async jobs, floating windows and popup menus,
   `vim.ui` hooks, and tree-sitter folding (which needs Vim's fold commands
   first; they are not in the plan yet, see that section).

## Foundation (milestone 4)

### Document, Window, Editor

Split today's `Editor`:

| Type | Holds |
|---|---|
| `Document` | rope (`Buffer`), undo tree, marks, file path and mtime, `modified`, line ending/EOL flags, `tracked` lines, buffer-local options |
| `Window` | document id, cursor, `curswant`/`keep_curswant`, scroll position (`View.top` moves here), Visual start, window-local options, jumplist |
| `Editor` | `documents`, `windows` and layout, current window, registers, search state, last `:s`, `.` repeat state, mode and pending keys, command line, messages, global options |

Rules: undo and marks belong to the document, so two windows on one document
share them; cursor and scroll belong to the window. Existing code that uses
`self.buf` and `self.cursor` goes through accessors for the current window and
document, so the split lands in small steps. Document ids are stable (not
indexes) because agent and LSP events refer to them. Each document has a
version counter that increments on every change; asynchronous results
(highlighting, predictions, diagnostics) carry the version they were computed
for and are dropped when it no longer matches.

### Event loop

`event::read()` blocks. Replace it with an internal event enum fed through a
channel:

```
enum Event { Key(Key), Paste(String), Resize(w, h), FocusGained, FocusLost,
             Timer(TimerId), FileChanged(DocId),
             /* later: */ Lsp(..), Agent(..), Prediction(..) }
```

- `main.rs` owns the threads: one reads crossterm events and converts them;
  others (file watcher, agent socket, prediction provider) send into the same
  channel. `Editor::handle_event(Event)` stays terminal-free and is what the
  tests drive; `handle_key` becomes a thin wrapper.
- Timers are requested by the editor (`Editor::next_deadline()`) and fired by
  `main.rs`. First user: mappings with `timeoutlen` (an ambiguous prefix waits
  for more keys, then resolves). Later users: prediction debounce, autosave,
  file checks.
- Coalesce bursts: before drawing, drain all queued events, so a held key or
  a large paste does not redraw per event.
- Nothing blocking runs on the UI thread. Anything that can take long (big
  file reads, process I/O, network) runs on a worker and reports back with an
  event.

### Styled frame and diff redraw

- `Frame` becomes a grid of cells `{ ch, width, style }` (or rows of styled
  spans). `Style` has foreground, background and attributes (bold, italic,
  underline, reverse). The `view.rs` layout code stays a pure function and
  gets unit tests that assert on styles.
- Draw the Visual area first (`v`, `V`, `<C-v>`, including `$` in blockwise
  and the empty-line cell in linewise). Then `hlsearch`, `incsearch` (live
  preview while typing `/`), matching paren, the cursor line, and syntax spans
  from milestone 6. Highlight groups are named (`Visual`, `Search`,
  `MatchParen`, `Comment`, `Suggestion`, ...) and mapped to styles by a theme,
  so themes and colour fallbacks need no layout changes.
- `main.rs` keeps the previous frame and writes only the changed runs, with a
  full repaint on resize, after suspend, and on `<C-l>`. Wide characters and
  the wrap rules in `text.rs` must keep working: compare by cell, not by byte.

### Input

- Enable bracketed paste (`EnableBracketedPaste`, disabled again in
  `restore_terminal`). A paste is one `Event::Paste`. In Insert mode it is
  inserted as one chunk and one undo step, with no autoindent or mappings
  applied. In Normal and Visual mode it is never executed as commands (it is
  put like `p` of a charwise register, or ignored). In the command line it is
  inserted literally with newlines dropped.
- `Key` grows modifiers: Alt, Shift for non-characters, F1-F12, Shift-Tab,
  Insert, Ctrl with non-letters. `convert()` maps them all; keys with no
  meaning are ignored, not errors. The Vim notation parser used by tests gets
  `<A-x>`, `<F5>`, `<S-Tab>`, `<Insert>`.
- Where the terminal supports it, push the kitty keyboard protocol flags
  (`PushKeyboardEnhancementFlags`, disambiguate escape codes), so `<C-i>` vs
  `<Tab>`, `<C-m>` vs `<CR>` and `<Esc>` vs Alt-key are told apart. Fall back
  silently to legacy input. This also removes the escape-time delay.

### Options registry

Replace `Options { ignorecase, smartcase, wrapscan }` with a typed registry:
name, short name, type (bool, number, string, enum), default, and scope
(global, buffer-local, window-local). `:set`, `:setlocal`, `:setglobal`,
`:set opt?`, `:set opt!`, `:set noopt`, `:set opt&` and `+= -= ^=` work from
the table, and a change notifies the view and editor. A new option is one
table entry plus the code that reads it.

First options on the registry: `autoindent`, `expandtab`, `shiftwidth`,
`softtabstop`, `tabstop`, `hlsearch`, `incsearch`, `scrolloff`, `wrap`,
`number`, `relativenumber`, `autoread`, plus the current three. Defaults are
Vim's `-N` defaults; rvim's friendlier defaults live in the default config
file (milestone 6), not in the table, so the differential tests are
unaffected. The harness learns to set an option in both rvim and Vim before a
case runs.

### Recovery

- Milestone 4: the panic hook writes every modified document to a recovery
  file (`$XDG_STATE_HOME/rvim/recovery/<name>.<time>` or the platform
  equivalent) and prints its path after the terminal is restored. Terminal
  close and SIGHUP/SIGTERM do the same on Unix.
- Milestone 5: Vim-style swap files with `directory`, periodic writes
  (`updatetime`, `updatecount`), the "swap file exists" prompt on open, and
  `:recover`. Recovery files from the panic hook are offered at the next
  start.

## Editing experience (milestone 6)

- **Highlighting**: hlsearch with `:noh`, incsearch, `MatchParen`, syntax
  highlighting via tree-sitter grammars per filetype, filetype detection by
  extension and shebang. Highlighting runs off the UI thread on a snapshot and
  posts spans back as an event (see document versions above).
- **Chrome**: line numbers (`number`, `relativenumber`), a real status line,
  and a buffer/tab line once there are buffers.
- **Defaults**: a built-in modern profile (`hlsearch`, `incsearch`,
  `autoindent`, `scrolloff=5`, `ignorecase smartcase`, mouse, true-colour
  theme) applied only when `--modern` is given or the config sets
  `[profile] modern = true`. No config and no flag means Vim's `-N` defaults.
  (refines the earlier wording "when a user config exists": a config with only
  other tables, such as `[agent]`, changes no default and no key.)
- **Config**: `$XDG_CONFIG_HOME/rvim/config.toml` for options, mappings and
  theme. Start with data, not a language; a Vimscript subset is milestone 9.
- **Discoverability**: a command palette (fuzzy search over Ex commands,
  options and rvim commands), `:help` covering what rvim implements,
  which-key style hints for pending operators and prefixes, and Ex
  completion (`<Tab>` on commands, options, file names).
- **Mouse**: click to place the cursor, drag for Visual, wheel to scroll.

### Start menu

A start screen like Neovim's intro screen. It is shown when rvim starts with
no file argument and the buffer is empty. It is an extension, so it follows
the Vim contract: with no config and no flag it is off, and rvim shows only
the one-line version message (as `vim -N -u NONE` shows only its intro text).

**Switch.** The first matching source wins:

| Source | Effect |
|---|---|
| `--start-menu` or `--no-start-menu` | forces the menu on or off for this run |
| `start.menu = true` or `false` in `config.toml` | the default when no flag is given |
| modern profile (a config file exists, or `--modern`) | on, unless `start.menu = false` |

**Content**, from top to bottom, centred in the frame (drawn to fit 80x24,
the size the golden tests use):

1. Logo. Until a logo exists (see "Future goals") it is the word `rvim` in a
   box. The logo is hidden when the window is narrower than 60 columns or
   shorter than 20 rows.
2. The version line: `VERSION_MESSAGE`, the same text as `:version`. It is
   always shown and cannot be turned off, because the Vim license (condition
   II.3) requires the modified-version message in the intro screen.
3. Menu items, each a line that can be selected:
   - New file: an empty unnamed buffer (as `:enew`).
   - Open file: puts `:e ` on the command line.
   - Recent files: up to `start.recent_files` paths (default 10, a choice
     not yet made), most recent first. A file that no longer exists is shown
     dimmed and is not opened.
   - Config: opens `$XDG_CONFIG_HOME/rvim/config.toml`. A missing file opens
     as an empty buffer named after it; nothing is written until `:w`.
   - Help: `:help` once it exists. Until then the item is shown dimmed.
   - Quit: `:q`.
   - Agent line (milestone 7): shown only when an agent is configured or
     detected, for example `Agent: Claude Code (detected, not connected)`.
     Absent otherwise.
4. A hint line: `arrows select  Enter open  Esc close`.

**Keys.** Vim's Normal mode uses nearly every letter, so the menu has no
letter shortcuts. Items are chosen with the arrows and `<CR>`, or by a mouse
click on the item (the mouse is part of milestone 6, see "Mouse" above). While
the menu is shown:

| Key | Effect |
|---|---|
| `<Up>`, `<Down>` | move the selection (in Vim these are `k` and `j`) |
| `<CR>` | runs the selected item and closes the menu (in Vim it moves down a line) |
| `<Esc>` | closes the menu and does nothing else |
| any other key | closes the menu, and the key is handled by Vim exactly as it is with no menu |

Rules:

- The menu takes only the keys in the first three rows. `<Up>`, `<Down>` and
  `<CR>` change their Vim meaning while the menu is shown, and their Vim
  meaning returns when it closes. This is allowed only because the menu is on.
- Any other key is not consumed. `3j`, `o`, `:`, `i` and `q` act as they do
  with no menu. A count, register prefix or `:` starts that command in the
  same step.
- The menu is an overlay drawn on the Frame, not buffer text. The buffer,
  cursor, registers and undo tree are the same as with no menu. Closing the
  menu never runs an action the user did not choose.
- A resize keeps the menu open and lays it out again. `:Start` (a new Ex
  command, capital letter as required) shows the menu again.

**Storage.** Recent files are kept in `$XDG_STATE_HOME/rvim/recent` (default
`~/.local/state/rvim/recent`), one path per line. A worker writes the file on
open and save, and only while the menu is enabled. With the menu off, rvim
writes nothing for it.

**Testing.**

- Frame golden screens: the menu at 80x24, at 40x20 (logo hidden), with an
  agent line, and with one item selected (the style of the selected row is
  checked, not only its text). Done in `tests/start_menu.rs` as structural
  assertions on the rows and styles (not golden files); the agent line is
  not built yet.
- Dismissal table: for every key class (letters, digits, `:`, `<Esc>`,
  arrows, `<CR>`, Ctrl keys, function keys) a unit test runs the sequence with
  the menu on and with the menu off. For keys outside the menu's set it
  asserts that buffer, cursor, registers and undo tree are equal. For the
  menu's set it asserts the effect in the table above.
- Differential tests: the harness runs Vim with `-N -u NONE`, so differential
  cases always run with the menu off. A test asserts that the menu is off
  with no config and no flag.
- Recent-file list: unit tests for write on open, missing files and the
  maximum count, using a temporary state directory.

**Status (implemented 2026-11-01, branch `worktree-agent-a4f51c1b0784be30f`).**

- Done: the `--start-menu` flag (and `--no-start-menu`, which is the default)
  sets the option `startmenu`, a Global bool that is off by default. It is not
  a Vim option, so `:set startmenu` is accepted here and gives E518 in Vim.
  `Editor::show_start_menu` shows the menu only with the option on, no file
  argument, an empty unnamed buffer and no changes; otherwise the intro
  message is shown as before. Content: the logo (hidden below 60x20), the
  `VERSION_MESSAGE` (always shown, word-wrapped), entries New file, Open file,
  Help (dimmed, skipped by the selection), Quit, and the hint line. Keys and
  dismissal follow the tables above; `startmenu.rs` holds the state, the
  layout and the key handling, and `view::frame` draws it through
  `Hl::MenuSelected` and `Hl::Dimmed`. A paste closes the menu. A resize keeps
  it open. Tests: `tests/start_menu.rs`.
- Entry actions: New file closes the menu (at startup the buffer already is
  the empty unnamed one). Open file puts `:e ` on the command line. Quit runs
  `:q`.
- Still to do: `:Start` (needs `ex.rs`, not touched yet); `:enew`, so New file
  can open a new buffer after startup; the recent-files list and its state
  file `$XDG_STATE_HOME/rvim/recent`; the Config entry and the config switch
  `start.menu` (no config file exists yet); the modern-profile default (menu on
  with `--modern`); the agent line (milestone 7); `:help`, which enables the
  Help entry; the logo (the word `rvim` in a box stands in for it).

## Agent integration (milestone 7)

The user chooses the agent. rvim does not embed one; it cooperates with
whichever agent edits the user's files. Two layers: files on disk (works with
any agent, no setup) and an optional local endpoint for richer integration.

### Detecting and connecting to agents

rvim finds the coding agents installed on the machine and connects to them,
so the user does not wire up each one by hand. Candidates: Claude Code,
Codex, Cursor (agent/CLI), Kimi Code, and any other agent in the ACP registry.
The list is data (a table in the source plus user additions in the config),
not code, so a new agent is one entry.

rvim plays the role that programs such as OpenClaw play: a host that finds
coding agents and drives them over a standard protocol (OpenClaw does this
with its ACP client plugin). OpenClaw itself is not a supported agent and is
not in the registry; it is only the model for how rvim should connect.

- **Detection** (local only, read-only, runs on a worker at startup and on
  `:AgentScan`): for each known agent, look for its executable on `PATH` and
  in the usual install locations, its config directory in the user's home,
  and any running instance (a lock file, a socket, a listening local port, or
  a process name). Each result is `Found { agent, version, how_to_connect }`
  or `NotFound`. Detection never starts an agent, sends data, or reads
  conversation history or credentials; it only checks that things exist.
- **Registry entry** per agent: id, display name, executable names, config
  paths, running-instance probes, preferred connection method(s), the command
  line to launch it in a terminal pane, and the file patterns it edits
  (for the file-change layer).
- **Connection methods**, tried in order of capability, per agent:
  1. **A standard editor-agent protocol** where the agent speaks one. The
     Agent Client Protocol (ACP, JSON-RPC over stdio, used by editors such as
     Zed) is the first target, since several of these agents have an ACP
     mode or adapter. rvim acts as the client: it starts the agent process
     (or attaches to a running one), sends prompts with editor context, and
     receives edits, diffs and permission requests.
  2. **The agent's own IDE integration**, where it advertises one by a
     discovery file (Claude Code's IDE integration is one; it finds the
     editor through a lock file and a local WebSocket). rvim publishes the
     same discovery file and serves the context and open-file calls, so a
     Claude Code started in rvim's terminal pane sees the current file,
     selection and diagnostics.
  3. **Files only** (always available): launch the CLI in a terminal pane or
     let the user run it elsewhere; rvim sees its edits through the
     file-change layer below. This is the fallback when nothing better is
     detected, and it needs no protocol support.
- **Research findings (2026-10-31, from official docs unless marked).**
  Facts below were looked up; items marked UNVERIFIED still need a check
  against the agent's own docs or a real install before a connector is
  written.

  | Agent | Executable | Best connection | Notes |
  |---|---|---|---|
  | Claude Code | `claude` (`claude.exe`); native install `~/.local/bin/claude`, Windows `%USERPROFILE%\.local\bin\claude.exe`; config `~/.claude`, `~/.claude.json`, `CLAUDE_CONFIG_DIR` | (a) rvim as IDE server: WebSocket on 127.0.0.1 plus lock file `~/.claude/ide/<port>.lock`; (b) ACP through the adapter `@agentclientprotocol/claude-agent-acp` (npx; older name `@zed-industries/claude-agent-acp`) | IDE protocol is JSON-RPC over WebSocket following MCP; lock file fields (`pid`, `workspaceFolders`, `ideName`, `transport`, `authToken`), header `x-claude-code-ide-authorization`, env `CLAUDE_CODE_SSE_PORT` and `ENABLE_IDE_INTEGRATION`, and the tool names (`openFile`, `openDiff`, `getCurrentSelection`, `getDiagnostics`, ...) come from the third-party write-up `coder/claudecode.nvim` PROTOCOL.md, not Anthropic docs: UNVERIFIED. From an external terminal the user runs `/ide` to attach. Headless: `claude -p`, `--output-format stream-json`. |
  | Codex | `codex`; config `~/.codex` (UNVERIFIED) | ACP through the adapter `codex-acp` (`npx @zed-industries/codex-acp`, development reportedly moved to `@agentclientprotocol/codex-acp`; UNVERIFIED); or `codex app-server` (JSON-RPC: `initialize`, `thread/start`, `turn/start`, approvals; launch command UNVERIFIED) | Headless: `codex exec`, flags UNVERIFIED. No documented lock file or socket. |
  | Cursor | `agent` (docs); `cursor-agent` is UNVERIFIED; Unix `~/.local/bin/agent`; install `curl https://cursor.com/install` | Native ACP: `agent acp`, newline-delimited JSON-RPC on stdio. Auth method `cursor_login` after `agent login` or `CURSOR_API_KEY`. | Extension methods `cursor/ask_question`, `cursor/create_plan`, `cursor/update_todos`, `cursor/task`. Headless: `agent -p`, `--output-format stream-json`. `agent` is a generic process name, so detect by path, not by name alone. |
  | Kimi Code | `kimi`; new CLI `kimi-code` (config `~/.kimi-code`, `KIMI_CODE_HOME`); legacy `kimi-cli` (config `~/.kimi`, archived September 2026) | Native ACP: `kimi acp` (user must `/login` first) | Windows needs Git for Windows. `kimi -p` is headless but auto-approves tool calls (opt-in only). `kimi web` is a local server on 127.0.0.1:58627 (API UNVERIFIED). Detect both config directories. |
  | Others via the ACP registry | per entry | ACP | An ACP agent registry exists (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`, about 58 agents including Gemini CLI `gemini --acp`, Goose, OpenCode, Qwen Code, Copilot CLI). Detection can read it, so new agents need no code. |

- **ACP is the first connector.** Every agent above except Claude Code's IDE
  mode can be reached over ACP, so one connector covers most of the list.
  What an ACP client (rvim) has to do, from the protocol docs
  (`agentclientprotocol.com`, protocol v1 is the stable line, v2 is draft):
  JSON-RPC 2.0 with the agent as a child process; call `initialize`
  (capability negotiation), `authenticate`, `session/new`, `session/prompt`,
  `session/cancel`; receive `session/update` notifications (message chunks,
  tool calls, plans, mode changes); answer `session/request_permission`
  (options `allow_once`, `allow_always`, `reject_once`, `reject_always`);
  optionally serve `fs/read_text_file` and `fs/write_text_file` (so the agent
  reads rvim's unsaved buffer text and its writes can be shown as a diff
  before they land) and the `terminal/*` methods. Prompts carry context as
  content blocks (`text`, `resource_link`, embedded `resource`); there is no
  selection type, so a selection is sent as an embedded resource with a
  `file:///path#Lstart-end` URI (UNVERIFIED convention). Edits come back as
  tool-call `diff` items holding whole old and new file text; rvim computes
  hunks itself. Only call optional methods the other side negotiated. The
  newline-delimited stdio framing is UNVERIFIED in this research; read the
  transports page first. Rust crate `agent-client-protocol` 3.6.0
  (Apache-2.0) exists; check its dependency footprint (tokio is likely)
  before adding it, and keep it out of the terminal-free core, in the
  connector module.
- **Claude Code IDE server is the second connector** (rvim writes the lock
  file and serves selection/open-file/diff calls), because it is what makes
  `/ide` and "at-mentions" in a Claude Code started in rvim's terminal pane
  work. Both Claude paths (ACP adapter and IDE server) should be offered;
  which is better is decided by trying them against a real install.
- **Headless fallback** (`claude -p`, `codex exec`, `agent -p`, `kimi -p`) is
  for one-shot commands such as `:Agent` with a prompt when no session is
  connected; auto-approving modes are never used without an explicit option.
- **Protocol details must be checked, not assumed.** Before implementing a
  connector, read the agent's current documentation and test against the real
  agent; record the verified protocol version in the registry. Protocols
  change, so each connector is an isolated module behind one `AgentConnector`
  trait (`detect`, `connect`, `send_context`, `send_prompt`, `events`) and
  failure of one never affects the editor or the others.
- **Connecting** is explicit the first time per agent and per project:
  `:Agent` lists what was found and offers connect; a one-line prompt says
  what will be shared (file path, selection, open buffers). The choice is
  remembered in the config, can be revoked with `:AgentForget`, and a
  project-level allow-list decides which directories may be shared.
  Automatic *detection* needs no consent; automatic *connection* only happens
  for agents the user already approved, and only to local endpoints.
- **Multiple agents**: several can be connected at once; one is the active
  target for `:Agent <prompt>` and the Visual-mode send key, switchable with
  `:AgentUse <name>`. The status line shows the active agent and its state
  (idle, working, waiting for permission, disconnected).
- **Permission requests** from an agent (write a file, run a command) show as
  a prompt in rvim and are never auto-approved. Edits an agent proposes over
  a protocol arrive as a diff and apply as one undoable step tagged with the
  agent's name, using the same review flow as external file changes.
- **Reconnect and lifecycle**: a lost connection is retried with backoff and
  reported in the status line; quitting rvim closes connections it opened and
  stops agent processes it started, but leaves ones it merely attached to.
- **Testing**: connectors are tested against a scripted fake agent
  (stdio/WebSocket) that replays recorded protocol transcripts, and detection
  against a temporary fake home and `PATH`. Real-agent checks are a manual
  checklist, not part of `cargo test`.

### Files changed under rvim

Agents edit files while rvim has them open.

- **Detect external changes**: poll mtime and size of the open files on a
  worker thread (every 500-1000 ms) and on focus-gained, which is Vim's
  `checktime`. Enable terminal focus reporting where the terminal supports
  it. Raise `FileChanged(DocId)`. The `notify` crate is an optional later
  addition behind a feature flag: it is noisy for editors (atomic-rename
  saves, duplicate events, unreliable on network and WSL filesystems) and its
  9.0 release was still in release-candidate stage in October 2026.
- **`autoread` behaviour**: if the document is unmodified, reload at once and
  keep the cursor and scroll position as close as possible (the cursor line
  is adjusted by the diff, not reset). Show a short message naming the file.
- **Reload as an undoable step**: compute a line diff between the buffer and
  the new file and apply it as one undo step (not a fresh load). `u` reverts
  the agent's change and `<C-r>` re-applies it. Marks and tracked lines move
  with the diff. Each undo step records its source (user, external reload,
  paste, prediction).
- **Conflict flow**: if the document has unsaved changes and the file changed
  on disk, never reload silently. Prompt: load the disk version (the buffer
  text stays in undo), keep mine, show diff, or merge (three-way, with the
  last-saved text as base; conflicts marked in the buffer). `:w` on a file
  that changed on disk asks before overwriting (Vim's `W11`/`W12` family).
- **Deleted or renamed files**: keep the buffer, mark it, and warn (Vim `W13`).
- **No echo**: rvim's own writes must not trigger a reload (record mtime and
  hash at save).

### Reviewing agent changes

- A **diff view** of what changed on disk since the buffer was last in sync:
  side by side or unified in a split (needs milestone 5), with hunk
  navigation (`]c`, `[c`). Vim's `:diffthis`/`:diffoff`/`do`/`dp` family is
  the base, because that part is Vim behaviour and gets differential tests.
- Per-hunk **accept and revert**, so the user can keep part of an agent's
  change and drop the rest.
- Changed lines marked in the gutter since the last save and since the last
  external change; a list of recent changes with their source, from the undo
  tree's step metadata.
- A baseline per document (last saved text, or git `HEAD`) to diff against,
  with commands such as `:DiffSaved` and `:DiffGit`.

### Letting the agent see what the user sees

- **Context export**: an optional local endpoint (Unix socket or named pipe,
  path in `$RVIM_SOCKET`, also exported to child processes) answering
  queries: current file, cursor, Visual selection text and range, visible
  range, open documents, modified state, diagnostics later. Read-only by
  default.
- **Agent commands** over the same endpoint, opt-in per session: open a file
  at a line, jump to a location, show a diff, apply an edit as an undoable
  step, set a message. This lets an editor-aware agent point at code instead
  of rewriting files blindly. The wire format is small JSON-RPC, documented in
  `docs/agent-protocol.md`; adapters for specific agents live outside the
  core.
- **Selection to agent**: a command (`:Agent <prompt>`) and a Visual-mode key
  that send the selection, path and line range to the configured agent
  command, or copy a reference (`path:start-end`) to paste into any agent.
- **Terminal pane**: run the agent's CLI in a terminal window (a PTY window
  type, needs milestone 5) so editor and agent share one screen, with
  `<C-\><C-n>` to leave Terminal mode as in Vim's `:terminal`.
- **Safety**: the endpoint listens only on a user-owned socket (mode 0600, or
  a per-user pipe ACL). Nothing leaves the machine unless the user configures
  a command that does it. No telemetry.

## Agent experience: how it fits together

This section fixes the user-facing path through the agent parts above. Where
it repeats a rule, the rule is the same. Where it makes a rule concrete, the
text says so, and where it refines an earlier statement it is marked
"(refines ...)". Nothing here is on by default: with no config and no flag,
rvim is `vim -N -u NONE`, and the agent features start only at an explicit
opt-in or an explicit `:Agent...` command. UNVERIFIED marks a protocol or
Vim detail that must be checked before it is built.

### Summary

| User action | What rvim does | Key or command |
|---|---|---|
| Looks for installed agents | Read-only scan; lists what it found; connects to nothing | `:AgentScan` |
| Sends a prompt the first time | Consent prompt for this agent and project; on yes, connects and sends | `:Agent <prompt>` |
| Sends a prompt later | Sends the prompt with the cursor and open-file paths; reply streams into the agent log | `\a`, then Enter; or `:Agent <prompt>` |
| Sends a selection | Opens `:'<,'>Agent ` with the selected range | Visual `\a` |
| Stops a turn | Sends `session/cancel` | `\ac`, `:AgentCancel` |
| Agent edits a file that is open and unmodified (opt-in only) | Reloads it as one undo step, tagged by the tagging rule in "The edit loop"; cursor and scroll follow the text. Without the opt-in: reported, not reloaded (Vim) | none (automatic) |
| Agent edits a file with unsaved changes | Conflict prompt; never a silent reload | `l` load disk, `k` keep mine, `d` diff, `m` merge |
| Agent asks permission | Modal prompt on the message line; rvim never answers it by itself. `a` and `N` also cover later requests of the same kind and path scope for this session only (shown in the status line, logged; never saved) | `y` `a` `n` `N` (`<Esc>` rejects once) |
| Reviews changes | Gutter marks; hunk navigation; accept or revert a hunk | `]c` `[c` `dp` (accept) `do` (revert); `:AgentChanges` |
| Switches the active agent | Sets the target of `:Agent` and `\a` | `:AgentUse <name>` |
| Reads the agent's output | Read-only agent log document | `\al`, `:AgentLog` |
| Runs Claude Code inside rvim | Terminal pane with the CLI; needs the agent's approval for this project first (consent prompt otherwise) | `:AgentTerminal`, `<C-\><C-n>` |
| Attaches Claude Code from outside | rvim's IDE server (lock file) | `/ide` in Claude Code |
| Revokes trust | Disconnects; removes the approval and this session's permission grants for this project | `:AgentForget[!]`, `:AgentGrants` |
| Has no agent | Files-only mode: reload on change, diff against saved text or git | `:DiffSaved`, `:DiffGit`, `:checktime` |

### Keys and commands

Keys. Vim's status is given for each, because a key may only be bound when
its extension is on (CLAUDE.md: an extension never changes a Vim key sequence
while it is off).

| Mode | Key | Action | Vim today |
|---|---|---|---|
| Normal | `\a` | Opens the command line with `:Agent ` | `\` has no default command (UNVERIFIED; check with `:map \` and a probe in Vim) |
| Visual | `\a` | Opens the command line with `:'<,'>Agent ` | same (UNVERIFIED) |
| Normal | `\ac` | `:AgentCancel` | same |
| Normal | `\al` | `:AgentLog` | same |
| Normal (modern profile only) | `<Space>a` | Same as `\a` | `<Space>` is `l`, so the modern profile changes it; the prefix is taken only with the profile on |
| Diff view | `]c` `[c` | Next and previous hunk | Vim, diff mode only |
| Diff view | `dp` in the buffer window | Accept hunk: copy the buffer's hunk into the baseline | Vim, diff mode only |
| Diff view | `do` in the buffer window | Revert hunk: copy the baseline's hunk into the buffer | Vim, diff mode only |
| Outside diff view | `]c` `[c` | Jump between gutter-marked lines | no-op in Vim (UNVERIFIED); bound only with agent keys on |
| Agent log window | `<C-c>` | `:AgentCancel` | Vim `<C-c>` elsewhere, unchanged |
| Any | `<C-^>` in the agent log | Back to the previous document (before splits) | Vim alternate file |

The leader is `\` (Vim's default `mapleader`) with the agent keys on. `<Space>`
is bound only with the modern profile, because Vim uses it as `l`; the modern
profile changes that key, which is allowed only because it is opt-in.

Ex commands (all capitalised, so they do not clash with Vim's):

| Command | Effect |
|---|---|
| `:AgentScan` | Runs detection on a worker; lists what was found with its state. The version comes from a package manifest beside the install (read as a file), or is shown as `version unknown`. The binary is never run (no `--version`). Connects to nothing. |
| `:Agent` | No text: lists agents and their states. With text: sends it to the active agent (see Prompting). With a range: sends the range as the selection. |
| `:AgentConnect [name]` | Consent prompt if not yet approved for this project, then connects. |
| `:AgentDisconnect [name]` | Closes the connection and keeps the approval. Stops the process only if rvim started it. |
| `:AgentForget[!] [name]` | Disconnects and removes the approval and this session's permission grants for this project. `!` does it for every project. |
| `:AgentGrants` | Lists this session's permission grants for this project. |
| `:AgentUse [name]` | Sets the active agent. No name: lists the connected agents. |
| `:AgentCancel` | Cancels the active agent's turn. |
| `:AgentLog [name]` | Shows the agent's output document. |
| `:AgentChanges[!]` | Recent changes with source tags (`!`: all documents). |
| `:AgentTerminal [name]` | Runs the agent's CLI in a terminal pane. Gated like a connection: the agent needs approval for this project, or the consent prompt comes first. See "Terminal pane". |
| `:DiffSaved`, `:DiffGit` | Diff the buffer against its baseline: last saved text, or git `HEAD` (run on a worker). The git text is requested for the document version at the time; if the buffer changed before it arrives it is dropped and the command must be run again (the same rule as every asynchronous result). |

Config (the opt-in; the config file itself is milestone 6, so until then only
this table is read, and the same loader is used):

```toml
[agent]
enabled = true              # agent keys, startup scan, file watching, gutter marks
shareable = ["C:/proj"]     # directories rvim may share; default: the project root
log = "events"              # or "transcript" (JSON-RPC with tokens redacted)
```

`rvim --agent` is the same as `enabled = true` for one run.

The `[agent]` table turns on nothing else. It does not enable the modern
profile (`--modern` or `[profile] modern = true`), so `<Space>a` (the leader
form) and `<Tab>` as the prediction accept key come only with that profile.
Each is its own opt-in.

### 1. First run

| Start | What the user sees | Keys | Startup work |
|---|---|---|---|
| No config, no flag | Nothing: no status indicator, no message | Vim's | none |
| `enabled = true` or `--agent` | Once a scan finds something, one message per session: `ai: found claude-acp, codex. :AgentConnect to use`. Status indicator shown (see Lifecycle) | Agent keys on | Scan on a worker at startup |
| Explicit `:AgentScan`, `:Agent`, `:AgentConnect` (no opt-in) | Result on the message line. From then on, for this session only, the status indicator is shown too | Agent keys stay off | Only what the command asks for |

(refines "Detecting and connecting to agents": startup detection runs only
with the opt-in, so a Vim user who never opts in sees no extra work.)

The consent prompt, exact text (one message line; `?` gives the rest):

```
Connect Claude Code for C:/proj? It sees open file paths and the file, cursor and selection you send with a prompt. It reads other files only inside shareable directories. [y]es [n]ot now [?] details
```

| Key | Result |
|---|---|
| `y` | Stores the approval, connects, sends the prompt that was waiting |
| `n`, `<Esc>` | Not now. Nothing stored; asked again the next time the agent is used |
| `?` | Opens the full text in the agent log: the exact paths that would be sent, the directories the agent may read, and that the agent's own service may receive what it reads (that is the agent's policy, not rvim's). The prompt stays open |

Storage. Approvals are in `agent-approvals.toml` in the rvim config directory
(`$XDG_CONFIG_HOME/rvim/`, Windows `%APPDATA%\rvim\`), not in `config.toml`,
so rvim never rewrites the file the user edits. (refines "The choice is
remembered in the config".)

```toml
[[approval]]
agent = "claude-acp"             # registry id
project = "C:/Users/axel/proj"   # canonical path, case-folded on Windows
protocol = "1"                   # protocol version the connector was verified against
approved = "2026-11-01"
```

Permission grants are not stored at all: they live for the session only (see
Permissions).

- Written atomically (temp file, then rename). If the file cannot be parsed,
  rvim treats it as empty, shows `agent-approvals.toml unreadable; no
  approvals loaded`, and does not overwrite it until the next approval.
- Project root: the nearest ancestor of the active file that contains `.git`;
  otherwise the directory rvim was started in. Both canonicalised.
- Approval is per (agent id, project root). `claude-acp` and `claude-ide` are
  separate ids with separate approvals.
- `:AgentForget` (or `:AgentForget!`) removes these records and this session's
  grants; it is the only way to revoke an approval. Approvals never expire.

### 2. Lifecycle

One state machine per agent. Its pure part (states, events, transitions,
backoff) is in `agent/state.rs`, with no I/O, so it is tested directly.

| State | Meaning | Status line (ASCII) |
|---|---|---|
| NotInstalled | Not found by the last scan | `ai: none` after a scan; nothing before one |
| Found | Executable or config seen; not approved here | `ai: claude found` |
| Connecting | Process started or socket opened; `initialize`, `authenticate`, `session/new` in progress. Also the brief stage after approval | `ai: claude connecting` |
| Idle | Connected, no turn running | `ai: claude` |
| Working | A `session/prompt` turn is running | `ai: claude working 12s` (seconds shown after 5 s; an ASCII spinner cycles at the start) |
| WaitingPermission | The turn is paused on a request | `ai: claude PERMISSION 2` (reverse video; count of queued requests) |
| Disconnected | Connection lost; retry scheduled | `ai: claude retry 2/5` |
| Error | Could not connect, or gave up retrying | `ai: claude error` (error style; detail once on the message line) |

The indicator is drawn when the agent layer is active: the opt-in is on, or an
Agent command (`:AgentScan`, `:Agent`, `:AgentConnect`, ...) has run in this
session. With neither, no indicator is drawn and the frame is Vim's. With the
layer active and no agent connected, the indicator reads `ai: files only`
(section 8).

Transitions:

| From | Event | To | Action |
|---|---|---|---|
| NotInstalled, Found | scan finds the agent | Found | none |
| Found | `y` at consent, or `:AgentConnect` with approval already stored | Connecting | store approval (first time) |
| Found | `n` at consent | Found | nothing stored |
| Found (approved here) | first `:Agent` in a session | Connecting | lazy: no connection at startup |
| Connecting | `initialize`, `authenticate`, `session/new` succeed | Idle | becomes active if none is |
| Connecting | no reply to `initialize` in 10 s; auth error; process exits | Error | message from the table in "Failures" |
| Idle | `session/prompt` sent | Working | none |
| Working | stop reason returned (end of turn) | Idle | none |
| Working | `session/request_permission` | WaitingPermission | prompt shown (Permissions) |
| WaitingPermission | last queued request answered | Working | none |
| WaitingPermission | `\ac` | Working until the stop reason | every queued request answered as cancelled; `session/cancel` sent |
| Idle, Working, WaitingPermission | EOF, socket error, process exit | Disconnected | turn (if any) ends with `claude: connection lost`; retry timer |
| Disconnected | reconnect succeeds | Idle | the lost turn is not resumed |
| Disconnected | 5 retries failed | Error | none |
| Error | `:AgentConnect` | Connecting | none |
| any connected | `:AgentDisconnect`, `:AgentForget`, rvim quits | Found | close; stop only processes rvim started |

Backoff: retries at 1, 2, 4, 8 and 16 seconds, then Error. A timer is requested
through `Editor::next_deadline()` and fired as an event, so nothing sleeps on
the UI thread. A successful connect resets the count.

Several agents keep separate states. The status line shows the active agent
and its state, with `(+N)` for other connected agents, and the `PERMISSION`
state of any agent is shown even when that agent is not active.

### 3. Prompting

`:Agent <text>` sends `<text>` as typed (the rest of the line, `|` is not a
separator). The target is the active agent. If no agent is active:

- one found agent and approved here: it becomes active on connect;
- one found agent not yet approved here: consent prompt, then send;
- several found and none active: message `ai: several agents found, pick one with :AgentUse` (no prompt).

`\a` (Normal) and Visual `\a` only fill the command line; the user finishes the
text and presses Enter. Nothing is sent until Enter.

Protocol surface used in this section: `initialize`, `authenticate`,
`session/new`, `session/prompt`, `session/cancel`, `session/request_permission`,
`session/update`, `fs/read_text_file`, `fs/write_text_file` and `terminal/*` are
confirmed from the ACP overview page only. Field-level shapes (option kinds,
tool-call kinds and locations, `resource_link`, stop reasons, cancelled
outcome) are UNVERIFIED until read from the protocol reference and tested
against a real agent.

Context attached to each prompt:

| Item | When | Sent as |
|---|---|---|
| Active file path | the buffer has a name | `resource_link` (UNVERIFIED shape) with the `file://` URI |
| Cursor | always | text block `Cursor: <path>:<line>:<col>` |
| Selection | the command had a range | embedded resource `file:///<abs>#L<start>-<end>` with its text (URI convention UNVERIFIED) |
| Open file paths | always; paths only | `resource_link` list (UNVERIFIED shape), at most 50, then `(+N more)`; filtered by shareable directories |
| Buffer text | never sent in the prompt | served only when the agent calls `fs/read_text_file`, and then it is the unsaved text |

Excluded everywhere, even inside shareable directories: `.env*`, `*.pem`,
`*.key`, `id_rsa*`. rvim does not list them and does not serve them.

Response pane:

- Before splits (milestone 5): the message line shows the last complete line of
  the reply, updated per redraw, truncated to the width. `\al` / `:AgentLog`
  replaces the current window's document with the agent log (one read-only
  document per agent, named `[claude]`, `[codex]`, ...). `<C-^>` returns to the
  previous document, as Vim's alternate file does.
- With splits: `:AgentLog` opens a split at the bottom, one third of the window
  height (at least 5 rows). It closes with `:q` or `<C-w>c`. Autoscroll only when
  the cursor is on the last line; otherwise the cursor and view stay where they
  are, as with `:terminal`.
- Rendering: message text as plain lines; thoughts (if sent) in the `Comment`
  group; tool calls as one line `[tool] edit src/a.rs: completed`; plans as
  `[plan] 2/5 done`; errors in the `ErrorMsg` group. No Markdown rendering until
  syntax highlighting (milestone 6).
- Streaming: chunks append to the document; a partial line is held until its
  newline. All updates in one event drain produce one redraw.

Cancel: `\ac`, `:AgentCancel`, or `<C-c>` in the agent log. `session/cancel` is
sent; pending permission requests are answered as cancelled (ACP outcome
`cancelled`, wire shape UNVERIFIED). Edits already written stay on disk and in
undo.

### 4. The edit loop

Principle: an agent's write is never held back to wait for review. It lands at
once as one undo step, and review comes after, always one `u` away.

Three ways a document changes because of an agent:

| Path | What rvim does |
|---|---|
| A. Something else writes the file on disk (a CLI in the terminal pane, a CLI run outside rvim, another editor) | The file watcher sees the change and reloads it (below), tagged by the tagging rule |
| B. ACP `fs/write_text_file` served by rvim | rvim checks the path (shareable, and permission if asked), records a write intent (path, SHA-256 of the new text, agent id, request id), writes the file, then sends `AgentWritten`. The editor applies it like a file change, tagged `agent:<id>` |
| C. ACP `diff` in a tool call | Informational only: shown in the permission prompt and the agent log. rvim never applies a diff itself |

**Tagging rule (one rule for every external write).** A changed file is tagged
`agent:<id>` only when one of these holds, and `external` otherwise:

1. the write is a served `fs/write_text_file` of a connected agent (a write
   intent matches the file's hash), or the `claude-ide` server's accepted
   `openDiff` (the same write path); or
2. a connected agent's turn reported the path as touched (ACP tool call
   locations, UNVERIFIED shape) and the change arrived while that turn was
   Working.

Writes made by a CLI in the terminal pane, by Claude Code through its own tools
(also when attached with `/ide`), or by any process outside rvim are
`external`, because they never pass through rvim's fs methods. Every other
section refers to this rule.

Echo rule. rvim does not reload its own saves: after `:w`, the synced hash is
the buffer's text hash, and the watcher ignores any disk state whose hash equals
the synced hash. Path B is not an echo. Its write is applied by the editor
when rvim finishes it (`AgentWritten`, tagged `agent:<id>`, with the same
reload or conflict rules), and that application records the hash, so the
watcher's later event for the same write is ignored. The write is applied once.

Nothing is applied "computed for an old version": the editor applies what is
on disk, so the rule that asynchronous results are dropped when the document
version has moved (CLAUDE.md) is not broken.

File watcher (worker, milestone 7 step 2): polls mtime and size of open files
every 750 ms and on focus gained; `:checktime` polls at once. When a file has
changed, the worker reads it and sends `FileChanged(DocId, text, hash,
writer)` (refines `FileChanged(DocId)`: the worker does the I/O, so the editor
receives the text). `writer` follows the tagging rule above.

When the editor applies it:

| Document | Mode | Result |
|---|---|---|
| Unmodified since the last sync | Normal, no pending keys | Reload as one undo step (below). Applied at once |
| Unmodified | Any other mode | Queued; applied on return to Normal mode. Message: `x.rs changed on disk; reloads in Normal mode` |
| Modified | Any | Conflict prompt (below). Never a silent reload |
| Deleted on disk | Any | Buffer kept and marked `[deleted]`; message `"x.rs" was deleted on disk`; `:w` writes it again |

Insert mode is never interrupted: a change that arrives while typing waits.
Leaving Insert mode with typed text makes the document modified, so the
conflict prompt appears rather than losing text.

Reload as one undo step (`autoread` on; with the opt-in it is on by default for
the session, see "Files-only fallback"):

- A line diff (`similar`, Myers) between the buffer and the new text is applied
  as line edits in one undo step tagged with the writer.
- Marks and tracked lines follow the diff, as in the existing mark code.
- Cursor: if its line is equal in both texts, it moves with that line. If its
  line is inside a deleted or replaced run, it goes to the first new line of the
  replacement, or the line before the deletion when there is none; the column is
  the desired column, clipped. Lines that did not change do not move.
- Scroll top: mapped the same way. If the top line itself changed, the view is
  adjusted so the cursor keeps its screen row where possible.
- Message: `"src/a.rs" changed on disk (agent:claude): +3 -1 lines`.
- `u` restores the text and the cursor from before the reload, as usual. `u`
  does not write the disk, so the buffer now differs from the file and is
  modified; `:w` then asks as in Vim (W11, below).
- Known difference, to record under "Known differences" when this is built:
  Vim's `autoread` reload keeps the cursor on its line number, while rvim maps
  the cursor through the diff as described above. Vim's exact placement is
  UNVERIFIED. Before the step lands, probe Vim with a file changed outside the
  harness (the harness cannot write files) and write the result down.

Conflict prompt (buffer modified and the file changed on disk). It needs the
opt-in. Without it, a modified buffer is never reloaded, the change is only
reported, and `:w` gives Vim's W11, as in Vim.

Message: `"src/a.rs" changed on disk and in rvim. [l]oad disk [k]eep mine [d]iff [m]erge`

| Key | Result |
|---|---|
| `l` | Loads the disk text as one undo step tagged `external`; the user's text is one `u` away |
| `k`, `<Esc>` | Keeps the buffer. The disk version's hash is recorded as seen, so the prompt does not come back for it. `:w` then asks: `"src/a.rs" changed since reading; overwrite? [y]es [n]o` (W11/W12) |
| `d` | With splits: opens the diff view (disk text on one side, buffer on the other); the prompt stays. Before splits: shows the unified diff in a read-only document, as `:DiffSaved` does; the prompt keys still work, and `<C-^>` returns to the buffer |
| `m` | Three-way merge: base = last saved text, ours = buffer, theirs = disk. Clean hunks merge; conflicts are written into the buffer as `<<<<<<< mine`, `=======`, `>>>>>>> disk` in one undo step. The buffer stays modified. With the opt-in, `:w` refuses while markers remain (rvim extension; with no opt-in there is no merge and `:w` is Vim's) |

The prompt is modal: only these keys act, others beep.

Per-hunk review:

- Baseline per document: the last saved text, kept in a scratch document
  `[base]`. It is modifiable, is never written to disk and is not a file in
  the buffer list. `:w` resets it to the buffer's text. `dp` copies the buffer's
  hunk into `[base]` (accept), so the baseline moves forward by one hunk. The
  buffer text does not change, so no undo step is added.
- Gutter: one column left of the line numbers, drawn only when the document
  differs from its baseline and agent keys are on. `+` added line, `~` changed
  line, `_` lines were deleted after this line. Style group `Changed` (new).
- Diff view (`:DiffSaved`, `:DiffGit`, or the conflict prompt's `d`): `[base]`
  is on one side, the buffer on the other, both with `:diffthis`. The keys are
  Vim's own (see Keys). `do` in the buffer copies `[base]`'s hunk into the
  buffer as one undo step tagged `user`. A `:DiffGit` result follows the
  document-version rule in the commands table.
- Before splits: `:DiffSaved` shows a unified diff in a read-only document.
  `do` and `dp` are not available; accept everything with `:w`, revert
  everything with `:e!`.
- Recent changes, `:AgentChanges`: the document's undo steps, newest first, last
  50, one line each: `#seq time source file +added -removed`. `<CR>` jumps to the
  first changed line of that step; it does not undo. Source tags: `user`,
  `agent:<id>`, `external`, `paste`, `prediction`.

Undo tags: every undo step records its source (existing rule), and the step
for a reload records the file's hash so `:AgentChanges` can show it.

### 5. Permission requests

A permission request is shown as a modal prompt on the message line:

```
claude-acp wants to edit src/a.rs (+3 -1). [y]es [a]lways [n]o [N]ever (1/2)
claude-acp wants to run: cargo test. [y]es [n]o (1/1)
```

- Only the options the agent sent are offered: `allow_once` = `y`,
  `allow_always` = `a`, `reject_once` = `n`, `reject_always` = `N` (the option
  kinds are UNVERIFIED until read from the protocol reference). For `a` and `N`
  the text names the scope: `[a]lways for edit in src/ this session`.
- `<Esc>` answers `reject_once`, or cancelled if no reject option was sent (wire
  shape UNVERIFIED).
- Queue: first in, first out across all agents; `(k/n)` counts the queue. Keys
  answer the head request; the prompt consumes the keys, so they do not reach
  Normal mode or prediction.
- rvim never answers a request by itself, and no timeout answers. After 5
  minutes the message line reads `claude-acp still waiting for y or n`, repeated
  every 5 minutes. The only answers rvim gives without a key are `cancelled`
  (on `\ac` or `:AgentCancel`, or on disconnect, when the agent is gone) and the
  session grants below, which the user created.
- `a` (`allow_always`) and `N` (`reject_always`) are the user's answer to this
  request, and they also answer later requests of the same tool kind and path
  scope (the directory of the file; the command for `execute`) in this session.
  Tool kinds are UNVERIFIED value names. Grants are kept in memory only: never
  written to disk, gone when the session ends. While any exist the status line
  reads `ai: claude (2 grants)`, and each use is logged as
  `grant used: <kind> <scope> allow|reject`. A request with no matching grant
  always prompts.
- `:AgentGrants` lists this session's grants; `:AgentForget` removes them for the
  project together with the approval; `:AgentForget!` removes them for every
  project.
- Requests from the IDE server (`openDiff`, below) are the same prompt, with
  `[y]es [n]o`, and get no grants.

### 6. Multiple agents and switching

- Each connected agent has its own state, its own agent log document, and its
  own entries in the permission queue (the prompt names the agent).
- The active agent is the first one that reaches Idle in the session, until the
  user runs `:AgentUse <name>` (id or display name; `<Tab>` completion once Ex
  completion exists). `:AgentUse` with no name lists the connected agents with
  their states. The choice lasts for the session; approvals are per agent, not
  per choice. `:AgentUse` lands with the connector (step 5 in section 11), since
  the consent and send flow needs it as soon as two agents are found.
- Writes are attributed per agent (source tag `agent:<id>`). Two agents editing
  one file need no special case: each write is tagged by its own intent, and the
  conflict rules apply as usual.

### 7. Claude Code: IDE server and terminal pane

Two ways to reach Claude Code, both with the approval rules above (agent id
`claude-ide` for the IDE server).

IDE server (Claude Code's own IDE integration, UNVERIFIED protocol details as
listed in "Detecting and connecting"):

- Starts only after `:AgentConnect claude-ide` is approved for the project.
- Binds `127.0.0.1` on an ephemeral port. The token is 128 random bits.
- Lock file: `<CLAUDE_CONFIG_DIR or ~/.claude>/ide/<port>.lock`, written
  atomically after the bind. Fields (all UNVERIFIED, from the third-party write-up
  cited in "Detecting and connecting"): `pid` (rvim's), `workspaceFolders` (the
  project root), `ideName` (`rvim`), `transport` (`ws`), `authToken`. Owner-only
  permissions on Unix; on Windows an ACL for the current user (UNVERIFIED how
  Claude Code reads it).
- Lock file lifecycle: created when the server starts; removed on clean quit,
  `:AgentDisconnect claude-ide`, and `:AgentForget`. At startup rvim removes a
  `<port>.lock` only if its `ideName` is `rvim` and its `pid` is not running.
  It never touches another tool's lock files.
- From an external terminal the user runs `/ide` in Claude Code and picks rvim
  (picker behaviour UNVERIFIED). Message on attach: `claude-ide: client attached`.
- Served calls (UNVERIFIED names, from the research above): current selection,
  open editors, open file, and `openFile` (open a shareable file at a line, no
  prompt). `openDiff` is a permission request (`[y]es [n]o`): `y` writes the
  proposal through the write-intent path, `n` rejects it. `getDiagnostics`
  returns an empty list until LSP (milestone 9).
- Transport: JSON-RPC over WebSocket (`tungstenite`, blocking). One thread per
  client; reads become events; no call from the editor waits on the socket.

Terminal pane:

- `:AgentTerminal [name]` runs the agent's CLI in a PTY (`portable-pty`, ConPTY on
  Windows). Before splits it takes the window, as `:terminal` does; with splits
  it opens a bottom split like the agent log.
- Gating: the agent needs an approval for this project, the same record as for
  a connection. If it is missing, the consent prompt comes first, with this text:
  `Run Claude Code in a terminal pane for C:/proj? It runs with your own
  permissions and edits the project under its own rules. [y]es [n]o`. `n` runs
  nothing.
- Environment for the child, and nothing else added by rvim: `RVIM_SOCKET`
  (only when the context endpoint is on); `ENABLE_IDE_INTEGRATION=true` and
  `CLAUDE_CODE_SSE_PORT=<port>` only when the `claude-ide` server is running for
  this project. The names are UNVERIFIED, from the research above. The child
  inherits the rest of rvim's environment; rvim adds no credentials.
- Writes made by the CLI in the terminal pane are `external` under the tagging
  rule (section 4).
- Terminal mode: keys go to the child. `<C-\><C-n>` returns to Normal mode in
  that window, and `i` or `a` in Normal mode returns to Terminal mode (Vim's
  `:terminal`). No other Vim key is handled in Terminal mode.
- When the child exits, the window shows `[exited <code>]` and stays until closed.
- Quitting rvim with a child it started still running asks:
  `claude still running. [y]es quit and stop it [n]o`. Children rvim did not start
  are never touched; the terminal pane only ever runs children it started.

### 8. Files-only fallback

Works with no agent installed. `:checktime`, `:DiffSaved`, `:DiffGit` and
`:AgentChanges` work with no opt-in at all. Reload of unmodified buffers follows
`autoread`: the opt-in sets it on for the session, and setting it by hand does
the same (the known difference in section 4). The conflict prompt needs the
opt-in. A user who runs `claude` in another terminal gets the same edit loop,
and its writes are tagged `external` (tagging rule, section 4).

How rvim says it is in this mode:

- Status line: `ai: files only` when the agent layer is active (opt-in, or an
  Agent command has run) and no agent is connected. If an agent is found but not
  connected, the Found state shows instead.
- First external change in a session, message: `files only: changes on disk are
  reloaded; no agent connected`.
- With no opt-in and `autoread` off (Vim's default): Vim's behaviour. A change is
  reported (`:checktime` gives Vim's warning) and nothing reloads until `:e!`.
  This is the differential case, and it is the one that must not change.

### 9. Prediction and agent coexistence

Prediction (milestone 8) is suppressed while the document is about to change
under it. Suppression is checked when a request would start and when a
suggestion would show:

| Condition for the document | Prediction |
|---|---|
| An agent turn is Working in this project | Off: no request, and any ghost text is dropped (not just hidden) |
| A permission request that touches this document is queued | Off |
| The conflict prompt is open for this document | Off |
| A reload is queued for this document | Off |

`Editor::prediction_allowed(doc)` returns this. It lands in milestone 7 step
5 as a function that returns false (prediction does not exist yet) and is
tested there, so milestone 8 only plugs in.

Keys that never conflict:

- Prediction accept (`<M-y>` by default; `<Tab>` only with the modern profile)
  acts only while ghost text is shown. Otherwise `<Tab>` is Vim's Insert `<Tab>`
  (unchanged).
- Agent keys are all `\`-prefixed or Normal-mode keys that prediction does not
  use. The permission and conflict prompts take their keys while open.
- Accepting a suggestion is an undo step tagged `prediction` (existing rule), so
  `:AgentChanges` shows it with its source.

### 10. Failures and privacy

Failures:

| Failure | How it is seen | Message (message line) | State and recovery |
|---|---|---|---|
| Agent process exits | stdout EOF, exit code | `claude-acp: exited 1; see :AgentLog` | Disconnected, retried 5 times, then Error |
| No reply to `initialize` | 10 s timeout | `claude-acp: no reply to initialize after 10s` | Error; `:AgentConnect` |
| Not logged in | `authenticate` error or auth-required | `claude-acp: not logged in. <hint from the registry>` (hint text UNVERIFIED) | Error; `:AgentConnect` after login |
| Request failed during a turn (network, service) | JSON-RPC error on the prompt | `claude-acp: request failed: <agent's message>` | Idle; the prompt can be sent again |
| Slow turn | none; no timeout on a turn | `ai: claude working 42s` | Working; `\ac` |
| Malformed JSON from the agent | parse error | line logged and skipped; 10 in a row: disconnect | Disconnected |
| Agent write fails (`fs/write_text_file`) | IO error | JSON-RPC error to the agent; message `write failed: <reason>` | unchanged |
| rvim panics | panic hook | recovery files (existing) | agent processes stop when their pipes close |

Slow agent: every call to an agent runs on the connector's worker. The UI never
waits for `initialize`, `session/prompt` or a read. The Working state is the
only sign of a slow turn, with elapsed seconds.

Child processes die with rvim where the platform allows it: a Windows job object
with kill-on-close; on Unix the process group, plus closing the pipes (macOS
behaviour UNVERIFIED).

Logs: `$XDG_STATE_HOME/rvim/agent/<id>.log` (Windows `%LOCALAPPDATA%\rvim\agent\`).
With `log = "events"` (default) it holds lifecycle events, method names, exit
codes and errors: no prompt text, no file contents, no replies. With
`log = "transcript"` it also holds the JSON-RPC lines with tokens redacted; this
is off by default. Rotation at 1 MiB, three files kept. Nothing is sent off the
machine by rvim. The agent's own process may use the network under its own
policy, which the consent text says.

Shareable directories: the context and the served file calls only touch paths
inside `shareable` (default: the project root). A path outside it is left out of
the context, and an `fs/read_text_file` or `fs/write_text_file` for it gets a
JSON-RPC error (`path outside shareable directories`). The same exclusions as
above (`.env*` etc.) apply.

### 11. Implementation order and tests

Each step lands with `RVIM_REQUIRE_VIM=1 cargo test`, clippy and fmt clean.
New modules keep the terminal out: `agent/state.rs` (pure), `agent/registry.rs`
and `agent/detect.rs`, `agent/approval.rs`, `agent/acp.rs` (connector),
`agent/ide.rs` (IDE server), `filewatch.rs` (worker), `reload.rs` (line diff,
cursor and scroll mapping, undo step), `review.rs` (baseline, gutter, diff
view model). `main.rs` only wires threads and draws.

This order refines "Suggested order" in the Status section. Only step 1 is a
prerequisite for the file reload (steps 2 and 3), and the connector (step 5)
needs no splits. Splits gate step 7 alone.

| Step | Lands | Depends on | Tests |
|---|---|---|---|
| 1 | Document/Window split (milestone 4, item (a)) | none | Existing compat suite unchanged. Unit: two windows on one document share undo and marks, keep their own cursors |
| 2 | Undo step source and hash, file watcher (worker), `checktime`, `autoread`, reload as one undo step with cursor and scroll mapping, echo suppression | 1 (per-document version) | Unit: mapping table (equal, inserted above, deleted under cursor, replaced, deleted top line); `u` and `<C-r>` after reload. Scripted watcher with fake clock and fake file contents. Echo: a `:w` does not reload itself. Differential: `autoread` value and `:checktime` with no change. Add the cursor-placement difference to "Known differences" |
| 3 | Conflict prompt (`l k d m`) with the opt-in, baseline (`[base]` scratch) and gutter, `:w` W11/W12 guard, deleted file, `:AgentChanges`, merge with markers (marker refusal of `:w` with the opt-in only) | 2 | Unit: each conflict key on a fixed state; merge (clean, conflicting, identical); Frame golden of the gutter and of the prompt line. With no opt-in: a modified buffer is not reloaded and `:w` gives W11. No differential (messages) |
| 4 | Opt-in config table, registry, detection, `:AgentScan`, status-line states without connection | minimal config loader (shared with milestone 6), options | Detection with a temporary fake home and `PATH`: found, not found, found under the config dir only. Frame golden for each indicator. Golden with no opt-in and no Agent command: no indicator, identical to Vim's frame. Golden after `:AgentScan` with no opt-in: indicator shown |
| 5 | ACP connector: consent, approval store, `:Agent`, `:AgentConnect`, `:AgentUse`, `:AgentCancel`, `:AgentDisconnect`, agent log document, `\a` and `\al`, lifecycle and backoff, `Editor::prediction_allowed` (returns false) | 4 | Fake agent (test helper binary) replays recorded JSON-RPC transcripts both ways: success, auth failure, reply timeout, EOF mid-turn, malformed line. Pure transition table test. Fake timer for backoff. Approval store round trip and unreadable file. Consent keys. `:AgentUse` with two fake agents: listing, switching, and the choice prompt when several are found and none is active. Cases: `:Agent` without opt-in still works, keys stay unbound |
| 6 | Permissions and served file calls: queue, prompt keys, session grants, `:AgentForget`, `:AgentGrants`, write intent and the tagging rule, `AgentWritten`, `fs/*` limited to shareable paths | 5, 3 | Transcripts with a permission queue of two; never-answered-without-key test (no reply sent until a key); a session grant (`a`) answers the next matching request of that kind and scope only, and nothing is written to `agent-approvals.toml`; forget removes it; cancel answers as cancelled; a served write is applied once, tagged `agent:<id>`, and the watcher's later event for the same hash is ignored; `fs/read_text_file` returns unsaved text; a path outside shareable gets an error |
| 7 | Splits-dependent UI: agent log split, side-by-side diff view, `:AgentChanges` jump, terminal pane (PTY) with its gating, `<C-\><C-n>` routing | milestone 5 splits, buffer list, PTY | Frame goldens with a split, status and agent log. Differential, scratch buffers with `:diffthis`: `]c [c do dp` on the same text in Vim and rvim (UNVERIFIED that the harness can drive two diff buffers; check first). Desired column, as CLAUDE.md requires: `lllljXj`-style cases for `]c`, `[c`, `dp`, `do` and the `<CR>` jump of `:AgentChanges`, each stating the column the command leaves and the column after a following `j`; expected values from Vim where the harness can express the case, otherwise from the CLAUDE.md rule and marked so. Terminal: key routing test with a fake child that echoes; no child is spawned without approval |
| 8 | Claude Code IDE server (`claude-ide`), context endpoint `RVIM_SOCKET`, `openDiff` as permission | 5, 6 | Lock file lifecycle in a temp `CLAUDE_CONFIG_DIR`: created, stale removed, another tool's file kept, removed on quit. WebSocket transcript replay with a fake client; `openDiff` answered only by a key. Manual checklist with a real Claude Code (`/ide`, at-mention, `openDiff`); not part of `cargo test` |

Step 1 is the only hard prerequisite for everything. Step 7 is the only one
that needs splits and a PTY, so steps 2 to 6 and 8 work before splits, with
the message-line and unified fallbacks above. Every Vim-counterpart behaviour
gets a differential case where the harness can express it (diff-mode keys on
scratch buffers, `:checktime` with no change, the option values). File changes
cannot be written by the harness between keys, so reload is tested in unit and
scripted tests, and the Vim placement rule is probed by hand.

## Prediction (milestone 8)

Inline suggestions of the code the user is about to write, shown as dimmed
ghost text after the cursor (further lines below it), accepted with a key.

- **Pipeline**: an Insert-mode edit starts a debounce timer (about 150 ms).
  When it fires, a snapshot (text before and after the cursor, path,
  filetype, a few related documents) goes to a provider on a worker thread.
  The result returns as `Prediction(DocId, version, text)` and is shown only
  if the document version and cursor still match; otherwise it is dropped. A
  new keystroke cancels the request in flight.
- **Providers**, pluggable behind one trait and config: (1) an external
  command that reads JSON on stdin and writes the completion on stdout (any
  local or hosted model), (2) an HTTP endpoint (OpenAI-compatible
  completions, fill-in-the-middle servers), (3) LSP inline completion once the
  LSP client exists. rvim ships no model and sends nothing until a provider is
  configured.
- **Privacy**: off by default. Allow-list of directories and file patterns
  per provider; skip files matching `.gitignore`, `.env*` and key files, and
  any buffer with `set noprediction`. A visible indicator while a provider is
  active or a request is in flight.
- **Rendering**: ghost text is a styled overlay in the frame (the
  `Suggestion` group), not buffer text. It does not move the real cursor and
  has no effect on undo, registers, the ruler or motions until accepted.
- **Accepting**: accept all, accept word, accept line, dismiss (`<Esc>` or any
  non-matching key). With the modern profile off, the accept key is one Vim
  leaves free (`<C-y>` is taken in Insert mode, so use `<M-y>` or a
  configured key); `<Tab>` only with the modern profile. Accepting inserts as
  one undo step tagged `prediction`, and `.` repeats it as typed text.
- **Typing through**: if the user types the characters the suggestion starts
  with, the ghost text shrinks instead of being requested again.
- **Local statistics**: counts of accepted and rejected suggestions are kept
  on disk locally (never uploaded) and shown by `:PredictStats`, so providers
  and debounce can be tuned.

## Neovim-inspired features (milestone 9 and later)

Neovim is the reference for the features in this section, not for Vim's
behaviour. The rules:

- **Indirect copying.** Each feature is described from Neovim's public
  documentation: its `:help` pages, its API reference, and the protocol
  specifications it links. The version used is recorded when the feature is
  built (which Neovim version is current is UNVERIFIED). rvim's code is its
  own Rust code. Neovim source (C or Lua) is not copied. If a Neovim file is
  ever reused verbatim, the license check in "Licensing" comes first and the
  file's license header is kept.
- **Own tests.** Tests are written for rvim from the documented behaviour.
  Neovim's test suite is not copied.
- **Names.** Where rvim uses a Neovim name, such as
  `vim.api.nvim_buf_get_lines`, the name is for compatibility (see "Neovim
  plugin compatibility"); it is not copied text. Neovim's name and logo are
  its own marks, so rvim's docs must not suggest that Neovim endorses rvim
  (trademark view UNVERIFIED).
- **Vim contract.** Each feature is an extension in the sense of "Extensions
  and the Vim contract": off unless configured, and it never changes a Vim key
  sequence while it is off.

| Feature | Milestone | Needs the Document/Window split |
|---|---|---|
| LSP client and diagnostics | 9 | Yes: diagnostics belong to a Document and carry its version |
| Tree-sitter highlighting and text objects | 6 (highlighting), 9 (text objects) | Yes: parse results belong to a Document version |
| Lua scripting | 9 | Partly: Lua reads Documents and Windows through the editor |
| RPC server, `--server`, `--remote` | 9 | Yes: remote commands name windows and documents |
| Floating windows and popup menus | 6 | Yes: a Window needs a layout rectangle and a kind |
| Built-in terminal (`:terminal`) | 7 | Yes: a terminal is a Window whose Document is a screen |
| Async jobs | 9 | Yes: job output goes to a scratch Document |
| Extmarks and virtual text | 5 (core), 7 to 9 (users) | Yes: extmarks belong to a Document |
| `vim.ui` input and select hooks | 9 | Partly: a prompt is Window-level UI |
| Tree-sitter folding | 9, after Vim folds | Yes: Vim folds are window-local |
| `:Health` report | 6 | No |
| XDG paths | 6 | No |
| Default modern options | 6 | Yes: buffer-local and window-local options |

Per feature:

- **Built-in LSP client and diagnostics.** A client that starts a language
  server from a command in the config (approved per project, like agents),
  speaks JSON-RPC over stdio, and sends open, change and close notifications
  with document versions. Diagnostics are stored on the Document with the
  version they were computed for, drawn in a sign column and as underlines,
  and listed by `:Diagnostics` (new Ex command). The protocol is taken from the
  Language Server Protocol specification at a pinned version (which version is
  current is UNVERIFIED). Inline completion stays deferred (see "Dependency
  choices").
- **Tree-sitter highlighting and text objects.** Highlighting is milestone 6
  (see "Editing experience"). Text objects selected by syntax node (function,
  class, argument) are a milestone 9 extension with rvim's own key choice,
  not Neovim's keys (Neovim's own text object support is in its plugins, not
  in core; UNVERIFIED). Parsing runs on a worker per document version.
- **Lua scripting.** See "Lua extensions".
- **RPC server, `--server` and `--remote`.** `rvim --listen <socket>` starts
  a server; `rvim --server <socket> --remote <files>` sends `:edit` commands
  to a running instance (flag names follow Neovim's `:help` as checked, which
  is UNVERIFIED). The wire format is JSON-RPC, the same family as the agent
  endpoint in `docs/agent-protocol.md`, not msgpack-RPC, so no msgpack
  dependency is needed. The socket is user-only (mode 0600 or a per-user pipe
  ACL). This is a separate opt-in endpoint from the agent endpoint, and
  nothing listens by default.
- **Floating windows and popup menus.** A floating window is a Window with its
  own position and size, drawn over the layout in a fixed z-order. A popup
  menu is a floating window anchored to the cursor. They serve Ex completion
  and the command palette (milestone 6). The Frame needs layered cells, so
  the `view.rs` layout stays a pure function.
- **Built-in terminal.** The terminal pane of milestone 7 is the PTY window
  type. This item adds Vim's `:terminal` command name and its Terminal-mode
  exit `<C-\><C-n>`, as Vim documents them (`terminal.txt`; checked against a
  Vim built with the terminal feature, UNVERIFIED on the reference Vim).
- **Async jobs.** Start a child process, stream its stdout and stderr lines as
  events, write to its stdin, and stop it. Exposed to Lua and as the Ex
  command `:Job` (new). Runs on worker threads; each event carries the job id.
  Needs the `process` capability (see "Lua extensions").
- **Extmarks and virtual text.** Positions that move with edits and carry
  data: highlight, virtual text and sign. The core (adjusting positions on
  line changes) reuses the lowercase-mark code in `buffer.rs`, so it fits
  milestone 5. Ghost text (milestone 8), diagnostics and hunk markers
  (milestone 7) use it. Undo restores extmarks as it restores marks.
- **`vim.ui` input and select hooks.** `vim.ui.input` and `vim.ui.select`
  (function names and signatures from Neovim's documentation, UNVERIFIED)
  give plugins and agent prompts one way to ask the user: on the command line
  for input, and in a popup menu for a choice. The callback runs on the Lua
  thread with the answer as an event.
- **Tree-sitter folding.** Vim's fold commands (`zf`, `zo`, `zc`, `za`, `zR`,
  `zM`, `foldmethod`) are not implemented yet, and the plan has no milestone
  for them. Before a tree-sitter fold method is added, the Vim fold model
  needs its own milestone and differential cases, because Vim has the
  behaviour. Folds are window-local in Vim, so this depends on the Window type.
- **`:Health`.** A scratch buffer listing checks: terminal capabilities
  (colour, bracketed paste, kitty keyboard), config file errors, configured
  and detected agents (existence checks only), recovery files left by a
  crash, and `.vim` files in plugin packages that are skipped. Read-only.
- **XDG paths.** Config in `$XDG_CONFIG_HOME/rvim` (default
  `~/.config/rvim`), state in `$XDG_STATE_HOME/rvim`, data in
  `$XDG_DATA_HOME/rvim` and cache in `$XDG_CACHE_HOME`, following the XDG
  Base Directory specification. Windows locations (`%APPDATA%`,
  `%LOCALAPPDATA%`) are UNVERIFIED and are checked against Neovim's
  documented layout before use. Recovery files already use the state path.
- **Default modern options.** The built-in modern profile (see "Editing
  experience", Defaults) is a table of values in `options.rs`, applied only
  under the rules there. Options that are buffer-local or window-local in
  Vim keep their scope.

## Lua extensions (milestone 9)

Lua is the scripting language for plugins and user logic, as Neovim uses Lua
for its `init.lua` and its plugins. Lua handles logic. `config.toml` keeps
data (see "Config" under "Editing experience").

- **Embedding.** The `mlua` crate (version UNVERIFIED), with Lua 5.4 built
  from source through its `vendored` feature (feature names UNVERIFIED).
  LuaJIT is the alternative: it is faster, but its platform coverage differs
  (UNVERIFIED). Lua 5.4 is the first choice as the plain reference
  implementation. Both need a C compiler, which tree-sitter already needs.
  Binary size, build time and run-time cost are UNVERIFIED and are measured
  before the choice is recorded in "Dependency choices". The licenses of Lua
  and mlua are MIT in their repositories (UNVERIFIED here; check the license
  file of the vendored source).
- **Threads.** The Lua state lives on one worker thread. `Editor` never calls
  Lua and never waits for it. Callbacks (keymaps, autocmds, user commands,
  timers, job output) arrive as events, and results return as events. API
  calls that read editor state (for example `nvim_buf_get_lines`) send a
  request to the editor and wait for its reply on the Lua thread; the UI
  thread never waits on Lua. Writes are applied on the UI thread as one undo
  step tagged `plugin:<name>`, the same source-tagging rule as external
  reloads and predictions.
- **Time slices.** Each callback runs with an instruction-count hook and a
  wall-clock budget (a default such as 100 ms is UNVERIFIED and will be
  measured). A callback over budget is aborted with a message. A plugin that
  goes over budget three times in one session is disabled until restart
  (the counts are proposals, UNVERIFIED).
- **Off by default.** No Lua state is created unless `[lua] enable = true` is
  set in `config.toml` or `--lua` is given. The modern profile does not turn
  Lua on by itself.
- **Where code loads from**, in this order, when Lua is on:
  - `$XDG_CONFIG_HOME/rvim/init.lua` (default `~/.config/rvim/init.lua`).
  - `$XDG_CONFIG_HOME/rvim/lua/`, added to `package.path` for `require`.
  - `pack/*/start/*` under `$XDG_CONFIG_HOME/rvim` and under
    `$XDG_DATA_HOME/rvim/site`: the Lua files in each package's `plugin/`
    directory load at start. `opt/` packages load with `:packadd`, which is
    Vim's command name (implementation planned with Lua). `.vim` files in a
    package are skipped and listed by `:Health`, because rvim runs no
    Vimscript before the milestone 9 subset.
- **Permissions.** Capabilities are `fs.read`, `fs.write`, `process` (start
  child processes) and `network`. A plugin has none unless the config approves
  it, for example `[lua.plugins."name"] allow = ["fs.read"]`. Approval is tied
  to the plugin's name and a hash of its content, so an updated plugin needs
  approval again. A call outside the approved set raises a Lua error at the
  API boundary. This is a policy in rvim's API, not an operating-system
  sandbox: an approved plugin runs with the user's privileges, and the user is
  the trust boundary. Unapproved plugins do not get `io.popen`, `os.execute`
  or `package.loadlib`. Lua's shared state makes full isolation hard, so the
  exact mechanism is prototyped before it is fixed (UNVERIFIED design).
- **API namespaces.**
  - `rvim`: rvim's own API (`rvim.api_version`, `rvim.version`, and a
    read-only `rvim.config`).
  - `vim`: the Neovim-compatible surface described in "Neovim plugin
    compatibility", stage 1. This covers `vim.api`, `vim.fn` (a subset,
    implemented in Rust under Vim's function names), `vim.keymap.set`,
    `vim.opt` (a typed wrapper over the options registry), `vim.g`, `vim.b`
    and `vim.w` (variables), `vim.cmd` (runs an Ex command string through the
    same path as typed `:` input and returns its error), `vim.schedule` (runs
    on the UI thread after the current event), `vim.notify`, and autocmds for
    the events rvim emits (listed in the API reference).
- **Versioning.** The API has an integer version, `rvim.api_version`, starting
  at 1 with the first release that contains Lua. Within one version, changes
  are additions only. A removal or a change of meaning waits for the next
  version, and the old form warns for one release first. The reference is
  `docs/lua-api.md` (planned; not created).
- **Agent endpoint.** Lua has no socket and no direct agent access. Any
  `rvim.agent.*` function goes through the same approval and project allow-list
  as the agent endpoint (see "Agent integration"). A plugin cannot approve an
  agent or widen its allow-list.
- **Config and Lua.** `config.toml` holds data: options, mappings that need no
  code, the theme, the start menu switch, the Lua enable flag and plugin
  approvals, agent registry additions, and prediction providers. It is read
  before `init.lua`. Lua may set options and mappings in code; both go to the
  same registry, and `:set` shows the final value.
- **Tests.** Lua behaviour is tested in `cargo test` by driving a Lua state
  with `Editor::handle_event` and asserting the document and messages. The
  differential harness does not cover Lua: Vim's `:lua` interface is a
  different API (UNVERIFIED details), so Neovim's documented behaviour is the
  reference, as for the other features in this section.

## Neovim plugin compatibility (milestone 9)

The goal is to run plugins written for Neovim's Lua API unchanged, for the
part of the API that the staged implementation covers.

| Stage | Scope | Exit criterion |
|---|---|---|
| 0 | rvim's own API only (see "Lua extensions") | `init.lua`, keymaps and autocmds work; no compatibility claim |
| 1 | a `vim.api` subset for small plugins (groups below) | the stage 1 plugins in `docs/compat/plugins.md` pass their smoke scripts |
| 2 | unmodified pure-Lua plugins from the compatibility list, run in CI | each listed plugin passes, or has a recorded reason to fail |

Stage 1 API groups (the names are to be checked against Neovim's current API
reference at the version chosen; the list is UNVERIFIED):

- **buffer**: get and set lines, line count, name, current buffer, buffer
  options.
- **window**: current window, get and set cursor, the window's buffer.
- **option**: `nvim_get_option_value` and `nvim_set_option_value`, mapped onto
  the options registry.
- **keymap**: `nvim_set_keymap`, `nvim_del_keymap` and `vim.keymap.set`
  (including its buffer-local form).
- **autocmd**: `nvim_create_autocmd`, `nvim_create_augroup`,
  `nvim_del_autocmd`, for the events rvim emits.
- **extmark**: `nvim_create_namespace`, `nvim_buf_set_extmark`,
  `nvim_buf_del_extmark`. Only after the extmark item in "Neovim-inspired
  features" lands.
- **user command**: `nvim_create_user_command`. A plugin's command name must
  start with a capital letter, which is Vim's rule for user commands
  (UNVERIFIED wording), so it cannot shadow a Vim command.

Stage 2 adds what a plugin may need from elsewhere, each only once rvim has
it: `vim.uv` (file and process I/O, behind the capabilities), `vim.lsp`,
`vim.treesitter`, `vim.ui` and floating windows. A plugin that needs a missing
feature is listed as failing with that reason. It is not skipped silently.

Non-goals for the first versions:

- Full API parity with Neovim, including every function and option.
- Remote plugin hosts in other languages (Neovim's Python, Node and Ruby
  providers, and msgpack remote plugins; names UNVERIFIED).
- Running Vimscript (`.vim`) plugins. A Vimscript subset is a separate
  milestone 9 item.
- Neovim's external UI protocol, and exact redraw timing.
- Neovim internals, including its runtime files and Lua modules that call
  internal C functions.

Compatibility test approach:

1. `docs/compat/plugins.md` (planned; not created) holds one row per plugin:
   name, repository URL, pinned commit, license (checked, not assumed), stage,
   status (`pass`, `fail` or `not tested`), missing API calls, and the date
   checked.
2. Each plugin gets a smoke script written by rvim's maintainers, not the
   plugin's own test suite. The script drives rvim headless (`Editor` with
   `handle_event` and no terminal) with keys, and asserts the buffer, cursor
   and messages after a documented use of the plugin.
3. A CI job, not part of `cargo test`, clones the pinned commits into a
   temporary directory and runs the scripts. It needs network access, so it
   runs in CI only, when the list changes or on a schedule.
4. A failing plugin stays in the table with `fail` and its reason. It is not
   removed to make the table green.
5. The first list is chosen by the owner. Candidates must be pure Lua, under
   a permissive license (checked), with a small API use. plenary.nvim is a
   candidate for its utility functions (UNVERIFIED that it is still pure Lua
   and maintained).

Every statement about Neovim's API in this section is UNVERIFIED until it is
checked against Neovim's current API reference at the pinned version.

## Dependency choices

Looked up on 2026-10-31 (versions, licences and dates from crates.io; items
marked UNVERIFIED were not checked in docs). All are MIT, Apache-2.0, 0BSD or
CC0, compatible with rvim's licence. The design is threads plus an `mpsc`
channel with no async runtime; every item below works that way, except
possibly the ACP crate (check).

| Need | Choice | Notes |
|---|---|---|
| Line diff, external-change undo step, diff view | `similar` 3.2.1 | Pure Rust, Myers/Patience. Its ops map to range edits. Three-way merge: write a small diff3 on top (or evaluate `diffy`, UNVERIFIED) |
| File changes | mtime/size polling | `notify` later, optional |
| Local IPC | `interprocess` 2.4.7 (`local_socket`) | One API for Unix socket and Windows named pipe; std alone has no named pipes. Set permissions, handle stale socket files |
| WebSocket (Claude Code IDE server) | `tungstenite` 0.30.0, blocking | One thread per client; bind 127.0.0.1 only and require a token |
| Terminal input | `crossterm` 0.29 | Bracketed paste, focus events and kitty flags are in the API; always feature-detect (`supports_keyboard_enhancement`). Whether 0.29 delivers paste, focus and kitty events on Windows (conhost vs Windows Terminal) is UNVERIFIED: test both and keep fallbacks. Filter `KeyEventKind::Release` |
| Syntax highlighting | `tree-sitter` 0.27 with per-language `tree-sitter-<lang>` crates | Needs a C compiler (also in CI); each grammar adds roughly 0.3-3 MB (UNVERIFIED), so bundle a small set (about 10) and keep a regex highlighter as fallback. `tree-sitter-language-pack` bundles 372 languages; too large, loading model UNVERIFIED |
| Terminal pane | `portable-pty` 0.9 plus `vt100` 0.18 | ConPTY on Windows 10 1809+. `portable-pty` has had no release since February 2025; expect ConPTY quirks |
| Prediction providers | blocking HTTP (`ureq`, version UNVERIFIED) with small `serde_json` request types per provider | FIM endpoints (Ollama `/api/generate` with `suffix`, llama.cpp `/infill`, Codestral `/v1/fim/completions`, OpenAI-style `/v1/completions` with `suffix`) are UNVERIFIED. LSP 3.18 `textDocument/inlineCompletion` is preview and `lsp-types` 0.97 (2024) lacks it; async LSP stacks need tokio. Defer LSP inline completion |
| ACP | `agent-client-protocol` 3.6.0 | See above; footprint unchecked |
| Lua (milestone 9) | `mlua` (version UNVERIFIED) with Lua 5.4 `vendored` (UNVERIFIED); LuaJIT is the alternative | See "Lua extensions". Lua runs on one worker thread, so the `send` feature (UNVERIFIED) is not needed; footprint and build time to be measured |
| LSP (milestone 9) | JSON-RPC framing written in rvim, sharing code with the agent connectors; `lsp-types` optional (version UNVERIFIED) | See "Neovim-inspired features". Blocking I/O on a worker, as for agents |

Waking the UI from a worker: have workers send into the one event channel
and let the input thread, not a poll timeout, own the wait (a small
self-wake mechanism is needed since `crossterm::event::poll` only waits on
terminal input). Cancelling a blocking HTTP request is awkward, so every
request carries a generation number and stale results are discarded.

## Extensions and the Vim contract

- Everything Vim does stays covered by differential tests. With no config
  file and no flag, rvim is `vim -N -u NONE`: no ghost text, no extra
  mappings, no changed defaults, no keys taken.
- Extensions never change what a Vim key sequence does while the extension is
  off. New Normal-mode keys use ones Vim leaves free (`\` as leader, `<C-\>`;
  `<Space>` is Vim's `l`, so it is a leader only with the modern profile), and
  new Ex commands start with a capital letter (`:Agent`,
  `:PredictStats`), which Vim reserves for users.
- Features beyond Vim (agent endpoint, prediction, reload as an undo step) are
  tested with unit and integration tests: the `Editor` is driven with events
  (`handle_event`) and the resulting document, undo tree and frame are
  asserted. Anything with a Vim counterpart (`autoread`, `:diffthis`, `]c`,
  `:set` semantics, paste in Insert mode) also gets differential cases.
- Rendering is tested on the `Frame` (cells and styles), with golden files for
  a few screens, not through a real terminal.
- The start menu (milestone 6) is on only when a config file exists, `--modern`,
  `--start-menu` or `start.menu = true` turns it on. While it is shown it takes
  `<Up>`, `<Down>` and `<CR>`, and `<Esc>` closes it. Every other key is
  handled by Vim as usual (see "Start menu").
- Lua (milestone 9) is off until `[lua] enable = true` or `--lua`. Neovim-style
  features are off until configured, as the other extensions are.

## Differential testing

`cargo test` runs `tests/compat_*.rs`. Each case is (text, keys). All cases of
a test run in one `vim -N -u NONE -es` process, fed with `feedkeys(keys,
'tx')` so they behave as typed. Lines and the cursor (`line('.')`,
`charcol('.')`) are compared with rvim. Set `RVIM_REFERENCE_VIM` to choose the
Vim binary and `RVIM_REQUIRE_VIM=1` to fail instead of skip when it is
missing. Add a case for every behaviour you implement, and when Vim and rvim
differ, Vim is right.

Things the harness cannot see: screen layout, messages, and file I/O. Those
have unit tests (for layout, on the styled `Frame` once milestone 4 lands).

## Licensing

Status: implemented on 2026-11-01, after the owner approved the scheme that
day. This is not legal advice: no lawyer has reviewed the scheme, the Vim
license, or Apache-2.0 compatibility. Counsel review is still open (see "Open
questions"). What is in effect:

- `LICENSE` is the Apache-2.0 text, verbatim. `LICENSE-VIM` is the old Vim
  license text, kept as LICENSE-VIM (verbatim copy of the previous LICENSE).
- `NOTICE` lists every Vim-licensed file and the Vim attribution.
- Every `.rs` file under `src/`, `tests/` and `examples/` has an SPDX line
  first. `tests/license_headers.rs` fails if one is missing (no Vim needed).
- `Cargo.toml` has `license = "Apache-2.0 AND Vim"`. Cargo accepted the
  expression for `cargo build` and `cargo package --list`. Whether crates.io
  and the AUR accept the identifier `Vim` is still UNVERIFIED, so `license-file`
  is the fallback if they do not.
- The README's License section explains the two licenses.
- The `VERSION_MESSAGE` notice in `src/lib.rs` is unchanged (condition II.3).

**Per-file result** (classified 2026-11-01 from the header comments and the
Vim C names they cite; Vim = `SPDX-License-Identifier: Vim`, the rest
Apache-2.0):

| Area | Files | Class |
|---|---|---|
| Word motions, findmatchlimit, utf_class | `src/motion.rs` | Vim |
| Text objects | `src/textobject.rs` | Vim |
| Operators, registers as ported from ops.c | `src/ops.rs` | Vim |
| Normal and Visual commands (`nv_*`) | `src/normal.rs` | Vim |
| Search (`searchit`, `do_search`, `current_search`) | `src/search.rs` | Vim |
| Ex commands (`:s` loops are a port of `do_sub`) | `src/ex.rs` | Vim (mixed) |
| Jumplist and context mark | `src/jumplist.rs` | Vim |
| Registers (`register.c`) | `src/register.rs` | Vim (conservative) |
| Undo tree (`u_undoredo` model) | `src/undo.rs` | Vim (conservative) |
| Command-line history (`histadd`) | `src/cmdline.rs` | Vim (conservative) |
| Editor state, `u_sync`, `open_line`, `start_arrow` | `src/editor.rs` | Vim (conservative) |
| Regex (Vim's backtracking semantics) | `src/regex.rs` | Vim (conservative) |
| Scrolling (`halfpage`, `nv_scroll` logic) | `src/scroll.rs` | Vim (conservative) |
| Lowercase marks (`mark_adjust` model) | `src/buffer.rs` | Vim (conservative) |
| Event loop, keys, options, macros, recovery, text widths, view, lib, main | `src/event.rs`, `src/key.rs`, `src/options.rs`, `src/macros.rs`, `src/recovery.rs`, `src/text.rs`, `src/view.rs`, `src/lib.rs`, `src/main.rs` | Apache-2.0 |
| Tests (rvim's own differential cases and harness) | `tests/*.rs`, `tests/common/mod.rs`, `tests/license_headers.rs` | Apache-2.0 |
| Example | `examples/try.rs` | Apache-2.0 |

"Conservative" means the file's comments name a Vim function or model but the
owner has not checked it line by line against Vim's source. These seven files
need owner review, and they stay Vim until it is done. Their status is not an
assertion that each line is a copy of Vim's code.

**Contributor rule** (in `CONTRIBUTING.md`): a contribution
to an Apache-2.0 file is Apache-2.0. A contribution to a Vim-licensed file
stays under the Vim license. Porting Vim code is declared in the commit message
and the PR, and the new file gets the Vim header.

**Previous state** (before 2026-11-01, kept for the record): `LICENSE` was the
full Vim license text, `Cargo.toml` had `license = "Vim"`, and no file under
`src/` had an SPDX identifier or a copyright line.

**Goal.** Apache-2.0 for rvim's new code. The Vim license for parts derived
from Vim: ports of Vim's C algorithms (the word motions, `searchit`, regex
semantics, `do_sub`, and possibly scrolling; see the table). This is meant to
resemble Neovim's arrangement (UNVERIFIED: check Neovim's `LICENSE` text and
its file headers before the arrangement is copied).

**Basis in the Vim license.** Section II.2.c lets a modified Vim choose its own
license for the new code it adds, if the changes do not restrict others from
making their own changes to the official Vim. Section II.1 keeps the license
text unmodified, II.3 keeps the modified-version message in `:version` and the
intro screen, and II.4 keeps the contact information. Whether Apache-2.0 meets
the "does not restrict" test is for counsel (UNVERIFIED).

**Vim-derived code, as the plan and the code comments state it.** Found by
grepping `src/` for "port", "ported" and "Port of" and reading the module
headers. The classification is a proposal to be checked file by file against
Vim's source.

| Module | What the code says | Class |
|---|---|---|
| `motion.rs` | Word motions are ports of `fwd_word`, `bck_word`, `end_word`; `findmatchlimit()` is a port; the `utf_class` table is from `mbyte.c` | Derived (port) |
| `textobject.rs` | "ported from Vim's textobject.c" | Derived (port) |
| `ops.rs` | "ported from Vim's ops.c and register.c" | Derived (port) |
| `normal.rs` | "ports of Vim's normal.c `nv_*` functions" | Derived (port) |
| `search.rs` | "a port of Vim's `searchit()` and `do_search()`" | Derived (port) |
| `ex.rs` | The `:s` loops are "a port of the loops of Vim's `do_sub()`"; `do_move` and `ex_mark` follow Vim | Derived for the `:s` part; the rest to check |
| `register.rs` | "following Vim's register.c" | To check: port or behaviour |
| `undo.rs` | Vim's `u_undoredo()` model and `uh_namedm` | To check |
| `cmdline.rs` | Vim's `histadd` | To check (small) |
| `editor.rs` | Many "Vim's X()" comments (`u_sync`, `start_arrow`, `open_line`) | To check: mostly structure |
| `regex.rs` | "Semantics follow Vim's backtracking engine" | To check: semantics only, or code |
| `scroll.rs` | Rules from `scroll.txt`; its header says Vim's C was not ported | Likely new code, by its own header |
| `text.rs`, `view.rs`, `buffer.rs`, `options.rs`, `key.rs`, `event.rs`, `macros.rs`, `recovery.rs`, `main.rs` | rvim's own design; Vim behaviour is checked only by differential tests | New |

The tests in `tests/` are rvim's own cases, run against Vim. They are not
copies of Vim's test suite (`src/testdir`), and must stay that way.

**Scheme.**

- **Per-file SPDX header**, first line after any shebang:
  - `// SPDX-License-Identifier: Apache-2.0` for new files.
  - `// SPDX-License-Identifier: Vim` for Vim-derived files. These also carry
    `// Derived from Vim (see LICENSE-VIM). This is a modified version, not
    the official Vim.` (the II.3 notice).
  - A copyright line in both: `// Copyright (c) <year> <holder>` (holder not
    chosen; see open questions). The identifier `Vim` is expected to be on the
    SPDX list (UNVERIFIED).
- **Mixed files.** A file that contains any ported code is Vim-licensed as a
  whole. New code that does not depend on the port goes into a new Apache-2.0
  module, so that the file can move out. For example, the `:s` port could move
  to its own module, which would let most of `ex.rs` become Apache-2.0. Whether
  to split is the owner's decision.
- **Files.**
  - `LICENSE`: Apache-2.0 text, for new code. It replaces the Vim text only
    when the scheme is adopted.
  - `LICENSE-VIM`: the current Vim license text, byte for byte, kept as
    LICENSE-VIM (verbatim copy of the previous LICENSE).
  - `NOTICE`: Apache-2.0 attribution, the Vim attribution (Bram Moolenaar and
    the Vim contributors) and the modified-version statement.
  - README License section: explains both files and which files are which.
  - `Cargo.toml` `license`: an SPDX expression. How Cargo and crates.io treat
    `Apache-2.0 AND Vim` is UNVERIFIED; `license-file` is the fallback.
  - `.github/workflows/release.yml` copies `LICENSE*` into each archive, so
    `LICENSE-VIM` ships with the binaries without a change there.
- **Contributor rule** (in `CONTRIBUTING.md`, written 2026-11-01): a contribution to a new
  file is Apache-2.0. A contribution to a Vim-derived file stays under the Vim
  license. Porting Vim code is declared in the commit message and the PR, and
  the new file gets the Vim header. Whether a contributor agreement (CLA) or a
  DCO sign-off is needed is an open question.
- **Process.**
  1. The owner confirms this scheme and the table above.
  2. Legal advice: the Vim license, Apache-2.0 compatibility, and whether a
     Vim-derived file can be re-headed.
  3. One commit adds the headers and `LICENSE-VIM` with no licence text changed.
  4. A second commit changes `LICENSE`, `NOTICE`, the README and `Cargo.toml`.
  5. The modified-version message stays in `:version` and the intro screen in
     every case.

**Open questions.**

- Legal review (not done; this is not legal advice): the Vim license, whether
  Apache-2.0 meets its "does not restrict" test (II.2.c), whether a
  Vim-derived file can be re-headed, and whether `Apache-2.0 AND Vim` is an
  acceptable license field for crates.io and the AUR.
- Owner review of the seven "conservative" files (`buffer.rs`, `cmdline.rs`,
  `editor.rs`, `regex.rs`, `register.rs`, `scroll.rs`, `undo.rs`). A
  line-by-line check against Vim's source could move some of them to
  Apache-2.0. Until then they stay Vim.
- The copyright holder for new code. `NOTICE` uses the git author name
  (Axel Furtenbach); the owner should confirm it. No per-file copyright lines
  were added.
- CLA or DCO for contributions. Not decided.
- Whether the Vim-derived code should move to a separate crate, so that new
  code is not in a Vim-licensed file. Keeping the whole project under the
  Vim license was the simplest option and is what this scheme replaced.

## Repository and packaging

Status on 2026-11-01: the five repository documents are written (`SECURITY.md`,
`CONTRIBUTING.md`, `AGENTS.md`, `MAINTAIN.md`, `INSTALL.md`). The `Makefile` is
present. `.github/workflows/ci.yml` runs on pushes to `main` and on pull requests: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` on Linux, macOS and Windows, with real Vim installed (`RVIM_REQUIRE_VIM=1` on Linux only; elsewhere a missing Vim skips the differential tests with a warning). It has never run on GitHub yet. `.github/workflows/release.yml` is the release workflow. Where a document's
text differs from the rows below, the document is the source of truth.

**Makefile.** Thin wrappers over cargo, so that the commands in CLAUDE.md and
CONTRIBUTING.md have short names. `make help` lists the targets. Implemented:

| Target | Runs |
|---|---|
| `build` | `cargo build --release` |
| `debug` | `cargo build` |
| `test` | `cargo test` |
| `test-vim` | `RVIM_REQUIRE_VIM=1 cargo test`, which fails when Vim is missing |
| `lint` | `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` |
| `fmt` | `cargo fmt` |
| `check` | `lint`, then `test-vim` (the pre-PR check) |
| `run` | `cargo run -- $(ARGS)`, for example `make run ARGS=file.txt` |
| `install` | `cargo install --path . --locked` (into `~/.cargo/bin`; no `PREFIX` or `DESTDIR`) |
| `clean` | `cargo clean` |
| `help` | the default target; lists the `##` comments |

Every target is `.PHONY`, and a failing command fails the target. The file
uses only POSIX make syntax, and was run with mingw32-make on Windows (Git
Bash has no `make` by default). Not planned for the Makefile: a release target.
Not yet: `uninstall`, `dist`, and `PREFIX`/`DESTDIR` for `install`, which the
original plan included. `make` does not replace CI, and `release.yml` stays the
release path.

**Release notes.** `release.yml` passes `.github/release-notes.md` to
`gh release create --notes-file`. For each release, edit that file, commit it
on `main`, and tag a commit that contains it (the workflow reads it from the
tagged checkout; a missing file fails the upload step). The file is not packed
into the archives. The v0.1.0-alpha.3 notes are in that file on the
alpha.3 release branch.

**Documents.** All five were written on 2026-11-01. The table is the original
plan, kept for the record; the files are the source of truth.

| File | Content | Status |
|---|---|---|
| `SECURITY.md` | Private reporting (GitHub private vulnerability reporting, UNVERIFIED that it is enabled on the repository); supported versions (the latest alpha until 0.1.0); the local-only statement: no telemetry, nothing leaves the machine unless the user configures a provider or command, the agent endpoint is local and on a user-only socket, prediction is off by default, recovery files stay on disk | Written 2026-11-01 |
| `INSTALL.md` | Release binaries per platform; `cargo install --path .` and `cargo install --git` (`cargo install rvim` only after a crates.io publish, not planned yet); building from source (the Rust toolchain needed for edition 2024 is UNVERIFIED as a minimum version); `make install`; packaging notes (keep `LICENSE*`, do not change `VERSION_MESSAGE`) | Written 2026-11-01 |
| `CONTRIBUTING.md` | The differential test rule (each behaviour gets `tests/compat_*.rs` cases; when Vim and rvim differ, fix rvim); how to run the tests (`make test-vim`, Vim 9 needed); commit and PR style (imperative subject, the milestone in brackets as in the history); PLAN.md is updated in every commit that moves a milestone; the licensing rule above | Written 2026-11-01 |
| `AGENTS.md` | Instructions for coding agents: read `CLAUDE.md` for the rules and `PLAN.md` for the status, and the test commands. A pointer to `CLAUDE.md`, not a copy; a symlink is avoided because Windows checkouts need developer mode (UNVERIFIED for git) | Written 2026-11-01 |
| `MAINTAIN.md` | Release process: tag `v0.1.0-alpha.N` on `main` after `make lint` and `make test-vim` pass; `release.yml` builds the binaries; release notes name the milestone. Milestone-release rule: release when a milestone moves to done or a user-visible feature lands. Triage: reproduce with `RVIM_REQUIRE_VIM=1`, add a differential case, fix rvim. The AUR update steps | Written 2026-11-01 (the "after the first milestone release" timing no longer applies) |

## Future goals

### Publish on the AUR

- **Packages.** `rvim` builds from the source archive of a tagged release.
  `rvim-bin` repackages the Linux release binary, if one is published for the
  architecture. The PKGBUILDs live in the AUR repositories (keeping them in
  this repository too, under `packaging/arch/`, is undecided).
- **Depends on** the release archives and checksums, `make install` with
  `PREFIX` and `DESTDIR`, and `LICENSE*` being installed. It should not start
  before the Makefile, `LICENSE-VIM` and `INSTALL.md` exist.
- **License field.** Matches "Licensing": `Apache-2.0 AND Vim` since
  2026-11-01. Whether the AUR accepts the identifier `Vim` is UNVERIFIED.
- **Name check** (UNVERIFIED, not yet done). Search the AUR, crates.io, Homebrew
  and the major distribution repositories for existing `rvim` packages, and
  check the name's trademark status, before publishing. Rename if it is taken.

### Logo design

- **Deliverables.** A vector master (`assets/logo.svg`, planned path), a
  favicon (PNG and ICO exported from the SVG, for the project page and the
  repository's social preview), a monochrome text version for the start menu
  (a constant in the source, under 60 columns, drawn by the Frame), a header
  image for the README, and the logo in release assets if the owner wants it.
- **Placeholder until designed.** The start menu shows the word `rvim` in a box.
  The README has no image.
- **Constraints.** The logo must not imitate the Vim logo or any Neovim mark
  (both are their projects' own marks; the trademark view is UNVERIFIED).
  Colours should map to the theme groups in "Styled frame and diff redraw", so
  the same design works in light and dark themes.
- **Who decides.** The owner. The design brief is written with the owner before
  work starts.

## Known differences from Vim

- Command-line flags: `-v` means `--version` in rvim, but Vim's `-v` starts
  Vi mode (rvim has no Vi mode). Vim's other flags (`-u`, `-N`, `-R`, `-es`,
  and so on) are not parsed; an unknown `-x` is taken as a file name, as
  before.
- Emoji use approximate block ranges instead of Vim's full `emoji_all` table
  for word motions; composing characters are separate characters.
- Options come from the registry in `options.rs` (all global for now).
  `:setlocal` is accepted as an alias of `:set`; `:setglobal` does not exist.
  Real local and global option scopes (`:setlocal`/`:setglobal` semantics) are
  still to do.
  `tabstop`, `softtabstop` (negative = 'shiftwidth'), `smarttab` and
  `shiftround` work, but these Vim cases are not matched yet; they are kept
  as `#[ignore]`d `known_gap_*` tests in `tests/compat_options.rs` (run with
  `cargo test -- --ignored`): (a) Insert Tab and BS with 'softtabstop' when
  'tabstop' is 2 or 4 and white space continues after the cursor (Vim's
  tabify step leaves a different run); (b) Replace-mode Tab with 'softtabstop'
  and noexpandtab, and Replace-mode BS over the white space such a Tab made
  (the replace stack assumes one entry per character); (c) block `<` on a line
  with a tab in the block, which removes too much (not tabstop specific);
  (d) block yank and put with 'tabstop' 2 and tabs. There is no `smartindent`,
  `backspace=indent,eol,start`, `whichwrap=b,s` and `startofline` are fixed,
  and the ruler is always shown. `hlsearch`, `incsearch`, `number`,
  `relativenumber`, `wrap` and `autoread` are stored but have no effect yet.
  `scrolloff` works. 'scroll' is not an option yet: a count on CTRL-D/CTRL-U
  sets a per-editor value (as Vim does), with no `:set scroll`.
- Scrolling (milestone 5), not verified against Vim, or only in part:
  the centring of an even-height window (`zz` and far jumps), CTRL-F and
  CTRL-B when the window is near the end of the file or has wrapped lines
  (the rules are from `scroll.txt`; the exact screen results were not
  probed), the cursor after CTRL-B (put on the bottom line with
  'scrolloff'), `scrolloff` counted in screen rows, and a count that is
  larger than the window for `H`/`L` (clamped, which Vim agrees with on a
  fresh window). Not implemented: `z+ z^ z{height}<CR>`, `smoothscroll`,
  `<PageUp>`/`<PageDown>`/`<S-Up>`/`<S-Down>`, and `:set scroll`.
- An Ex line with a very large number of `|` separators (thousands) is slow:
  the cost is quadratic, because each chained command re-parses the rest of
  the line (measured in a debug build: 0.19 s at 2,000 bars, 3.2 s at 8,000;
  not compared with the older code). Fix idea: split the line once.
- An error does not flush pending typeahead (only matters for pasted key
  sequences).
- `n` after `/pat/e+1` when the cursor is on the last char of the buffer
  wraps in Vim but not in rvim (unexplained; see `compat_m3.rs`).
- After `:%s/\n//` Vim leaves the cursor at the last join point; rvim puts
  it on the first non-blank.
- `:s` `\=` expressions (`src/expr.rs`) are a subset. Not supported: floats
  (`1.5` is E15), dictionaries, funcrefs, lambdas, user functions, `v:`, `g:`
  and other scoped variables, Vim9 syntax, `<Key>` names beyond `Esc CR NL Tab
  BS Space lt Bslash Bar NUL`. Strings are indexed
  by byte, so a cut inside a multi-byte character gives U+FFFD. `substitute()`
  expands only `&`, `\&`, `\0`-`\9`, `\t`, `\n`, `\r`, `\\` (no case escapes,
  no `~`). `split()` and `substitute()` match one line, so a pattern cannot
  match a line break there. `printf()` has `%d %x %X %s %%` with `-`, `0` and
  width. Error numbers of the cases that were not probed in Vim (unknown
  `str2nr()` base, `printf()` odd conversions) are not checked. Errors inside
  built-in functions give Vim's default value (0, "" or []) and go on.
  Limits, which Vim does not have in this form: nesting of expressions is 100
  levels and also 512 KB of parser stack (`MAX_DEPTH`, `STACK_LIMIT`; Vim
  accepts 200 and fails at 1000 with E1169, which rvim also reports); a list,
  a range, or a split above 1,000,000 items, and a string above 64 MiB (from
  `repeat()`, `join()`, `.`, `substitute()`), give E342 "Out of memory!" and
  the match is replaced by nothing (Vim may return "" for some of these instead, for
  example `repeat('x', 9223372036854775807)`); a `printf()` width above
  1,000,000 gives E1510. The values one expression keeps (the results of
  function calls, `MAX_TOTAL`, 256 MB) are limited too, so a list of big
  results gives E342. `str2nr()` and literals clamp a too-large number to the
  largest one, as Vim does (`-MAX` for a negative `str2nr()`); a string in
  arithmetic clamps a negative one to the smallest (`'-9...9' + 0`), as Vim
  does. `substitute()` checks the projected size before each match is
  appended, so a size error comes early.
  When an expression error occurs (any error, including E1169 "too recursive",
  which rvim reports at its 100-level nesting limit where Vim allows 200), the
  match is replaced by nothing and the `:s` continues, as Vim does for
  expression errors.
  Speed: `split()` and `substitute()` take time linear in the matches; a `.`
  chain appends in place, so long chains are linear here, where Vim copies the
  left side on each `.`. The `c` flag with `\=` evaluates the expression
  when the match is answered, as the plain replacement is asked about.
- The `\n` (NUL) replacement is not implemented. `:p`, `:nu`, `:=` only set a
  message.
- The `c` flag of `:s` (`ex.rs`, `SubRun`): `<C-e>` and `<C-y>` only ask
  again (Vim scrolls the window). A declined match over a line break (`n`
  after `\n`) moves on to the next line, where Vim sometimes offers the same
  match again; the cases are kept in `tests/compat_m3_confirm.rs` as an
  ignored test until ported. Keys typed after a finished `:s` cannot be
  compared with Vim (see the working notes).
- `:r !cmd` (insert the output of a command) is not implemented; `:r file`
  is. `:r` takes the file's lines as `:e` decodes them (CRLF removed).
- The frame carries highlight groups per cell (`view::Hl`); only Visual and
  the `~`/`@` filler use them so far. The terminal redraws only rows that
  changed. Still missing: hlsearch, incsearch, matching parens, syntax.
- A panic writes unsaved text to a recovery file (`recovery.rs`), but a
  closed terminal (SIGHUP) does not yet, and there are no swap files.
- Recovery files are created owner-only on Unix: the directory with mode 0700
  (an existing directory the user owns is tightened to 0700; a symbolic link or
  a directory of another owner is not used, and the file goes to a fresh private
  directory under the temp dir, which the printed message names), each file with
  mode 0600 and `create_new`, so an existing name or link is never written
  through (`src/recovery.rs`). On Windows the default directory is under the
  per-user `%LOCALAPPDATA%` and no permissions are set; that is the only Windows
  protection. **Unverified (2026-11-01):** the Unix code compiles and passes
  clippy for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`, but its
  five `cfg(unix)` tests (modes, symlinked directory, planted symlink) have
  never run, because the development machine is Windows. Run
  `cargo test --lib recovery` on Linux or macOS (or in WSL with Rust) before
  trusting it; the release workflow has no test step, so this is a to-do.
- CI (`.github/workflows/ci.yml`) has never run on GitHub. Its first run is
  also the first execution of the Unix recovery-file tests (`cfg(unix)` in
  `src/recovery.rs`), on the Linux and macOS runners. A failure there is a real
  finding about the Unix code, not a CI setup problem to paper over.
- Alt, F-keys, Insert and Shift-Tab work (milestone 4). The kitty keyboard
  protocol is still missing (milestone 4, "Left").
- Jumplist: entries are positions in the one buffer. `:e` of another file
  clears the jumplist and the context mark (Vim keeps entries for other files
  and sets the context mark to the old position). `:jumps` prints one line
  (`jumps: 1:1 >3:4`, `>` at the current entry), not Vim's table. Vim's
  `:s` with `\n` in the pattern sets the context mark on the joined line, and
  leaves the cursor on the join point; rvim does neither (cases in
  `compat_jumps.rs`, ignored until the cursor difference above is fixed).
  Columns after a `:s` that splits a line into more than two pieces are not
  checked against Vim.
- `[[ ]]` are not in rvim yet. When they land they must call `set_pcmark()`
  when they move, as `G` and `%` do. `H M L` already do (scrolling landed).
- Marks: only lowercase marks exist. Missing: `mA`-`mZ` (file marks),
  `m[ m] m< m>`, `'[ ']` and `` `[ `] `` as motions, `'<` and `'>` as
  Normal-mode motions, `:marks` and `:delmarks`. The
  Visual marks stay in `Editor::last_visual` and do not follow edits.
- Marks have no column adjustment for single-line edits, as in Vim (`x`, `D`,
  `r`, insert-mode typing do not move them). Joins (`J`, `gJ`, `:j`, `<BS>`
  and `<Del>` at a line end) and `:s` with a line break in the pattern keep
  them on the joined line with Vim's column mapping.
- The test harness cannot test state carried between cases (last search,
  last `:s`, `.`), since rvim starts each case fresh and Vim does not.
