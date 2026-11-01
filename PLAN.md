# rvim plan

Goal: a Vim-compatible modal editor in Rust. "Compatible" means: the same keys
on the same text give the same buffer and cursor as Vim 9 started with
`vim -N -u NONE` (Vim defaults, no vimrc). Every behaviour is checked against
a real Vim by the differential harness in `tests/common/mod.rs`.

## Status: read this first

Last updated 2026-11-01. **rvim is an alpha, roughly 25-30% of this plan.**
Released as `v0.1.0-alpha.1` (that release predates everything below the
"Since the alpha" line). All tests pass against Vim 9.1 and clippy/fmt are
clean at the last commit.

| Milestone | State |
|---|---|
| 1 Core, 2 Vim grammar | Done |
| 3 Search and Ex | ~97%. Left: `:s` `\=` expressions; `<C-e>`/`<C-y>` in the `c` prompt do not scroll yet |
| 4 Foundation | ~70%. Left: the Document/Window/Editor split (the biggest item; see "Foundation"), kitty keyboard protocol, cell-level diff redraw, `:setlocal`/`:setglobal`, recovery on terminal close (SIGHUP) |
| 5 Windows and buffers | ~25%. Done: macros (`q`, `@`, `@@`, `@:`), lowercase marks (`m{a-z}`, `'a`, `` `a ``, `:k`, `:mark`, `'a` addresses), scrolling (`<C-e> <C-y> <C-f> <C-b> <C-d> <C-u> zt zz zb z<CR> z. z- H M L`, counts, `scrolloff`, wrapped lines). Left: splits, buffer list, the other marks (uppercase, `''`, `'[`/`']`, `'<`/`'>` motions), jumplist, swap files |
| 6 Modern editing experience | 0%. Not started (hlsearch, incsearch, syntax, line numbers, config file, palette, mouse) |
| 7 Agent integration | 0%. Not started (file-change reload as an undo step, diff view, ACP connector, agent detection). Research is in "Detecting and connecting to agents" |
| 8 Prediction | 0%. Not started |
| 9 Extras | 0%. Not started |

Since the alpha (all committed and pushed): bracketed paste, Alt/F-keys/Insert/
Shift-Tab, Visual area drawn through `view::Hl` groups, row-level diff redraw,
the options registry (`options.rs`: `autoindent`, `expandtab`, `shiftwidth`
work), panic recovery files (`recovery.rs`), the channel-based event loop
(`event.rs`), `:` command-line editing and history (`cmdline.rs`), `&`/`g&`,
macros (`macros.rs`), `/` and `?` prompts with the same editing keys, `<C-r>`
and history (also in `cmdline.rs`), `gn`/`gN` (`current_search()` in
`search.rs`), `:r`, the `c` flag of `:s` (Confirm mode, `ex.rs`). Search-prompt
editing and history landed in commit e556a0b. Lowercase marks `a`-`z` (kept
in `buffer.rs`, followed through edits by the buffer's line methods, saved and
restored by undo in `undo.rs`), and the scrolling commands with `scrolloff`
(`scroll.rs`; the window's top line now lives in `Editor`).

**In progress** (as of 2026-11-01; maintained by the orchestrator, and an
entry is removed when its branch lands on main):

- Jumplist (`<C-o>`, `<C-i>`/`<Tab>`, `''`, ``` `` ```, `:jumps`, context mark):
  being built by a Haiku agent (branch name pending).
- Indent options (`tabstop`, `softtabstop`, `smarttab`, `shiftround`): being built by a Haiku agent (branch name pending).

**Suggested order for what is left:** (1) Document/Window split, (2) splits +
buffers + scrolling, (3) the ACP connector and file-change reload
(milestone 7), because those are the goals beyond Vim, (4) hlsearch/incsearch/
syntax, (5) prediction. Cut a new release (`git tag v0.1.0-alpha.N`) when a
milestone lands; `.github/workflows/release.yml` builds the binaries.

**Planned additions from the owner's todo (2026-11-01).** These are plans
only. Nothing listed here is built, and the milestone table above is unchanged.

- Start menu at startup, off by default: see "Start menu" (milestone 6).
- License: Apache-2.0 for new code and the Vim license for Vim-derived code,
  pending the owner's decision: see "Licensing".
- Neovim-inspired features, reimplemented from Neovim's documentation: see
  "Neovim-inspired features" (milestone 9 and later).
- Lua extension layer: see "Lua extensions" (milestone 9).
- Neovim plugin compatibility, in stages: see "Neovim plugin compatibility"
  (milestone 9).
- Makefile and repository documents (`SECURITY.md`, `CONTRIBUTING.md`,
  `AGENTS.md`, `MAINTAIN.md`, `INSTALL.md`): see "Repository and packaging".
- Future goals (AUR packages, logo): see "Future goals".

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
  trailing `\r` kept.
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
| `text.rs` | Display widths and virtual columns (tabstop 8, `^X` control chars, wide chars) |
| `motion.rs` | Positions and motions; word motions are ports of Vim's `fwd_word`/`bck_word`/`end_word` with its `inc`/`dec` cursor walk and `utf_class` table |
| `editor.rs` | Mode state machine (Normal, Insert, Command-line), counts, desired column (`curswant`) |
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
   "Working notes"). Left: `:s` `\=` expressions, and the `<C-e>`/`<C-y>`
   scrolling of the prompt (its keys are not handled yet).
   Marks in Ex addresses (`'a`) are done with the marks (milestone 5).
4. **Foundation (in progress).** Done: bracketed paste, Alt/F-keys/Insert/Shift-Tab, Visual area drawn via highlight groups, row-level diff redraw, options registry (`autoindent`, `expandtab`, `shiftwidth`), panic recovery files, channel-based event loop with an input thread. Left: the Document/Window/Editor split, kitty keyboard protocol, cell-level diff, `:setlocal`, SIGHUP recovery. Original description: Restructure before adding more features, because windows,
   agents and prediction all depend on it. Order, each step landing with the
   differential tests green: (a) Document/Window/Editor split, (b) internal
   event loop, (c) styled frame with the Visual area drawn, (d) diff redraw,
   (e) input (bracketed paste, modifiers, kitty protocol), (f) options
   registry, (g) emergency recovery files. Details in "Foundation".
5. **Windows and buffers.** (Macros `q`/`@`, lowercase marks and scrolling are
   done: `<C-e> <C-y> <C-f> <C-b> <C-d> <C-u> zt zz zb z<CR> z. z- H M L` with
   counts, 'scrolloff'.) Left: splits, buffer list, the remaining marks
   (uppercase, `''`, `'[`/`']`, `'<`/`'>` as motions), jumplist; then swap
   files (see "Recovery").
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
  theme) applied only when a user config exists or `--modern` is given. No
  config and no flag means Vim's `-N` defaults.
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
  checked, not only its text). Golden files go under `tests/golden/`
  (planned; format decided when built).
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
  off. New Normal-mode keys use ones Vim leaves free (`<Space>` as leader,
  `<C-\>`), and new Ex commands start with a capital letter (`:Agent`,
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

Status: a planning note. Nothing in this section is in effect. `LICENSE`,
`Cargo.toml` and the README's License section are unchanged. The owner must
confirm the scheme below, and should get legal advice, before any relicensing.

**Current state** (checked 2026-11-01):

- `LICENSE` is the full Vim license text. `Cargo.toml` has
  `license = "Vim"`. The README's License section says the whole project is
  under the Vim license. `VERSION_MESSAGE` in `src/lib.rs` carries the
  modified-version notice that condition II.3 requires.
- No file under `src/` has an SPDX identifier or a copyright line.

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
  - `LICENSE-VIM`: the current Vim license text, byte for byte, moved with
    `git mv`. Renaming does not change the text.
  - `NOTICE`: Apache-2.0 attribution, the Vim attribution (Bram Moolenaar and
    the Vim contributors) and the modified-version statement.
  - README License section: explains both files and which files are which.
  - `Cargo.toml` `license`: an SPDX expression. How Cargo and crates.io treat
    `Apache-2.0 AND Vim` is UNVERIFIED; `license-file` is the fallback.
  - `.github/workflows/release.yml` copies `LICENSE*` into each archive, so
    `LICENSE-VIM` ships with the binaries without a change there.
- **Contributor rule** (in `CONTRIBUTING.md`, planned): a contribution to a new
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

**Open questions.** The copyright holder for new code. CLA or DCO. Whether
`scroll.rs` and `regex.rs` are derived or new (a line-by-line check against
Vim's source is needed). Whether the Vim-derived code should move to a
separate crate. Keeping the whole project under the Vim license (today's
state) is the simplest option and needs no change.

## Repository and packaging

Planned, not started. Nothing below exists in the repository yet (there is no
`Makefile`; `.github/workflows/release.yml` is the only CI file).

**Makefile.** Thin wrappers over cargo, so that the commands in CLAUDE.md and
README.md have short names:

| Target | Runs |
|---|---|
| `build` | `cargo build` |
| `release` | `cargo build --release` |
| `test` | `cargo test` |
| `test-vim` | `RVIM_REQUIRE_VIM=1 cargo test`, which fails when Vim is missing |
| `lint` | `cargo clippy --all-targets` and `cargo fmt --check` |
| `fmt` | `cargo fmt` |
| `run` | `cargo run -- $(FILE)` |
| `clean` | `cargo clean` |
| `install` | copies `rvim` to `$(DESTDIR)$(PREFIX)/bin` (`PREFIX` defaults to `/usr/local`), and `LICENSE*` and `README.md` to `$(DESTDIR)$(PREFIX)/share/doc/rvim/` |
| `uninstall` | removes what `install` copied |
| `dist` | builds the release binary and writes an archive with the same files, named as `release.yml` names them (checked when written) |

Every target is `.PHONY`, and a failing command fails the target. Windows
users run `cargo` directly, or run `make` from Git Bash if GNU make is
installed there (UNVERIFIED). `make` does not replace CI, and `release.yml`
stays the release path.

**Documents.** None is drafted by this plan. Each one is planned as follows.

| File | Content | When |
|---|---|---|
| `SECURITY.md` | Private reporting (GitHub private vulnerability reporting, UNVERIFIED that it is enabled on the repository); supported versions (the latest alpha until 0.1.0); the local-only statement: no telemetry, nothing leaves the machine unless the user configures a provider or command, the agent endpoint is local and on a user-only socket, prediction is off by default, recovery files stay on disk | Before the next release |
| `INSTALL.md` | Release binaries per platform; `cargo install --path .` and `cargo install --git` (`cargo install rvim` only after a crates.io publish, not planned yet); building from source (the Rust toolchain needed for edition 2024 is UNVERIFIED as a minimum version); `make install`; packaging notes (keep `LICENSE*`, do not change `VERSION_MESSAGE`) | Before the next release |
| `CONTRIBUTING.md` | The differential test rule (each behaviour gets `tests/compat_*.rs` cases; when Vim and rvim differ, fix rvim); how to run the tests (`make test-vim`, Vim 9 needed); commit and PR style (imperative subject, the milestone in brackets as in the history); PLAN.md is updated in every commit that moves a milestone; the licensing rule above | With the Makefile |
| `AGENTS.md` | Instructions for coding agents: read `CLAUDE.md` for the rules and `PLAN.md` for the status, and the test commands. A pointer to `CLAUDE.md`, not a copy; a symlink is avoided because Windows checkouts need developer mode (UNVERIFIED for git) | With the Makefile |
| `MAINTAIN.md` | Release process: tag `v0.1.0-alpha.N` on `main` after `make lint` and `make test-vim` pass; `release.yml` builds the binaries; release notes name the milestone. Milestone-release rule: release when a milestone moves to done or a user-visible feature lands. Triage: reproduce with `RVIM_REQUIRE_VIM=1`, add a differential case, fix rvim. The AUR update steps | After the first milestone release |

## Future goals

### Publish on the AUR

- **Packages.** `rvim` builds from the source archive of a tagged release.
  `rvim-bin` repackages the Linux release binary, if one is published for the
  architecture. The PKGBUILDs live in the AUR repositories (keeping them in
  this repository too, under `packaging/arch/`, is undecided).
- **Depends on** the release archives and checksums, `make install` with
  `PREFIX` and `DESTDIR`, and `LICENSE*` being installed. It should not start
  before the Makefile, `LICENSE-VIM` and `INSTALL.md` exist.
- **License field.** Matches the decision in "Licensing". Until then it is
  `Vim`. Whether the AUR accepts the identifier `Vim` is UNVERIFIED.
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

- Emoji use approximate block ranges instead of Vim's full `emoji_all` table
  for word motions; composing characters are separate characters.
- Options come from the registry in `options.rs` (all global for now).
  `tabstop` is fixed at 8, there is no `softtabstop`/`smarttab`/`smartindent`,
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
- An error does not flush pending typeahead (only matters for pasted key
  sequences).
- `n` after `/pat/e+1` when the cursor is on the last char of the buffer
  wraps in Vim but not in rvim (unexplained; see `compat_m3.rs`).
- After `:%s/\n//` Vim leaves the cursor at the last join point; rvim puts
  it on the first non-blank.
- `\=` expressions and the `\n` (NUL) replacement are not implemented.
  `:p`, `:nu`, `:=` only set a message.
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
- Alt, F-keys and Shift-Tab now work. (Old note: - Alt, F-keys and Shift-Tab are ignored; there is no recovery file after a
  crash. Both are fixed in milestone 4. (Bracketed paste is done: it is
  enabled in `main.rs`, handled by `Editor::paste`, and tested in
  `tests/paste.rs`; step (e) of milestone 4 still needs modifiers and the
  kitty protocol.)
- Marks: only lowercase marks exist. Missing: `mA`-`mZ` (file marks), `m'`,
  `` m` ``, `m[ m] m< m>`, `'[ ']` and `` `[ `] `` as motions, `''` and
  `` `` `` (both need the jumplist; a jump does not set the previous-context
  mark), `'<` and `'>` as Normal-mode motions, `:marks` and `:delmarks`. The
  Visual marks stay in `Editor::last_visual` and do not follow edits.
- Marks have no column adjustment for single-line edits, as in Vim (`x`, `D`,
  `r`, insert-mode typing do not move them). Joins (`J`, `gJ`, `:j`, `<BS>`
  and `<Del>` at a line end) and `:s` with a line break in the pattern keep
  them on the joined line with Vim's column mapping.
- The test harness cannot test state carried between cases (last search,
  last `:s`, `.`), since rvim starts each case fresh and Vim does not.
