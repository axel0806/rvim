# rvim plan

Goal: a Vim-compatible modal editor in Rust. "Compatible" means: the same keys
on the same text give the same buffer and cursor as Vim 9 started with
`vim -N -u NONE` (Vim defaults, no vimrc). Every behaviour is checked against
a real Vim by the differential harness in `tests/common/mod.rs`.

## Status: read this first

Last updated 2026-11-13 (milestone 7 step 5b-3, subscriptions: the agents' ACP routes and sign-in methods were researched from current sources and written up in "Agent SDKs and subscriptions" (Claude Code, Codex, Gemini CLI and Antigravity, Copilot, Cursor and the rest, with what is UNVERIFIED and the Anthropic terms question for the owner); the registry has the registry's 2026-11-13 versions, `-y` for Codex's `npx`, the new agent `antigravity` (`agy acp`) and the fields `login` and `login_methods`; when an agent answers auth required and advertises methods rvim may send, the message line offers them as `[1]`..`[9]` (`ai: <id> login?`), nothing is sent without a key, `n` or `<Esc>` stops the process and shows the login hint, and a failed `authenticate` offers the choice again (`src/agent_session.rs`, `src/acp.rs`; `tests/agent_auth_choice.rs`); milestone 7 step 5b-2: `:Agent {text}` sends one prompt turn (text only), `:Agent` alone opens the prompt line, one turn at a time, the agent log as an unmodifiable scratch document shown by `:AgentLog` (`:AgentLog!` goes back), streamed messages, thoughts, tool calls, plans, stop reasons and errors in it, the leader keys `\a` `\al` `\ac` with the opt-in only, the working, cancelling and PERMISSION indicator states, a `-32000` on a prompt as the auth-required state, and permission requests shown as waiting and answered as cancelled by `:AgentCancel`; see "ACP connector, step 5b-2"; milestone 7 step 5b-1b: the reader no longer holds the child's lock while it waits, so `Stop` always kills; a retry re-checks the approval; the registry adds gemini, github-copilot-cli, opencode, goose and qwen-code; milestone 7 step 5b-1: the ACP connector's process and connection on worker threads (`src/agent_conn.rs`); `:AgentConnect [name]` with the consent prompt on the message line (`y` saves the approval to `agent-approvals.toml`, `n` and `<Esc>` do nothing), `:AgentDisconnect`, `:AgentCancel`, the lifecycle states and backoff timed through `next_deadline`, the indicator states `ai: <id> connecting`, `ai: <id>`, `ai: <id> retry n/5` and `ai: <id> error`, the approval store (`src/agent_approval.rs`), and `Editor::prediction_allowed` (false); see "ACP connector, step 5b-1"; milestone 7 step 3a: the conflict prompt for a modified buffer whose file changed, behind the agent opt-in (`[agent] enabled = true` or `--agent`; off by default, and with it off the W11/W12 warnings are Vim's as before): `l` loads the disk text, `k` and `<Esc>` keep the buffer, `m` merges (`src/merge.rs`), `d` (the diff view) is not built yet; a per-document baseline and the state `k` saw; the `:w` guard (W12 for a file that changed since reading, E13 for leftover conflict markers, `:w!` overrides); a deleted file keeps the buffer and says so; the code is in `src/conflict.rs`; milestone 5 uppercase file marks: `mA`-`mZ`, `'A` and `` `A `` as motions, operator targets and Ex addresses, `:k A` and `:mark A`, `:marks` and `:delmarks` with `A-Z`; the marks live in `Buffer::file_marks` (`src/buffer.rs`) with one letter per editor, and `Editor::set_named_mark()` clears a letter in the other documents; they follow edits as Vim's mark_adjust() does for file marks (a mark on deleted lines moves to the line after them) and undo does not restore them, as in Vim; E19, E20 and E78 as Vim; checked by `tests/compat_marks_file.rs`; see "Known differences"; milestone 6 hlsearch and incsearch: the `Search` and `IncSearch` groups in the styled frame, `:nohlsearch` and its re-enabling rules, and the `/` and `?` prompt that scrolls the view to its match and restores the view on cancel (see "Highlighting" under "Editing experience"); milestone 6 line numbers: `number`, `relativenumber` and `numberwidth` (window-local), drawn in a number column by `view.rs` with the text area narrowed by it for wrapping and scrolling, and the `LineNr` and `CursorLineNr` groups; see "Line numbers" under "Editing experience"; milestone 7 step 4b: `:AgentScan`, which runs the detection on a worker and shows the result on the message line (`src/agent_scan.rs`, `Event::AgentScan`); the status indicator `ai: ...` for the states without a connection, drawn only after the opt-in or after `:AgentScan`, left of the ruler; the config's `[agent] enabled` opt-in, with a scan at startup, and `shareable` and `log` warned as not supported yet; see "Detecting and connecting to agents", section 11 row 4, and "Known differences"; milestone 7 step 4a: the agent registry and the read-only detection core in `src/agent_detect.rs`, with the registry checked against the agents' own docs and the ACP registry, see "Detecting and connecting to agents"; milestone 7 step 5a: the ACP connector core in `src/acp.rs`, pure with no process or UI: newline-delimited JSON-RPC framing with an 8 MiB line cap, typed ACP v1 messages, and the `Conn` session state machine driven by a fake clock; recorded transcripts in `tests/fixtures/acp/` replayed by `tests/acp_connector.rs`; `serde` and `serde_json` added, see "ACP connector core" and "Dependency choices"; the modified-Vim notice on the message line is the short `NOTICE_SHORT` (39 characters, fits 80 columns) for the intro and `:version`, with the config note `[N config warnings]`; the full `VERSION_MESSAGE` stays in `--version` and the start menu; see "Licensing" and "Known differences"; milestone 6 config file loader: `config.toml` read once at startup, `[options]`, `[start] menu`, raw `[agent]`, `-u NONE`, `--clean`, `-N`, `:ConfigWarnings`; see "Config" under "Editing experience"; the non-file marks: `'[` `']` `'<` `'>` `'"` as motions, operator targets and Ex addresses, `m[ m] m< m>`, the `'[`/`']` marks set by each command, `:marks` and `:delmarks`; milestone 7 step 2b: the file watcher thread (`src/watch.rs`) with the option `filewatch`, off by default: `FileChanged`, `FileDeleted` and `FileTooLarge` events, echo suppression by hash, reads capped at 64 MiB, one decision shared with `:checktime` (`Editor::check_disk`), and the change applied in Normal mode only; milestone 7 step 2b: the file watcher thread (`src/watch.rs`) with the option `filewatch`, off by default, echo suppression by hash, reads capped at 64 MiB; milestone 7 step 2a: undo steps record their source, the disk state per document with a hash, `:checktime`, and the reload of an unmodified buffer as one undo step with cursor and scroll mapping; milestone 4 option scopes: `:setlocal` and `:setglobal`, buffer-local, window-local and global-local values, see "Options registry"; Document/Window/Editor split steps 1 to 3 of milestone 4 (ids, collections, accessors, version counter, marks, `buf_mut()`, window size); CI first ran on GitHub; Linux builds Vim 9.1.1914 from source, and all three OS jobs require Vim; cell-level diff redraw in `redraw.rs`, with its escape sequences tested in `main.rs`; kitty keyboard protocol, with `RVIM_NO_KITTY=1` to turn it off and no support query when `TERM` is unset, empty or `dumb`; SIGHUP/SIGTERM recovery on Unix, milestone 4; Makefile packaging targets `install` with `PREFIX`/`DESTDIR`, `uninstall` and `dist`). **rvim is an alpha, roughly 25-30% of this plan.**
As of 2026-11-01 milestones 1-3 are done; 4 and 5 are partly done; 6 has line numbers, hlsearch and incsearch, the start menu and the config loader; 7 has its file reload, file watcher (2a, 2b), conflict prompt (3a) the ACP protocol core (5a), the connector's process and consent (5b-1), prompt turns and the agent log (5b-2), and agent detection and the opt-in with its indicator (4a, 4b), 8-9 are plans.
Released as `v0.1.0-alpha.1`, `v0.1.0-alpha.2` and `v0.1.0-alpha.3`. The next release is `v0.1.0-alpha.4`, not yet tagged. All tests pass against Vim 9.1 and clippy/fmt are
clean at the last commit.

| Milestone | State |
|---|---|
| 1 Core, 2 Vim grammar | Done |
| 3 Search and Ex | 100%. `:s` `\=` expressions done as the subset in `src/expr.rs` (see Known differences) |
| 4 Foundation | 100%. The Document/Window/Editor split: steps 1 to 3 done (one document and one window, with `DocId`/`WinId` collections and window size from the screen; see "Foundation"). Option scopes done: `:setlocal`, `:setglobal`, buffer-, window- and global-local values (see "Options registry"). Foundation is complete; the rest is milestone 5 (layout and splits, several documents and windows in use) |
| 5 Windows and buffers | ~30%. Done: macros (`q`, `@`, `@@`, `@:`), lowercase marks (`m{a-z}`, `'a`, `` `a ``, `:k`, `:mark`, `'a` addresses), uppercase file marks (`m{A-Z}`, `'A`, `` `A ``, `:k A`, `:marks`, `:delmarks A-Z`, E19/E20/E78; `src/buffer.rs` `file_marks`), the other marks (`'[` `']` `` `[ `] `` as motions, operator targets and Ex addresses, set by yank, delete, change, insert, put, shifts, `J`, `~` `g~` `gu` `gU`, `:s`, `:d`, `:m`, `:t`, `:pu`, undo and redo; `'<` `'>` and `'"`; `m[ m] m< m>`; `:marks`, `:delmarks`; `src/marks.rs`), the jumplist (`<C-o>`, `<Tab>` with counts, `:jumps`) and the context mark (`''`, `` `` ``, `m'`, `` m` ``), scrolling (`<C-e> <C-y> <C-f> <C-b> <C-d> <C-u> zt zz zb z<CR> z. z- H M L`, counts, `scrolloff`, wrapped lines). Left: splits, buffer list, swap files |
| 6 Modern editing experience | Started, not near done. Landed: line numbers (`number`, `relativenumber`, `numberwidth`; see "Line numbers"); hlsearch (`hls`, `:noh`, every match of the last pattern drawn with the `Search` group) and incsearch (`is`, the match of the `/` and `?` prompt drawn with `IncSearch` and shown by scrolling, restored on cancel; see "Highlighting"); the start menu (opt-in `--start-menu`, `startmenu` option); the config file loader (`config.toml`: `[options]`, `[start] menu`, `[agent]` kept raw; `-u NONE`, `--clean`; `:ConfigWarnings`; see "Config"). Left: syntax, palette, mouse, `:Start`, the start menu's recent files, Config and agent entries, its modern-profile default (see "Start menu"), and the config's `[profile]`, mappings and theme |
| 7 Agent integration | ~20%. Step 3a done (2026-11-01): the conflict prompt behind the agent opt-in, the `:w` guard, the merge and the deleted-file message (`src/conflict.rs`, `src/merge.rs`; see "Files changed under rvim" and "Known differences"). Left in step 3 (3b): the gutter, the `[base]` document, the diff view `d` and `:AgentChanges`. Step 4a done (2026-11-01): the agent registry and the read-only detection core (`src/agent_detect.rs`, see "Detecting and connecting to agents"); 4b done (2026-11-01): `:AgentScan`, the status indicator without a connection and the `[agent] enabled` opt-in (`src/agent_scan.rs`; see section 11, row 4, and "Known differences"). Step 5a done (2026-11-01): the ACP protocol core, pure (`src/acp.rs`, see "ACP connector core"). Step 2a done (2026-11-01): undo steps record their source (`UndoSource`), the disk state per document (`DiskState`, FNV-1a hash, mtime and size, taken by `:e`, `:w` and `:checktime`), `:checktime` with W11, W12, W13 and E211, and a changed file reloaded when unmodified and `autoread` is on, as one undo step tagged `external` with the cursor and scroll mapped (`src/reload.rs`, `Editor::apply_external_change`). Step 2b done (2026-11-01): the file watcher thread (`src/watch.rs`, option `filewatch`, off by default; `FileChanged`, `FileDeleted` and `FileTooLarge` events, echo suppression by hash, reads capped at 64 MiB, see "Files changed under rvim"). Step 2 is complete. Step 3a done (2026-11-01): the conflict prompt, the `:w` guard, the merge and the deleted-file message, behind the agent opt-in (`src/conflict.rs`, `src/merge.rs`; see "Files changed under rvim" and "Known differences"); the rest of step 3 is 3b (see section 11). Step 5b-1 done (2026-11-01): the connector's process, consent, approval store, `:AgentConnect`, `:AgentDisconnect`, `:AgentCancel`, lifecycle and backoff (see "ACP connector, step 5b-1"). Step 5b-1b done (2026-11-01): the reader no longer holds the child's lock while it waits, so `Stop` always kills; a retry re-checks the approval; the registry adds gemini, github-copilot-cli, opencode, goose and qwen-code (see "Detecting and connecting to agents"). Step 5b-2 done (2026-11-01): `:Agent` prompt turns, the agent log, `\a` `\al` `\ac` with the opt-in, `:AgentLog`, the working and PERMISSION indicator states, the auth state on a prompt (`src/agent_turn.rs`; see "ACP connector, step 5b-2"). Step 5b-3 done (2026-11-13): the subscription sign-in choice, the registry corrections and the research in "Agent SDKs and subscriptions" (no real agent has been run yet). Left in 5: `:AgentUse`, several connections, terminal sign-in methods (they need the PTY of step 7) and context attachments. Not started: the diff view. |
| 8 Prediction | 0%. Not started |
| 9 Extras | 0%. Not started |

Since the alpha (all committed and pushed): `:s/pat/\=expr/` with the
expression subset in `src/expr.rs` (numbers, strings, lists, operators, `?:`,
indexing and slicing, the common built-in functions, Vim's error messages;
differential cases in `tests/compat_m3_expr.rs`), bracketed paste, Alt/F-keys/Insert/
Shift-Tab, Visual area drawn through `view::Hl` groups, cell-level diff redraw
(`redraw.rs`: runs of changed cells with one style, wide characters, cleared
row tails, a full repaint on resize and the first draw),
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

- Agent integration, step 5b-1: done on branch `worktree-agent-af855357da4ca928b` (the connector's process, consent, approval store, `:AgentConnect`, `:AgentDisconnect`, `:AgentCancel`, lifecycle and backoff; milestone 7). Remove this entry when the branch lands on main.
- Agent integration, step 3a: the conflict prompt, the `:w` guard and the merge behind the agent opt-in (milestone 7; `src/conflict.rs`, `src/merge.rs`). Remove this entry when the branch lands on main.
- Agent integration, step 5b-2: done on branch `worktree-agent-a02a0e6055e4be789` (`:Agent` prompt turns, the agent log, the leader keys, the auth state on a prompt; milestone 7). Remove this entry when the branch lands on main.
- Agent integration, step 4b: done on branch `worktree-agent-a77247acadd2b92b6` (`:AgentScan`, the status indicator and the opt-in config table; milestone 7). Remove this entry when the branch lands on main.
- Milestone 6 line numbers (`number`, `relativenumber`, `numberwidth`): see "Line numbers" under "Editing experience"
- Milestone 5, uppercase file marks (branch `worktree-agent-a1ff665089217bf14`; removed when it lands on main)

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
- Makefile: done 2026-11-01 (`build`, `debug`, `test`, `test-vim`, `lint`, `fmt`, `check`, `run`, `install` with `PREFIX`/`DESTDIR`/`SKIP_BUILD`, `uninstall`, `dist`, `cargo-install`, `clean`, `help`); see "Repository and packaging". `release` is not in it.
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
| `marks.rs` | The non-lowercase marks (`'[ ']` in `Document::op_start`/`op_end`, `'< '>` in `Document::last_visual`, `'"`), `:marks` and `:delmarks`; ports of Vim's `getmark`, `setmark_pos`, `clrallmarks`, `show_one_mark` and `ex_delmarks` in mark.c |
| `jumplist.rs` | The jumplist and the context mark (ports of Vim's `setpcmark`, `checkpcmark`, `movemark`). Window state, so it moves to `Window` with the split; the buffer logs its line edits (`PosEdit`) and the editor applies them in order |
| `ex.rs` | Ex commands: ranges (`%`, `.`, `$`, marks `'<` `'>` and `'a`, `/pat/`, `;`), file commands, `:k`/`:mark`, `:d :y :pu :m :t :j :> :<`, `:s` (port of `do_sub`; the `c` prompt keeps the loop state in `SubRun` between keys), `:g`/`:v` and command lines as queued `ExTask`s, `:normal`, `:set` |
| `normal.rs`, `ops.rs`, `textobject.rs`, `register.rs`, `undo.rs` | Normal/Visual parsing and motions, operators, text objects, registers, undo tree |
| `regex.rs` | Vim regex (`\v \m \M \V`, `\{}`, look-around, `\zs`, `\_x`, collections, `\%` items) as a backtracking matcher over buffer lines |
| `search.rs` | Port of `searchit()` / `do_search()` (offsets, `;`, `cpo-c`, wrapscan), `*`/`#` word lookup |
| `scroll.rs` | The window's top line and size (in `Editor`), `update_topline` with 'scrolloff' and wrapped lines, and the scroll commands (`CTRL-E/Y/F/B/D/U`, `z` commands, `H M L`) |
| `view.rs` | Layout as a pure function of the editor (`frame`): wrapping, `~`/`@` rows, message line and ruler as plain strings |
| `redraw.rs` | The cell grid of a `Frame` and `diff` between two grids, which returns `DrawOp`s (terminal-free; `main.rs` writes them) |
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
- `main.rs` enables bracketed paste, so a paste arrives as `Event::Paste`.
  The paste handler replays characters one by one (not one undo step).
- `convert()` drops Alt, F-keys, Shift-Tab, Insert and other modifiers, and
  `Key` cannot represent them.
- The panic hook restores the terminal but saves nothing.
- `Editor` holds a single buffer, cursor and undo tree, and `View` lives in
  `main.rs`.
- `event::read()` blocks, so nothing can happen except on a key press: no
  timers (`timeoutlen`), no file watching, no background results.
- `Options` is three fixed booleans (`ignorecase`, `smartcase`, `wrapscan`).

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
4. **Foundation (done).** Done: bracketed paste, Alt/F-keys/Insert/Shift-Tab, kitty keyboard protocol, Visual area drawn via highlight groups, cell-level diff redraw (`redraw.rs`), options registry with option scopes (`:setlocal`, `:setglobal`; see "Options registry"; `autoindent`, `expandtab`, `shiftwidth` and the others), panic recovery files, SIGHUP/SIGTERM recovery files on Unix, channel-based event loop with an input thread, the Document/Window/Editor split. Original description: Restructure before adding more features, because windows,
   agents and prediction all depend on it. Order, each step landing with the
   differential tests green: (a) Document/Window/Editor split, (b) internal
   event loop, (c) styled frame with the Visual area drawn, (d) diff redraw,
   (e) input (bracketed paste, modifiers, kitty protocol), (f) options
   registry, (g) emergency recovery files. Details in "Foundation".
5. **Windows and buffers.** (Macros `q`/`@`, lowercase marks, the jumplist, the
   context mark and scrolling are done: `<C-e> <C-y> <C-f> <C-b> <C-d> <C-u> zt
   zz zb z<CR> z. z- H M L` with counts, 'scrolloff'; the non-file marks
   `'[ ']`, `'< '>`, `'"`, `m[ m] m< m>`, `:marks` and `:delmarks`, in
   `marks.rs`.) Left: splits, buffer list; then
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

**Step 1 done (2026-11-01).** One document and one window, with the fields moved out of `Editor`. `src/document.rs` has `Document`: `buf` (the `Buffer`, marks included), `undo` (`UndoTree`), `uline` (`ULine`) and `last_visual` (Vim's `b_visual`, so it goes with the document, as marks do). `src/window.rs` has `Window`: `cursor`, `curswant`, `keep_curswant`, `vstart`, `jumps` and `top`. `Editor` holds them as `doc` and `win`. Still in `Editor`: `op_start`/`op_end` (the `'[` and `']` marks; they move with the marks work), `screen_cols`/`screen_rows` (the terminal size), `scroll_opt` (the `'scroll'` option, window-local in Vim, so it waits for window-local options), and the rest of the table (mode, registers, search, `.`, command line, messages, options). Accessors: `Editor::buf()` (read-only; tests and `main.rs` use it) and `Editor::cursor()`. Code inside the crate uses the direct paths `self.doc.buf` and `self.win.cursor`, not `&self` methods, so two fields can still be borrowed at once. Not yet: several documents or windows, stable ids, the document version counter, `buf_mut()`.

**Step 2 done (2026-11-01).** Ids and collections, with still one document and one window and no change in behaviour. `src/document.rs` has `DocId(usize)` and `src/window.rs` has `WinId(usize)`. `Editor` holds `documents: Vec<Document>`, `windows: Vec<Window>` and `cur_win: WinId`. `Window` has `doc: DocId`, so the current document is `windows[cur_win].doc`. Nothing is removed yet, so ids stay valid. Accessors on `Editor`: `doc()`, `doc_mut()`, `win()`, `win_mut()`, and `parts_mut()`, which returns the current document and window at once (a split borrow, no `unsafe`). Code in the crate uses them everywhere; the direct `self.doc.x` and `self.win.x` paths are gone. `buf()` and `cursor()` still work. `Document::version()` is the text's change counter: the buffer's `changedtick` (bumped by every text change, by undo and redo, and by `mark_changed`) plus a base that `Document::reload()` advances, so it never goes back when `:e` replaces the text. Its unit tests (in `document.rs`) cover insert, delete, undo/redo, reload and cursor motion. The `'[` and `']` marks (`op_start`/`op_end`) moved from `Editor` into `Document`, as Vim's `b_op_start` and `b_op_end`; a reload keeps them. Asynchronous results will carry `version()`; nothing uses it yet. Not yet: several documents or windows in use, layout, `:new`, `buf_mut()`, and the `screen_*`/`scroll_opt` fields and the rest of the table, which stay in `Editor`.

**Step 3 done (2026-11-01).** The last foundation step, with no change in behaviour. `Editor::buf_mut()` is the mutable counterpart of `buf()`, and the crate reaches the text through `buf()`/`buf_mut()` or the `doc()`/`doc_mut()`/`win()`/`win_mut()`/`parts_mut()` accessors. The `documents` and `windows` Vecs are private to `editor.rs`. `Window` has `width` and `height` (the text area: the screen's columns and the rows above the command line), set by `Editor::fit_windows` from `set_screen_size`. Scrolling and the frame's text area read them; the frame and status line still use the screen size. Nothing else moved: after step 2 the per-document fields were already in `Document`, and the remaining editor-level state (registers, `.`, search, `resel`, `last_pat`) is global in Vim. `screen_cols`/`screen_rows` are the terminal size and stay in `Editor`; `scroll_opt` stays too, with the options work. Unit tests in `editor.rs` cover the id round trips, `parts_mut` disjointness, window size from the screen size, and `version()` not changing on resize. Left for milestone 5: the window layout (each window gets its own rectangle), `:new`, a second document, window-local options (`'scroll'`), and more than one window in use.

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
- Where the terminal supports it, rvim pushes the kitty keyboard protocol flag
  `DISAMBIGUATE_ESCAPE_CODES` (only that flag; no release or repeat events),
  so `<C-i>` vs `<Tab>`, `<C-m>` vs `<CR>` and `<Esc>` vs Alt-key are told
  apart on the wire. Done in `main.rs`: `supports_keyboard_enhancement` is
  checked first, but not when `TERM` is unset, empty or `dumb` (the query
  blocks up to 2 s on a terminal that answers neither query; `kitty_wanted`
  decides this, with `RVIM_NO_KITTY` folded in), the flag is popped on every
  exit path (`restore_terminal`, which the panic hook also calls), and
  `RVIM_NO_KITTY=1` turns it off. The
  key mapping gives the same keys as the legacy bytes (`<C-i>` is `<Tab>`,
  as in Vim). Unit tests in `main.rs` cover the table; the protocol itself
  is a manual checklist item (see "Known differences").

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

**Scopes (done).** The scope table in `src/options.rs` (`TABLE`) is Vim
9.1's, from `:help local-options`, and the unit test `scope_table_matches_vim`
checks every row:

| Scope | Options (short name) |
|---|---|
| global | `ignorecase` (`ic`), `smartcase` (`scs`), `wrapscan` (`ws`), `smarttab` (`sta`), `shiftround` (`sr`), `hlsearch` (`hls`), `incsearch` (`is`); `startmenu` and `filewatch` are rvim-only |
| buffer-local | `autoindent` (`ai`), `expandtab` (`et`), `shiftwidth` (`sw`), `tabstop` (`ts`), `softtabstop` (`sts`) |
| global-local to buffer | `autoread` (`ar`) |
| global-local to window | `scrolloff` (`so`) |
| window-local | `number` (`nu`), `relativenumber` (`rnu`), `numberwidth` (`nuw`), `wrap` |

Storage: the global values are in `Editor`'s `Options`. A document has its
buffer-local values (`Document::local`, a `Local`), and a window has its
window-local values (`Window::local`). A buffer-local or window-local value is
always set: a new document or window copies the global values when it is
made (so a later `:setglobal` does not change an existing buffer's copy). The
local values are kept raw, as Vim keeps them: a boolean is 0 or 1. A
global-local value is unset while it is negative (Vim's `-1`); a negative
local value makes the global value the effective one, and a negative number
shows as is (`scrolloff=-2`). Every read goes through `Editor::eff()`, the
effective value for the current document and window (the local value if set,
else the global one). With one document and one window the behaviour of the
existing tests is unchanged.

What the commands write (probed with `vim -N -u NONE -es` on 2026-11-01):

- `:set opt=val`, `opt`, `noopt`, `invopt`, `opt!`, `opt&`: the global value,
  and the local value too for a buffer-local or window-local option. For a
  global-local option the local value changes only when it is already set:
  `:setglobal so=3` then `:set so=5` leaves the local value at -1 (probed),
  and `:set so&` and `:set so<` do the same. Once set, `:set opt&` makes both
  the default and `:set opt<` copies the global value to the local value.
- `:setlocal`: the local value only. For a global-only option, which has one
  value, it sets that value. `:setlocal opt<` copies the global value to the
  local one, or unsets the local value of a global-local option. `:setlocal
  opt&` sets the local value to the default.
- `:setglobal`: the global value only. The local value does not change, so
  the current buffer keeps its `shiftwidth` after `:setglobal sw=4`.
  `:setglobal opt<` changes nothing.
- `+=`, `-=`, `^=` and `!` start from a base that depends on the command:
  `:set` from the effective value (`:setglobal so=3` then `:set so+=2` gives
  global 5, local still -1); `:setglobal` from the global value; `:setlocal`
  from the raw local value, which is -1 while a global-local option is unset
  (`:setglobal so=3` then `:setlocal so+=2` gives local 1, effective 1; and
  `:setlocal so-=1` gives local -2, effective 3). A boolean's `!` and `inv`
  are `^ 1` on that base, so `:setlocal ar!` on an unset value gives -2,
  which is still unset. A negative value is E487 for every option except
  `softtabstop` and `scrolloff`, which Vim accepts.
- Queries: `:set opt?` shows the effective value; `:setlocal opt?` the raw
  local value (`--opt` for a boolean that is negative, as Vim shows an unset
  one; `opt=-2` for a number at -2); `:setglobal opt?` the global value.
- Errors are Vim's: E518 (the item as typed), E474 (`noopt` of a number,
  `opt=` for a boolean), E521 (a number after `=` is missing or not a number),
  E487 (negative).

Tests: `tests/compat_option_scopes.rs` (differential cases with `:setl`,
`:setg` and their long forms, the three commands in the same keys as `:set`,
and message and error checks against Vim's messages, which the harness cannot
compare), and the unit tests in `options.rs` (the scope table, two documents
and two windows in one `Editor`, and the write, reset and query rules).

### Recovery

- Milestone 4: the panic hook writes every modified document to a recovery
  file (`$XDG_STATE_HOME/rvim/recovery/<name>.<time>` or the platform
  equivalent) and prints its path after the terminal is restored. On Unix,
  SIGHUP (terminal closed) and SIGTERM do the same and then exit with
  128 + the signal number (129, 143). The signal handler does nothing but
  hand the signal to signal-hook's iterator thread, which sends an
  `Event::Signal` into the main loop; the main thread restores the terminal,
  calls `recovery::write_modified` and exits (`signals.rs`, `main.rs`). Only
  `cfg(unix)` registers signals, so Windows is unchanged. Done 2026-11-01.
  A terminal I/O error that ends the loop also saves the text.
- Milestone 5: Vim-style swap files with `directory`, periodic writes
  (`updatetime`, `updatecount`), the "swap file exists" prompt on open, and
  `:recover`. Recovery files from the panic hook are offered at the next
  start.

## Editing experience (milestone 6)

- **Highlighting, hlsearch and incsearch (done 2026-11-01).** `hlsearch`
  paints every match of the last used pattern (`last_pat`, Vim's `RE_LAST`)
  in the window with the `Search` group (`Editor::hl_matches`, drawn in
  `view.rs`). It also covers the line above the top line, for a match that
  runs into the window. An empty match is one cell: its char, or the cell
  after the line's end. A match across a line break covers the break cell.
  The Visual area and the incsearch match are drawn over it. `:nohlsearch`
  (`:noh`, `:nohl`, ...) sets `no_hlsearch` until the next search (`/ ? n N
  * #`), a `:s` or `:&` with a pattern, or `:set [no]hls`, `invhls`, `hls!`
  (Vim 9.1 `v:hlsearch` probes: `n`, `*` and `:s` turn it on; `<C-l>` does
  not change it). `incsearch`: while a `/` or `?` prompt is open,
  `Editor::incsearch_update` finds the match of the text typed (from the view
  the prompt opened with; `/offset` is ignored), moves the cursor there,
  scrolls to show it and draws it with `IncSearch`. `<Esc>` and `<C-c>` put
  the cursor and top line back. `<CR>` puts them back before the search runs,
  so the scroll is the same as without `incsearch`. Both options are off by
  default, so with no config nothing is drawn, as in `vim -N -u NONE`. Tests:
  `tests/search_highlight.rs` (Frame goldens and option forms) and
  `tests/compat_m6_search.rs` (differential, text and cursor only).
  `MatchParen` and syntax highlighting are not done. Other items:
  highlighting via tree-sitter grammars per filetype, filetype detection by
  extension and shebang. Highlighting runs off the UI thread on a snapshot and
  posts spans back as an event (see document versions above).
- **Chrome**: line numbers (done, see "Line numbers" below), a real status line,
  and a buffer/tab line once there are buffers.
- **Defaults**: a built-in modern profile (`hlsearch`, `incsearch`,
  `autoindent`, `scrolloff=5`, `ignorecase smartcase`, mouse, true-colour
  theme) applied only when `--modern` is given or the config sets
  `[profile] modern = true`. No config and no flag means Vim's `-N` defaults.
  (refines the earlier wording "when a user config exists": a config with only
  other tables, such as `[agent]`, changes no default and no key.)
- **Config** (the loader is done, `src/config.rs`; mappings and theme are
  not). Start with data, not a language; a Vimscript subset is milestone 9.
  - File: `$XDG_CONFIG_HOME/rvim/config.toml`. Without `XDG_CONFIG_HOME`, or
    with one that is not absolute (the XDG spec says to ignore it): on Windows
    `%APPDATA%\rvim\config.toml` (the same directory as
    `agent-approvals.toml`), and elsewhere `~/.config/rvim/config.toml`. On
    Windows, `~/.config` uses `%USERPROFILE%`, then `HOME`. Empty variables
    count as unset. Windows `XDG_CONFIG_HOME` is honoured too (the plan's
    Windows XDG locations are UNVERIFIED; this follows the approvals file).
  - Format: TOML 1.x (crate `toml`, see "Dependency choices").
  - Keys read in this version:
    - `[options]`: each option of the registry by its `:set` name or short
      name, with its type: `shiftwidth = 2`, `expandtab = true`. Vim options
      that rvim accepts and ignores (`list`, `ruler`, ...) are dropped
      silently, as `:set` ignores them.
    - `[start]` `menu = true|false`: the start menu when no flag is given.
      `startmenu = true` in `[options]` is the same option, and it works too.
    - `[agent]`: `enabled = true|false` is read (step 4b: the status indicator
      and a scan at startup, `agent_scan.rs`). `shareable` and `log` warn as
      not supported yet. Nothing else acts on it yet.
  - Reserved, warned as "not supported yet" and ignored: `[profile]` (the
    modern profile), `[lua]`. Not yet read at all: mappings, theme, and
    `[start] recent_files`, which warn as unknown keys.
  - Problems are warnings, never errors. An unknown key, an unknown option or
    a value of the wrong type is skipped with a warning, and the rest of the
    file still applies. A syntax error gives one warning with the line number,
    and the defaults. A file over 256 KiB is not read (a warning). A file that
    is not UTF-8 is not read (a warning with the line of the first bad byte).
    A missing file is not a warning: it is the defaults.
  - When: read once in `main.rs`, before the terminal is set up (a small
    blocking read; the approvals file is the other startup read, see "Known differences"). The values go to the
    editor through `Editor::apply_config` (`options.rs`), which runs each as
    `:set` does (global and local value, `Target::Both`), so a document made
    later copies them. Then `--start-menu` or `--no-start-menu`, when given,
    overrides `[start] menu`. The file is not watched or reloaded.
  - Warnings: kept in the editor (`Editor::set_config_warnings`). The startup
    message is the intro (the short modified-version notice, `NOTICE_SHORT`,
    which the Vim license asks to keep in the intro screen) with ` [N config
    warnings]` appended (`[1 config warning]` for one), so the notice is
    never replaced. The note does not name `:ConfigWarnings`, to fit the
    message line; this file and the README say that `:ConfigWarnings` lists
    them. With the start menu on, the menu draws the full notice and the
    message line shows only the note. A file given on the command line keeps
    its info message and gets the note too. `:ConfigWarnings` (rvim only,
    capital letter) lists every warning on the message line, joined with ` |
    `, or says `No config warnings`. There is no general message history.
  - Flags: `-u NONE` and `--clean` skip the file. `-N` is accepted and does
    nothing (rvim is always Vim's `-N`). Any other `-u` value is a usage error
    (exit 2), since rvim reads no other vimrc. `--config <file>` is not added:
    the plan does not ask for it.
  - Without a config file, or with `-u NONE`, the options are the defaults
    (test `no_config_leaves_the_options_at_their_defaults`).
  - Modern profile: the plan's rule that a config file turns on the modern
    profile and the start menu is not implemented yet. A config file does not
    change the start menu by itself; only `[start] menu` does.
- **Discoverability**: a command palette (fuzzy search over Ex commands,
  options and rvim commands), `:help` covering what rvim implements,
  which-key style hints for pending operators and prefixes, and Ex
  completion (`<Tab>` on commands, options, file names).
- **Mouse**: click to place the cursor, drag for Visual, wheel to scroll.

### Line numbers (done)

- Options `number` (`nu`), `relativenumber` (`rnu`) and `numberwidth` (`nuw`,
  default 4, kept within 1 to 20 without an error, as Vim does). All three
  are window-local (see "Options registry"). Off by default, so with no
  config the frame is unchanged.
- The column is Vim's `number_width()` plus one space: the digits of the line
  count, or of the window height when only 'relativenumber' is set, and never
  narrower than 'numberwidth'. It widens past 999 lines
  (`Editor::number_col_width()` in `view.rs`).
- The text area is the window minus the column (`Editor::text_width()`).
  Wrapping, the cursor cell and the line heights in `scroll.rs` use it, so
  scrolling, `H M L` and 'scrolloff' see the narrower area.
- The first row of a line shows its number; the later rows of a wrapped line
  and the `~` and `@` rows show blanks. With 'number' and 'relativenumber'
  together, the cursor line shows its absolute number left-aligned; with only
  'relativenumber' it shows `0`.
- Styles: `Hl::LineNr` and `Hl::CursorLineNr`. The colours in `main.rs` are
  rvim's theme (dim grey and yellow), not Vim's defaults.
- Tests: `tests/line_numbers.rs` (Frame rows and cursor, wrapped rows, the
  narrower scroll, widening past 999 lines, styles, `:set nu?`, `:setlocal`,
  `:set invnu`, `numberwidth` and its clamp, and the default output). The
  column rules and the `numberwidth` clamp and scope were probed with
  `vim -es`. Vim run that way has no screen, so the drawn layout is checked
  against rvim's own Frame, not against Vim's screen.

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
  file `$XDG_STATE_HOME/rvim/recent`; the Config entry (the config file can be
  read now, but the entry that opens it is not built); the modern-profile
  default (menu on with `--modern`); the agent line (milestone 7); `:help`,
  which enables the Help entry; the logo (the word `rvim` in a box stands in
  for it). The config switch `start.menu` is done (see "Config").

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
  | Cursor | `agent` (docs); `cursor-agent` is registry-confirmed; Unix `~/.local/bin/agent`; install `curl https://cursor.com/install` | Native ACP: `agent acp`, newline-delimited JSON-RPC on stdio. Auth method `cursor_login` after `agent login` or `CURSOR_API_KEY`. | Extension methods `cursor/ask_question`, `cursor/create_plan`, `cursor/update_todos`, `cursor/task`. Headless: `agent -p`, `--output-format stream-json`. `agent` is a generic process name, so detect by path, not by name alone. |
  | Kimi Code | `kimi`; new CLI `kimi-code` (config `~/.kimi-code`, `KIMI_CODE_HOME`); legacy `kimi-cli` (config `~/.kimi`, archived September 2026) | Native ACP: `kimi acp` (user must `/login` first) | Windows needs Git for Windows. `kimi -p` is headless but auto-approves tool calls (opt-in only). `kimi web` is a local server on 127.0.0.1:58627 (API UNVERIFIED). Detect both config directories. |
  | Others via the ACP registry | per entry | ACP | An ACP agent registry exists (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`, about 58 agents including Gemini CLI `gemini --acp`, Goose, OpenCode, Qwen Code, Copilot CLI). Detection can read it, so new agents need no code. |

- **Verification, 2026-11-01 (step 4a; the registry in `src/agent_detect.rs` follows it).**
  The sources were read with a web fetch, official docs and the ACP registry only:
  - Claude Code setup, `https://code.claude.com/docs/en/setup` (the `docs.anthropic.com`
    address redirects there). Confirmed: the executable `claude`; the native
    install in `~/.local/bin/claude` and `%USERPROFILE%\.local\bin\claude.exe`;
    the config in `~/.claude` and `~/.claude.json`. Not on the page, so not
    verified: `CLAUDE_CONFIG_DIR`, `~/.claude/local` and `%APPDATA%\npm`
    (kept as probes, marked `verified: false`).
  - Cursor CLI, `https://cursor.com/docs/cli/overview` and `https://cursor.com/docs/cli/acp`.
    Confirmed: the executable `agent`; install by `curl https://cursor.com/install`
    or `irm 'https://cursor.com/install?win32=true' | iex`; `agent acp` with
    newline-delimited JSON-RPC on stdio; auth method `cursor_login` (`agent login`,
    `CURSOR_API_KEY`, `CURSOR_AUTH_TOKEN`); blocking extension methods
    `cursor/ask_question` and `cursor/create_plan`; notifications `cursor/update_todos`,
    `cursor/task` and `cursor/generate_image`. `cursor-agent` is confirmed by the ACP registry (read 2026-11-01). Not verified: the Unix install location `~/.local/bin/agent` (the page gives neither).
  - Codex CLI, `https://learn.chatgpt.com/docs/codex/cli` (the `developers.openai.com`
    address redirects there). Confirmed: the executable `codex`; `codex exec`
    for scripts; an app server exists, with no launch command given. Not verified:
    `~/.codex` and `CODEX_HOME` (the page does not name a config directory), and
    the app-server launch command.
  - Kimi Code CLI, `http://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started`
    (redirected from the older `kimi.com/code/docs/en/kimi-cli` address) and
    `https://github.com/MoonshotAI/kimi-cli`, which was archived 2026-09-23.
    Confirmed: the executable `kimi`; install from `code.kimi.com/kimi-code/install.sh`
    and `install.ps1` or `npm install -g @moonshot-ai/kimi-code`; the config in
    `~/.kimi-code/` or under `KIMI_CODE_HOME`; the legacy `~/.kimi`, which the
    kimi-cli migration reads; `kimi acp`; login by `/login` in the UI.
    Correction: `kimi-code` is the npm package, not the executable. Not verified:
    whether `kimi -p` auto-approves (it is not used), and the installer location
    (no install directory is probed).
  - ACP registry, `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`
    (version 1.0.0, 43 entries), and `https://agentclientprotocol.com/get-started/agents`.
    `claude-acp` is `@agentclientprotocol/claude-agent-acp@0.99.1`; `codex-acp` is
    `@agentclientprotocol/codex-acp@2.5.0`; `kimi` runs `kimi acp`; `cursor` runs
    `cursor-agent acp` from its package (a package path, not a PATH name); `gemini`
    runs `@google/gemini-cli@0.66.0 --acp`. Correction: the `@zed-industries`
    adapter names in the table above are the older names.

  The registry as built (`AGENTS` in `src/agent_detect.rs`; V = verified, U = unverified):

  | id | Executable names | Install dirs probed | Config dirs probed | Protocols | ACP command |
  |---|---|---|---|---|---|
  | `claude` | `claude` (V) | `~/.local/bin` (V); `~/.claude/local` (U); `%APPDATA%\npm` (U) | `~/.claude` (V), `~/.claude.json` (V), `CLAUDE_CONFIG_DIR` (U) | acp-adapter (V); claude-ide (U); files-only | `npx -y @agentclientprotocol/claude-agent-acp@0.107.0` (Node 22+; Windows: `npx` is retried as `npx.cmd`) |
  | `codex` | `codex` (V) | `%APPDATA%\npm` (U) | `~/.codex` (V), `CODEX_HOME` (U) | acp-adapter (V); files-only | `npx -y @agentclientprotocol/codex-acp@2.6.0` (`-y` added 2026-11-13: without it npx can stop to ask) |
  | `cursor` | `agent` (V, path must contain "cursor"); `cursor-agent` (V, registry; same rule) | `~/.local/bin` on Unix (U) | none known | acp (V); files-only | `agent acp` (V) |
  | `kimi` | `kimi` (V) | none (installer location not stated) | `KIMI_CODE_HOME` (V), `~/.kimi-code` (V), `~/.kimi` (V, legacy) | acp (V); files-only | `kimi acp` (V) |
  | `gemini` | `gemini` (V) | none | none | acp (V); files-only | `npx -y @google/gemini-cli@0.68.0 --acp` (V, registry) |
  | `github-copilot-cli` | `copilot` (V) | none | none | acp (V); files-only | `npx -y @github/copilot@1.0.110 --acp` (V, registry) |
  | `opencode` | `opencode` (V, registry command) | none | none | acp (V); files-only | `opencode acp` (V, registry) |
  | `goose` | `goose` (V, registry command) | none | none | acp (V); files-only | `goose acp` (V, registry) |
  | `qwen-code` | `qwen` (V) | none | none | acp (V); files-only | `npx -y @qwen-code/qwen-code@0.27.3 --acp --experimental-skills` (V, registry) |
  | `antigravity` | `agy` (V) | none | none | acp (V); files-only | `agy acp` (V; added 2026-11-13) |

  Versions in this table are the ACP registry's as read on 2026-11-13 (the
  2026-11-01 reading above is superseded). Each entry also has `login` (the
  agent's own sign-in command, shown when rvim has no method to offer) and
  `login_methods` (the `authenticate` ids that reuse the agent's login); see
  "Agent SDKs and subscriptions" below.

  Checked again 2026-11-01 (step 5b-1b): the Claude Code IDE documentation
  (`code.claude.com/docs/en/ide-integrations`, VS Code only) gives the loopback
  WebSocket on 127.0.0.1, the lock file `~/.claude/ide/<port>.lock` (or
  `$CLAUDE_CONFIG_DIR/ide/`) and the header `X-Claude-Code-Ide-Authorization`; the
  lock file's JSON schema, the handshake and the tool names stay UNVERIFIED, and
  the IDE protocol is not a supported third-party API. The installed executable
  names of Gemini CLI (`gemini`), GitHub Copilot CLI (`copilot`) and Qwen Code
  (`qwen`) are UNVERIFIED (from memory); their install and config locations are not
  probed. Still UNVERIFIED: the Codex config directory and app-server command,
  the Cursor and Kimi install locations, and whether
  `kimi -p` auto-approves. Others in the ACP registry (about 40 more entries) are
  not in the static table; reading the registry at run time is a later step.
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

### Agent SDKs and subscriptions (research 2026-11-13)

Hard requirement from the owner: rvim must work with the user's existing
**subscriptions** (Claude Pro or Max, ChatGPT Plus or Pro, Google AI Pro or Ultra,
GitHub Copilot, Cursor, Qwen, and so on), not only API keys. rvim itself never
reads, stores, refreshes or sends a credential. The only designs allowed are
"the user is already signed in through the agent's own CLI" and "the agent runs
its own sign-in when rvim asks it to". Sources are official docs, the adapters'
GitHub sources (read through the GitHub API) and npm, all read on 2026-11-13.
Secondary reports (news and blogs) are marked as such and decide nothing.

**What an ACP client sees.** `initialize` returns `authMethods`. A method has an
`id`, `name`, an optional `description` and a `type`: absent or `agent` (the
agent runs the flow after `authenticate {methodId}`), or `terminal` (the client
runs the agent's own program interactively with the method's `args` and `env`,
then reconnects; the client must declare `clientCapabilities.auth.terminal` to
get these, and must never send `authenticate` for them). The schema names
the error `auth_required`; the adapters throw it as `-32000`.
(`agentclientprotocol.com/protocol/v1/authentication`.) The page does not fix when
`authenticate` happens; the adapters read here (`codex-acp`, Gemini CLI, `agy acp`)
raise it from `session/new`, which is the order `acp.rs` already uses.

#### Claude Code (Claude Pro, Max, Team, Enterprise)

| | |
|---|---|
| Route | ACP adapter `@agentclientprotocol/claude-agent-acp` 0.107.0 (npm and ACP registry id `claude-acp`, published 2026-11-13; bin `claude-agent-acp`; Apache-2.0). Launch: `npx -y @agentclientprotocol/claude-agent-acp@0.107.0` (Node 22+). Standalone archives with a bundled `claude` are on its GitHub releases. Env: `CLAUDE_CODE_EXECUTABLE` (use your own `claude`), `CLAUDE_AGENT_ACP_PROTOCOL` (`v1`, `v2` or `auto`, default `auto`). `github.com/agentclientprotocol/claude-agent-acp` |
| Other routes | Claude Agent SDK (TypeScript and Python; it runs the `claude` binary, not an ACP route); `claude -p --output-format stream-json` (headless; `--bare` ignores the subscription login); the IDE lock-file server (`~/.claude/ide/<port>.lock`, documented for VS Code only, "relying on it would mean depending on internal behavior": `code.claude.com/docs/en/ide-integrations`); no native `claude acp` exists (no ACP page in `code.claude.com/docs/llms.txt`) |
| Subscription | **PARTIAL.** Technically yes: the adapter runs the Agent SDK, which runs `claude` with the login `claude auth login` stored (credential order: cloud vars, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, `apiKeyHelper`, `CLAUDE_CODE_OAUTH_TOKEN`, profiles, then the `/login` subscription; `code.claude.com/docs/en/authentication`). Policy is not clear (below) |
| authMethods | Only when the client declares terminal auth (or the `_meta` `terminal-auth` extension); otherwise an empty list. All are `terminal`: `claude-ai-login` "Claude Subscription" (args `--cli auth login --claudeai`; local), `claude-login` "Log in with Claude" (args `--cli`; chosen when `NO_BROWSER`, `SSH_CONNECTION`, `SSH_CLIENT`, `SSH_TTY` or `CLAUDE_CODE_REMOTE` is set), `console-login` "Anthropic Console" ("Use Anthropic Console (API usage billing)", args `--cli auth login --console`). Source `src/auth-methods.ts` on main. No `agent` method exists, so `authenticate` is never how Claude signs in |
| Not signed in | `session/new` or `session/prompt` fails with `-32000`: the adapter turns the CLI's "Run /login" messages into `authRequired` (CHANGELOG, `message-filters.ts`). A flag `--hide-claude-auth` makes it refuse subscription accounts (`src/hide-claude-auth.ts`); it is off by default |
| Caveats | The user needs the official `claude` login on the machine (the adapter's own binary shares `~/.claude`, or `$CLAUDE_CONFIG_DIR`). `ANTHROPIC_API_KEY` in rvim's environment outranks the subscription and bills the key, so rvim never sets it and the docs tell the user to unset it. The branding rules for SDK partners allow "Claude Agent" and not "Claude Code" in a product's UI (`code.claude.com/docs/en/agent-sdk/overview`); the registry's display name stays `Claude Code` only because it names the user's installed CLI |

Policy, quoted from Anthropic (primary): `code.claude.com/docs/en/legal-and-compliance`
says OAuth "is intended exclusively for purchasers of Claude Free, Pro, Max, ..."
subscriptions and "ordinary use of Claude Code and other native Anthropic
applications"; developers building products, "including those using the Agent
SDK, should use API key authentication"; "Anthropic does not permit third-party
developers to offer Claude.ai login into their own applications, or to route
requests through Free, Pro, Max, or Ultra plan credentials on behalf of their
users"; developers "may not collect, store, or intermediate Claude.ai
credentials or session tokens - sign-in to a Claude account must complete
through Anthropic's own flow." The same page says this "does not prevent an end
user from signing in to the unmodified Claude Code binary with their own Claude
subscription". The Agent SDK overview says: "Unless previously approved,
Anthropic does not allow third party developers to offer claude.ai login or rate
limits for their products, including agents built on the Claude Agent SDK."
The help article `support.claude.com/en/articles/15036540` (updated after
2026-10-07) says "Claude Agent SDK, claude -p, and third-party app usage still draw
from your subscription limits" and that its announced changes to SDK billing
are paused. News from February to May 2026 (secondary: The Register, Gigazine and
others) describes enforcement against third-party harnesses and a later reversal;
it conflicts and is not relied on.

Reading for rvim: rvim offers no Claude login, handles no credential and does not
resell anything; the user's own unmodified `claude` holds the login. That fits
the "end user signs in to the unmodified binary" sentence, and it is what Goose
documents for its `claude-acp` provider ("uses the same Claude subscription ...
needs an authenticated claude CLI", `goose-docs.ai/docs/guides/acp-providers`).
But it is a third-party app driving the Agent SDK, which the SDK note says
Anthropic does not allow without approval. **This is a decision for the owner.**
The route with no policy question is the terminal pane (section 7): run the
official `claude` itself in rvim's terminal and let rvim see its edits through
the file layer. rvim keeps the ACP route off until the user connects `claude`
explicitly (it already does: consent per agent and project).

#### Codex (ChatGPT Plus, Pro, Business, Enterprise)

| | |
|---|---|
| Route | ACP adapter `@agentclientprotocol/codex-acp` 2.6.0 (npm, registry id `codex-acp`). Launch: `npx -y @agentclientprotocol/codex-acp@2.6.0` (the `-y` was missing). It starts the Codex app server itself and bundles a compatible `@openai/codex` (`CODEX_PATH` overrides). `github.com/agentclientprotocol/codex-acp` |
| Direct route | `codex app-server`: JSON-RPC over stdio, one JSON message per line (default); `--listen ws://127.0.0.1:4500` (experimental, unsupported) or `unix://`. `initialize {clientInfo: {name, title, version}}` then the `initialized` notification, one `initialize` per connection. `thread/start` (`model`, `cwd`, `approvalPolicy`, `sandbox`), `turn/start {threadId, input[]}`, streamed `item/*` and `turn/*` notifications until `turn/completed`. Approvals are server requests `item/commandExecution/requestApproval` (`accept`, `acceptForSession`, `decline`, `cancel`, `acceptWithExecpolicyAmendment`) and `item/fileChange/requestApproval`. Auth: `account/read {refreshToken}` returns `{account, requiresOpenaiAuth}`; `account/login/start` with type `apiKey`, `chatgpt` (returns `authUrl`), `chatgptDeviceCode` (returns `verificationUrl`, `userCode`) or experimental `chatgptAuthTokens`; then `account/login/completed` and `account/updated`; `account/logout`. `learn.chatgpt.com/docs/app-server` (the `developers.openai.com` address redirects there). Also `codex exec` (headless) |
| Subscription | **YES** for a local app. ChatGPT sign-in is a first-class method; the CLI and IDE extension share one cached login in `~/.codex/auth.json` or the OS credential store (`cli_auth_credentials_store`: `file`, `keyring`, `auto`, `ephemeral`), refreshed automatically; `codex login`, `codex login --device-auth`, `codex login status`, `codex logout` (`learn.chatgpt.com/docs/auth`). Limit: "App-server authentication has never been permitted for commercial or hosted services"; existing local or open-source apps may continue to use it, and OpenAI recommends its Sign in with ChatGPT program for anything else (app-server page, "Auth endpoints") |
| authMethods | From `src/CodexAuthMethod.ts` on main, all `agent` type (none is terminal): `api-key` "API Key" ("Use an API key to authenticate"; always, listed first), `chat-gpt` "ChatGPT" ("Use ChatGPT to authenticate"; not when `NO_BROWSER` is set), `chat-gpt-device-code` "ChatGPT (device code)" (only if the client declares URL elicitation), `gateway` "Custom model gateway" (only if `clientCapabilities.auth._meta.gateway` is true) |
| What `authenticate` does | `chat-gpt`: reads the account with a token refresh; if a working ChatGPT login exists it returns success and starts no login (this is "already logged in through `codex login`"); otherwise it starts the browser login and the adapter opens the browser itself. `chat-gpt-device-code`: same check, then needs URL elicitation to show the code (else `invalidRequest`). `api-key`: the key from `_meta["api-key"]` or the agent's environment `CODEX_API_KEY`, then `OPENAI_API_KEY`; it always logs in with the key. `gateway`: stores client-supplied routing, no login. Without a login, `session/new` throws `authRequired` unless `DEFAULT_AUTH_REQUEST` is set (`src/CodexAcpServer.ts`, `src/CodexAcpClient.ts`) |
| Caveats | `chat-gpt` is the method rvim offers first. An API key from the environment bills the API, not the plan; rvim lists `api-key` after the ChatGPT methods and never supplies a key. Plans: Codex is not in Free or Go (OpenAI help article, secondary summary; the page itself returned 403) |

#### Gemini CLI and Antigravity CLI (Google AI Pro and Ultra, Code Assist)

| | |
|---|---|
| Gemini CLI route | `@google/gemini-cli` 0.68.0 (npm latest 0.68.0 of 2026-11-10; registry id `gemini`). `gemini --acp` over stdio JSON-RPC (`geminicli.com/docs/cli/acp-mode`, which names `--acp` only). Launch: `npx -y @google/gemini-cli@0.68.0 --acp` |
| Gemini CLI methods | From `packages/cli/src/acp/acpRpcDispatcher.ts` on main: `oauth-personal` "Log in with Google", `gemini-api-key` "Gemini API key", `vertex-ai` "Vertex AI", `gateway` "AI API Gateway"; all without a type. `authenticate` calls `refreshAuth`, which per its own comment "will reuse existing credentials if they're valid, or perform new authentication if needed"; a failure is `-32000` |
| Gemini CLI subscription | **PARTIAL, mostly NO for individuals.** Since 2026-06-18 Gemini CLI "will stop serving requests for Google AI Pro and Ultra" and Code Assist for individuals; it stays for Code Assist Standard and Enterprise licences and paid API keys (`developers.googleblog.com/en/an-important-update-transitioning-gemini-cli-to-antigravity-cli`; `geminicli.com/docs/get-started/authentication`). `geminicli.com/docs/resources/tos-privacy` says reaching Gemini through third-party software that uses the CLI's OAuth is "a violation of applicable terms" and "may be grounds for suspension" |
| Antigravity route | `agy acp` (Antigravity CLI, executable `agy`; docs: `antigravity.google/docs/cli/acp` and `/docs/ide/extensions/acp`). Sign in once with an interactive `agy` or `agy acp --login`; `agy acp` reuses the cached login, and without one answers `Authentication required` (`-32000`). Documented clients: Zed, JetBrains, Android Studio, Xcode (native), VS Code (community). The registry also lists `antigravity-acp` 1.5.0, a proprietary `agy_acp_server` download; not used |
| Antigravity methods | `oauth-personal` (Google personal), `oauth-business` (Gemini Enterprise), `gemini-api-key`, `agent-platform` (Gemini Enterprise Agent Platform), `terminal-login` (only for clients that support terminal auth) |
| Antigravity subscription | **YES (inferred).** It is the product Google moved AI Pro and Ultra users to, and "available to everyone"; no page read says which plan entitles which use. The registry entry is proprietary (`antigravity.google/terms`); a Google forum thread asks whether wrapping `agy` for editors is allowed (secondary, unanswered in what was read). What `authenticate` does for each method is UNVERIFIED |
| Caveats | The registry adds `antigravity` (`agy acp`) and keeps `gemini` for licence and key users. The Gemini CLI's own docs warn that Google One and unpaid users must use Antigravity |

#### GitHub Copilot CLI (Copilot subscription)

| | |
|---|---|
| Route | `@github/copilot` 1.0.110 in the registry (npm latest 1.0.111), executable `copilot`, `copilot --acp` (stdio default; `--port` for TCP). Public preview, "subject to change" (`docs.github.com` Copilot CLI ACP server page). Install: `npm install -g @github/copilot` (Node 22+), `winget install GitHub.Copilot`, `brew install --cask copilot-cli`, `curl -fsSL https://gh.io/copilot-install \| bash`. Launch: `npx -y @github/copilot@1.0.110 --acp` |
| Subscription | **YES.** "An active GitHub Copilot subscription" is the prerequisite; sign in with `copilot login` or `/login` (browser flow, device code on remote hosts). The token goes to the OS keychain (service `copilot-cli`) or a plaintext fallback `~/.copilot/config.json`. BYOK providers can run "without GitHub login" |
| authMethods | The ACP page lists none: UNVERIFIED what `initialize` returns |
| Caveats | `COPILOT_GITHUB_TOKEN`, then `GH_TOKEN`, then `GITHUB_TOKEN` "silently override" the stored login, and classic `ghp_` tokens are not supported: rvim must not set them, and a user who has one set will get that account |

#### Cursor CLI (Cursor plans)

| | |
|---|---|
| Route | `agent acp` (docs; newline-delimited JSON-RPC on stdio), `cursor-agent acp` in the registry (id `cursor`, version 2026.11.11, a binary package). Install: `curl https://cursor.com/install` (Windows `irm 'https://cursor.com/install?win32=true' \| iex`) |
| Subscription | **PARTIAL (not stated).** Sign-in is the Cursor account; the ACP page does not name plans |
| authMethods | `cursor_login`. The docs say to sign in first with `agent login`, or `--api-key` / `CURSOR_API_KEY`, or `--auth-token` / `CURSOR_AUTH_TOKEN`; they do not say whether `authenticate` reuses that login or opens a browser, nor what an unauthenticated agent returns: UNVERIFIED |
| Caveats | Extension methods `cursor/ask_question` and `cursor/create_plan` block until answered; rvim answers unknown requests with `-32601`, which a Cursor turn may not survive: check with the real agent |

#### The rest

| Agent | Route and version | Subscription | Methods and caveats |
|---|---|---|---|
| OpenCode | `opencode acp` (registry 2.2.7); `opencode.ai/docs/acp` | YES through its providers: `/connect` offers "Sign in with ChatGPT" (Plus or Pro plan) and GitHub Copilot (device OAuth); Claude Pro and Max are not mentioned (`opencode.ai/docs/providers`) | ACP page says nothing on auth: UNVERIFIED if it advertises methods |
| Goose | `goose acp` (registry 1.59.0) | YES through providers: `goose configure` offers ChatGPT Codex (browser OAuth) and GitHub Copilot (device flow); its `claude-acp` and `codex-acp` providers reuse the logged-in CLIs | Methods UNVERIFIED |
| Qwen Code | `qwen`, `npx -y @qwen-code/qwen-code@0.27.3 --acp --experimental-skills` (registry) | YES: `/auth` offers a Qwen account with a Plus or Pro subscription, or the Alibaba Cloud Coding Plan (fixed monthly fee) | The README names no `--acp` flag; the registry does. Free OAuth tier status UNVERIFIED |
| Kimi Code | `kimi acp` (the executable `kimi`); the registry has `kimi-code` 2.6.1 (`npx -y @moonshot-ai/kimi-code@2.6.1 acp`) and `kimi` 1.52.0 (legacy binary) | PARTIAL: `/login` offers "Kimi Code" OAuth (device code) or a platform API key; the docs do not name a membership | ACP auth behaviour UNVERIFIED |

#### What changed in the code (2026-11-13)

- `src/agent_detect.rs`: the registry follows the table above. Versions are the
  registry's of 2026-11-13. Codex now has `-y` (without it `npx` can stop to
  ask whether to install and the connector would wait). `gemini`, `copilot`
  and `qwen` executable names are now verified; `~/.codex` is verified. A new
  agent `antigravity` (`agy acp`). New fields `login` (the agent's own sign-in
  command, shown as "codex-acp: not logged in; run `codex login` in a shell,
  then :AgentConnect") and `login_methods` (the `authenticate` ids that reuse
  the agent's own login).
- `src/acp.rs`: `AuthMethod::is_agent_flow` and `is_offered`. Only methods
  with no type or type `agent` are sent to `authenticate` (a type this client
  does not know is refused, as `terminal` was), and a method `gateway` is
  refused because it needs data only a client supplies. A failed `authenticate`
  now reports the failure and then offers the methods again.
- `src/agent_session.rs`: the sign-in choice. When `session/new` (or a prompt)
  fails with auth required and the agent advertised methods rvim may send, the
  process is kept, the indicator reads `ai: codex login?` and the message line
  shows `codex-acp: not logged in. Sign in with: [1] ChatGPT [2] ChatGPT (device
  code) [3] API Key [n]ot now`. The registry's `login_methods` come first,
  then the others, nine at most; the agent's names have control characters
  removed. `1` to `9` send `authenticate` with that method (the agent then uses
  its stored login or opens its own browser flow, and the connection continues
  with `session/new`); `n`, `<Esc>` and `<C-c>` send nothing, stop the process and
  show the login hint; other keys are ignored while the choice is up. With no
  method to offer (the Claude adapter's list is empty or all `terminal`), the
  process is stopped as before and the message gives the registry hint. A choice
  is never made for the user and no request is sent without a key. `:AgentConnect`
  while a choice is up shows it again; `:AgentDisconnect` drops it.
- Tests: `tests/agent_auth_choice.rs` (the editor flow with an in-process agent
  advertising Codex's and Claude's real method lists, the registry commands),
  `tests/acp_connector.rs` (a new transcript `auth_failed_then_retry.jsonl`, the
  offered-method rule, a method type the client does not know),
  `tests/agent_detect.rs` (the registry names).
- Known leftover: `src/agent_turn.rs` still logs
  `[error] not logged in; log in with the agent's own tool, then :AgentConnect` for
  an auth failure during a turn, even when a choice is on the message line. That
  file belonged to another change; align the text there.

#### UNVERIFIED

- Whether Anthropic accepts a third-party editor driving the Agent SDK with a
  user's own subscription login (the primary documents conflict as quoted above).
  Needs the owner's decision or Anthropic's written answer.
- The method ids in `login_methods` come from adapter sources on `main` (Codex:
  `chat-gpt`; Gemini: `oauth-personal`), not from their release tags, and not
  from a real `initialize`. No real agent has been run by this work.
- What `authenticate` does for Cursor `cursor_login`, Antigravity
  `oauth-personal` and `oauth-business`, and whether OpenCode, Goose, Qwen, Kimi
  and Copilot advertise `authMethods` over ACP.
- The Copilot token lifetime and what ACP returns without a login.
- Codex plan names that include Codex (the help article could not be fetched).
- The Codex app-server's exact JSON-RPC envelope (whether it carries a
  `jsonrpc` field), and `codex mcp-server` (not read).
- Whether `npx` leaves the agent's own `node` child running after rvim kills the
  launcher on Windows (still open from step 5b-1).
- That `claude-agent-acp` sees a login made by `claude auth login` under a
  `CLAUDE_CONFIG_DIR` that rvim does not set. (It inherits rvim's environment.)

#### Recommended order for the remaining connector work

1. Run the ACP connector against the real `codex-acp` (the best documented
   subscription route: a login that is reused, no terminal step), then
   `agy acp`, `copilot --acp` and `claude-agent-acp`, as a manual checklist.
   Record the real `initialize` replies as new fixtures, and fix the
   UNVERIFIED items above.
2. Step 6 (permissions and `fs/*`): real agents ask for permission on the first
   write; the connector cannot be used before it.
3. Terminal auth: declare `clientCapabilities.auth.terminal`, run the method's
   program (the agent's own `--cli auth login` for Claude) in the terminal
   window, then reconnect. It needs the PTY of step 7, so until then the login
   hint stands. This is what lets a Claude subscription user sign in from
   inside rvim, still through Anthropic's own flow.
4. The terminal pane with the official `claude` and `codex` (section 7), which
   is the route with no policy question for Claude.
5. `:AgentUse` and several connections.
6. The Claude IDE server (`claude-ide`), only after its protocol is checked
   against a real install.
7. Codex through `codex app-server` directly: only if the adapter proves
   limiting. Do not build a second connector before that; the notes above are
   the plan (`initialize`, `initialized`, `account/read`, `account/login/start`,
   `thread/start`, `turn/start`, the two approval requests).
8. Reading the ACP registry at run time, so new agents need no code.

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

Config (the opt-in; the loader is in place, see "Config" under "Editing
experience". `[agent] enabled` is read (step 4b, with the scan at startup); the keys below are not yet
interpreted):

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
| Connecting | Process started or socket opened; `initialize`, `authenticate`, `session/new` in progress. Also the brief stage after approval. While the sign-in choice is on the message line (see "Agent SDKs and subscriptions"), the text is `ai: claude login?` | `ai: claude connecting` |
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
confirmed from the ACP schema v1 (step 5a, see "ACP connector core"): option
kinds, the selected and cancelled outcomes, stop reasons, tool-call kinds,
statuses and content, `resource_link` and the fs shapes. The items still
UNVERIFIED there (the authenticate timing, the file URI of a selection, the
tool-call location tagging) and everything about a real agent remain UNVERIFIED
until tested against one.

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

File watcher (worker, milestone 7 step 2b, built): polls mtime and size of the
open files every 750 ms (`POLL_INTERVAL`, `src/watch.rs`). When a file's mtime
or size differs from its last look, the worker reads it and hashes it, unless
it is larger than 64 MiB (`MAX_WATCH_BYTES`): then it reads nothing and sends
`Event::FileTooLarge`. Same contents as the synced state (an echo of our write,
or a touch): nothing. Other contents: `Event::FileChanged` with the bytes, mtime,
size, hash and the document version the watcher was told about. A file that is
gone (and was on disk) sends `Event::FileDeleted`. The editor decides with the
same function as `:checktime` (`external_change_decision`, `Editor::check_disk`).
A file too large to read is never loaded automatically: it is marked stale and
the user is told to use `:e!`. Every watcher reload is tagged `external`; the
`agent:<id>` tag comes with the connector (step 5). Focus is not a trigger,
because rvim does not ask the terminal for focus events. `:checktime` polls at
once. With the option `filewatch` off (the default), the watched set is empty
and the thread never reads a file.

When the editor applies it:

| Document | Mode | Result |
|---|---|---|
| Unmodified since the last sync | Normal, no pending keys | Reload as one undo step (below). Applied at once |
| Unmodified | Any other mode | Queued; applied on return to Normal mode. Message: `x.rs changed on disk; handled when you return to Normal mode` (a change to a modified buffer then warns, W12, as in the row below) |
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
| Not logged in | auth-required (`-32000`) on `session/new` or a prompt | With no method rvim may send: `claude-acp: not logged in; <login hint from the registry>, then :AgentConnect`. With methods: `codex-acp: not logged in. Sign in with: [1] ChatGPT [2] API Key [n]ot now` (process kept; `n` or `<Esc>` sends nothing and shows the hint). A failed `authenticate` shows its reason and the choice again | Error after `n`, or at once when there is nothing to offer; `:AgentConnect` after login |
| Request failed during a turn (network, service) | JSON-RPC error on the prompt | `claude-acp: request failed: <agent's message>` | Idle; the prompt can be sent again |
| Slow turn | none; no timeout on a turn | `ai: claude working 42s` | Working; `\ac` |
| Malformed JSON from the agent | parse error | line logged and skipped; 10 in a row: disconnect | Disconnected |
| Agent write fails (`fs/write_text_file`) | IO error | JSON-RPC error to the agent; message `write failed: <reason>` | unchanged |
| rvim panics | panic hook | recovery files (existing) | agent processes stop when their pipes close |
| Terminal closed (SIGHUP) or SIGTERM on Unix | `signals.rs` event; exit 129 or 143 | `rvim stopped by SIGHUP. Unsaved text was saved to <path>` | recovery file; agent processes stop when their pipes close |

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
`agent/ide.rs` (IDE server), `watch.rs` (file watcher, worker; was planned as `filewatch.rs`), `reload.rs` (line diff,
cursor and scroll mapping, undo step), `review.rs` (baseline, gutter, diff
view model). `main.rs` only wires threads and draws.

This order refines "Suggested order" in the Status section. Only step 1 is a
prerequisite for the file reload (steps 2 and 3), and the connector (step 5)
needs no splits. Splits gate step 7 alone.

| Step | Lands | Depends on | Tests |
|---|---|---|---|
| 1 | Document/Window split (milestone 4, item (a)) | none | Existing compat suite unchanged. Unit: two windows on one document share undo and marks, keep their own cursors |
| 2 | **2a done (2026-11-01):** undo step source and hash, `checktime`, `autoread`, reload as one undo step with cursor and scroll mapping (`reload.rs`, `Editor::apply_external_change`), echo suppression by hash and by the stat taken after `:w`. **2b done (2026-11-01):** file watcher (`src/watch.rs`: worker thread, stat polling every 750 ms, `WatchSet` and `poll` pure over a `FileSystem` trait), `FileChanged`, `FileDeleted` and `FileTooLarge` events, echo suppression by hash, the watched set sent by `Editor::take_watch_commands` (empty unless `filewatch` is on), resync when an event is dropped, and the option `filewatch`. Step 2 is complete | 1 (per-document version) | Unit: mapping table (equal, inserted above, deleted under cursor, replaced, deleted top line); `u` and `<C-r>` after reload. Scripted watcher with fake clock and fake file contents. Echo: a `:w` does not reload itself. Differential: `autoread` value and `:checktime` with no change. Add the cursor-placement difference to "Known differences" |
| 4 | **4a done (2026-11-01): the registry and the detection core (`src/agent_detect.rs`; tests in `tests/agent_detect.rs`). 4b done (2026-11-01):** `:AgentScan` (`src/agent_scan.rs`; the scan runs on a worker and comes back as `Event::AgentScan`), the status indicator for the states without a connection (`ai: files only`, `ai: none`, `ai: <ids> found`; drawn only after the opt-in or `:AgentScan`), and the `[agent] enabled` opt-in (`config.rs`; `shareable` and `log` warn as not supported yet). Tests: `tests/agent_scan.rs` and the unit tests in `config.rs` | minimal config loader (shared with milestone 6), options | Detection with a temporary fake home and `PATH`: found, not found, found under the config dir only. Frame golden for each indicator. Golden with no opt-in and no Agent command: no indicator, identical to Vim's frame. Golden after `:AgentScan` with no opt-in: indicator shown |
| 3 | **3a done (2026-11-01):** the conflict prompt (`l k d m`, with `<Esc>` keeping the buffer as `k` does; `d` says the diff view is not built yet), the per-document baseline and the `seen` state, the `:w` guard, the deleted-file message and the merge (`src/merge.rs`), all behind the opt-in, in `src/conflict.rs`. **Left (3b):** the gutter, the `[base]` scratch document, the `d` diff view (it needs a scratch document, so it waits for splits and step 7), `:AgentChanges`, and the `[deleted]` mark in the status line. Scope as first planned: Conflict prompt (`l k d m`) with the opt-in, baseline (`[base]` scratch) and gutter, `:w` W11/W12 guard, deleted file, `:AgentChanges`, merge with markers (marker refusal of `:w` with the opt-in only) | 2 | Unit: each conflict key on a fixed state; merge (clean, conflicting, identical); Frame golden of the gutter and of the prompt line. With no opt-in: a modified buffer is not reloaded and `:w` gives W11. No differential (messages) |
| 5 | **5a done (2026-11-01): the pure protocol core, see "ACP connector core". 5b-1 done (2026-11-01): the process, consent, approval store, `:AgentConnect`, `:AgentDisconnect`, `:AgentCancel`, lifecycle and backoff (see "ACP connector, step 5b-1"). 5b-1b done (2026-11-01): the reader liveness fix, the retry approval check, the paste and prompt-precedence tests, and the registry additions (see "ACP connector, step 5b-1" and "Detecting and connecting to agents"). 5b-2 done (2026-11-01): `:Agent`, the agent log, `\a` `\al` `\ac`, `:AgentLog`, prompt turns; left: `:AgentUse`, several connections (see "ACP connector, step 5b-2"). 5b-3 done (2026-11-13): the sign-in choice on the message line for agents that advertise methods rvim may send, the login hint from the registry, corrected registry commands (see "Agent SDKs and subscriptions"); left in 5: terminal sign-in methods (PTY, step 7), `:AgentUse`.** ACP connector: consent, approval store, `:Agent`, `:AgentConnect`, `:AgentUse`, `:AgentCancel`, `:AgentDisconnect`, agent log document, `\a` and `\al`, lifecycle and backoff, `Editor::prediction_allowed` (returns false) | 4 | Fake agent (test helper binary) replays recorded JSON-RPC transcripts both ways: success, auth failure, reply timeout, EOF mid-turn, malformed line. Pure transition table test. Fake timer for backoff. Approval store round trip and unreadable file. Consent keys. `:AgentUse` with two fake agents: listing, switching, and the choice prompt when several are found and none is active. Cases: `:Agent` without opt-in still works, keys stay unbound |
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

### ACP connector core (step 5a, 2026-11-01)

What was read, and from where. Protocol: ACP v1 (`ProtocolVersion` is the
integer 1, "only bumped for breaking changes"). Pages read on 2026-11-01:
`agentclientprotocol.com/protocol/overview`, `/protocol/transports`,
`/protocol/initialization`, `/protocol/session-setup`, `/protocol/prompt-turn`,
`/protocol/file-system` and `/protocol/v1/authentication`. (`/protocol/auth`
returns 404.) Schema: the rendered v1 reference at
`agentclientprotocol.com/protocol/v1/schema`, read as text. The raw JSON was not
downloaded (a sandbox check refused the command), so the field details come
from the rendered page. The upstream repository's `main` head was commit
`b4c72a8` (2026-10-31), where `schema/v1/schema.json` has blob `776c231`. The
v1 changelog's latest entry is schema package 1.28.0 (2026-10-30). The docs
name no protocol version string beyond the `v1` path. Framing is confirmed by
the transports page only, not by a real agent.

Implemented (client side only; `src/acp.rs`, no I/O):

- Framing: newline-delimited JSON-RPC 2.0, UTF-8, no embedded newlines.
  `LineDecoder` keeps partial lines, skips empty and whitespace-only lines,
  accepts CRLF, and rejects a line longer than 8 MiB (bytes before its newline),
  reporting it once and discarding the rest of that line. Invalid UTF-8, invalid
  JSON and non-JSON-RPC objects are reported per line.
- Requests sent: `initialize` (protocolVersion 1; `fs` and `terminal` all false,
  because file calls and terminals are not served before step 6),
  `session/new` (`cwd`, `mcpServers: []`), `authenticate` (`methodId`, never a
  terminal method), `session/prompt` (text blocks). Notification sent:
  `session/cancel`. `session/load` has its params type only; it is not called.
- Received requests: `session/request_permission` (queued; answered only by a
  `PermissionAnswer` event, or as `cancelled`), `fs/read_text_file` and
  `fs/write_text_file` (surfaced as `ServeRead` and `ServeWrite` data for the
  caller; the path check is step 6). Any other request is answered with
  `-32601` (method not found).
- Received notifications: `session/update` with `agent_message_chunk`,
  `agent_thought_chunk`, `user_message_chunk`, `tool_call`, `tool_call_update`
  and `plan`. An unknown kind, or a known kind whose body does not parse, is
  kept as `SessionUpdate::Other` with its raw JSON and logged, not shown. Other
  notification methods are logged and ignored.
- Responses: `stopReason` `end_turn`, `max_tokens`, `max_turn_requests`,
  `refusal`, `cancelled` (an unknown value parses as `Other`).

Not implemented in step 5a: `session/load`, `session/set_mode`, `logout`, the
newer `session/list`, `session/close` and `session/resume`, `terminal/*`,
`elicitation/*`, process spawning, the path sandbox, and the tagging of changes
as agent writes.

The state machine. `Conn` (a new one per connection attempt) takes `InEvent`s
and returns `OutAction`s. States (`ConnState`): `Disconnected` (before
`Connect` and after a lost connection), `Initializing`, `NeedsAuth`, `Ready`,
`Prompting { turn }`, `Cancelling { turn }`, `Closed` (final: handshake failed
or timed out, or the agent speaks another protocol version). Differences from
the list in the task: `Cancelling` carries the turn, and "waiting for
permission" is `Prompting` with a non-empty queue (`permissions_waiting()`), not
a state of its own. Events beyond the task's list: `Connect { cwd, now }`
starts the handshake, `Authenticate { method_id }`, and `FileReadAnswer` and
`FileWriteAnswer`, the caller's answers to `ServeRead` and `ServeWrite`. Time
is data: `Bytes` and `Tick` carry `now: Instant`, and the module never reads
the clock or sleeps. Output actions: `Send` (one encoded line), `Update`,
`Notice` (for the UI: connected, auth required, turn started and ended,
request failed, connection lost, failed), `AskPermission` (with the queue
length), `ServeRead`, `ServeWrite`, `Log` and `Close`.

Rules where the spec is silent or PLAN decides:

- A 10 s limit for the `initialize` reply (PLAN, section 10). The same limit is
  applied to the `session/new` reply (a choice, UNVERIFIED). A timeout ends in
  `Closed`. Nothing times out for `authenticate` or a turn.
- A protocol version other than 1 in the `initialize` reply ends in `Closed`.
  The spec says the client should disconnect.
- `session/new` failing with `-32000` (auth required) gives `NeedsAuth`. Only
  the user's choice of a non-terminal method (`Authenticate`) sends
  `authenticate`; success sends `session/new` again. The order of `session/new`
  and `authenticate`, and what triggers `authenticate`, are UNVERIFIED: the auth
  page gives no timing rule and names only the string `auth_required`.
- A malformed line is logged and skipped. Ten in a row lose the connection
  (`ConnectionLost`, then `Close`).
- Connection lost (EOF, or the ten-line rule) during a turn gives
  `ConnectionLost { turn }`. The permission queue is dropped and the turn is not
  resumed by a reconnect.
- Permissions are never answered by the machine except as `cancelled`: on
  `Cancel`, when a turn ends with requests still queued, or for a request that
  arrives during `Cancelling`, which is answered without asking. An option id
  that was not offered is logged and the request stays queued.
- Requests from rvim are numbered from 1 per connection. A response with an
  unknown id is logged. A `session/update` for another session is dropped.
- `Connect` outside `Disconnected` and `Prompt` outside `Ready` are refused
  with a notice.

Checked against the schema (these were UNVERIFIED in the overview research):
the option kinds `allow_once`, `allow_always`, `reject_once` and `reject_always`;
the outcome objects, sent as `result.outcome` with `"outcome":"selected"` and an
`optionId`, or `"outcome":"cancelled"`; the stop reasons; the tool-call statuses
`pending`, `in_progress`, `completed` and `failed`; the tool-call kinds; plan
priorities and statuses; tool-call content `content` and `diff`; `resource_link`
with `uri` and `name`; the embedded resource's `uri`, `text` and `mimeType`; the
fs parameters (absolute paths, 1-based `line`, `limit`) and the `{}` reply to
`fs/write_text_file`; and the error codes `-32000` (auth required), `-32002`
(not found), `-32601` (method not found) and `-32602` (invalid params).

Still UNVERIFIED:

- The order of `session/new` and `authenticate`, and the trigger for
  `authenticate` (see above).
- The number of the auth-required error for the wire. The schema says `-32000`;
  the auth page names `auth_required` without a number.
- The file URI convention for a selection (`file:///path#Lstart-end`).
- The tool-call `locations` rule for the change tag (step 6).
- Behaviour with a real agent. None of this has been tested against Claude Code,
  Codex, Cursor or Kimi. The fixtures in `tests/fixtures/acp/` are written by
  hand from the schema, each labelled with its spec section.

Tests (`tests/acp_connector.rs`, 23 tests): the eight transcripts, each replayed
with the agent's bytes split at every position and at every byte, with the same
actions and states required; a table of 34 state transitions; the permission
queue (never answered without an event; cancel answers all queued requests as
`cancelled`; an unknown option is refused); unknown kinds kept as raw JSON; the
8 MiB line cap, checked with a 8 MiB + 1 byte line in 1 MiB reads, and the exact
limit with a 64-byte cap; invalid UTF-8, invalid JSON and CRLF; and the
malformed-line rule (ten in a row, and nine).

### ACP connector, step 5b-1 (2026-11-01)

The design is in "Agent experience" sections 1, 2, 5 and 10. What is built:

- `src/agent_conn.rs`: the process, on worker threads. The editor sends
  `ConnCmd` (`Start`, `Send`, `Stop`, `SaveApprovals`); the worker sends back
  `AgentEvent`s (bytes, end with the exit code, spawn failure, a note), each
  tagged with the connection's generation. The agent is a local child with
  piped stdin and stdout; stderr is discarded. On Windows a command that is
  not found is retried with `.cmd` (for `npx`).
- `src/agent_session.rs`: the editor's side, one connection at a time.
  `:AgentConnect [name]` takes the name, or the one agent the last scan found,
  and refuses an agent with no ACP command in the registry. For an
  (agent id, project root) pair not yet approved it shows the consent prompt:
  `y` approves, saves and connects; `n` and `<Esc>` do nothing; the prompt
  takes every key. `:AgentDisconnect` stops the process rvim started and keeps
  the approval. `:AgentCancel` sends `session/cancel` when a turn runs and says
  so otherwise.
- Lifecycle: Connecting, Idle, Disconnected (with retry) and Error, as in
  "Lifecycle". A first connect that fails is Error. A lost connection is
  retried after 1, 2, 4, 8 and 16 seconds, then Error. A retry that fails in
  the handshake counts as one retry. A successful connect resets the count.
  The timers are `Editor::next_deadline` with `Event::Timer`, and the
  handshake timeout is `Conn::next_deadline` (new in `acp.rs`).
- `src/agent_approval.rs`: `agent-approvals.toml` beside `config.toml`, one
  `[[approval]]` table per (agent id, project root). `main.rs` reads it at
  startup, as it reads the config: a small local read. The worker writes it
  on approval, through a temporary file and a rename. An unreadable or invalid
  file means no approvals, with a warning on the message line. The file is
  then never overwritten, and the approval lasts for this session only.
- The indicator: `ai: claude connecting`, `ai: claude`, `ai: claude retry 2/5`
  and `ai: claude error`, drawn while the layer is active. Without a connection
  the step 4b states are shown as before.
- Messages: `claude-acp: connected`, ``claude-acp: not logged in; run `claude
  auth login` in a shell, then :AgentConnect`` (since 2026-11-13 the hint is the
  registry's `login` text, and an agent that advertises sign-in methods gets
  the choice instead: see "Agent SDKs and subscriptions"), `claude-acp: exited 1`,
  `claude-acp: could not start: <reason>`, `claude-acp: no reply to
  initialize after 10s`, `claude-acp: connection lost; retry 1/5`.
- `Editor::prediction_allowed()` returns false.

Not in 5b-1 (later steps): `:Agent` and prompt turns, the agent log and its
keys, `:AgentUse` and several connections, permission answers, file calls and
terminals. No `fs` or terminal capability is advertised yet, so a
well-behaved agent sends no file requests. If it sends a permission request
anyway, the turn waits, since no answer is given before step 6. Auth methods
that need a terminal are refused (`acp.rs`).

Not verified (UNVERIFIED): on Windows, killing an `npx` launcher may leave the
agent's own node process running and holding the pipe. Only the launcher is
killed. Check this with the real `claude-agent-acp` before relying on it.

Tests: `tests/agent_connect.rs` (20 tests) runs the connection through the
editor's events with an in-process fake agent. It answers the requests with
the ACP v1 shapes that `acp.rs` uses, not with recorded transcripts and not
with a test binary; no process is spawned. The tests cover success, auth
failure, EOF mid-start, spawn failure, malformed lines (ten in a row), the
handshake timeout on a fake clock, nothing spawned without approval, the
consent keys, the approval store (round trip, unreadable file, no overwrite),
a missing config directory, disconnect, cancel, the indicator states,
backoff through five retries with the stale-generation rule, and a retry that
succeeds. Unit tests: `agent_approval.rs` (2) and `agent_conn.rs` (1, a missing
program reported as spawn failure).

### ACP connector, step 5b-2 (2026-11-01)

What is built (`src/agent_turn.rs`, with the lines in `src/acp.rs`,
`src/agent_session.rs`, `src/normal.rs` and `src/ex.rs` that call it):

- `:Agent {text}` sends one `session/prompt` turn with the text as typed; `|`
  is part of the prompt (`Agent` is one of the commands `split_bar` leaves
  whole). `:Agent` with no text opens the prompt line with `Agent ` typed. One
  turn at a time: a second prompt during a turn says so. A first prompt with no
  connection starts the connection (consent first when not approved) and is
  sent when the connection is Idle; a consent `n`, a failed connection or a
  disconnect drops it.
- The agent log is a scratch document with no file (`Document::nomodifiable`).
  `:AgentLog` shows it in the window in place of the document (no splits yet);
  `:AgentLog!` or `\al` again goes back to that document and cursor. It holds
  `> ` for each prompt, the agent's message text (streamed: a chunk can end in
  the middle of a line, which stays open until its newline), `[thought] ` for
  thought chunks, `[tool] <kind> <title>: <status>` for tool calls,
  `[plan] n/m done`, `[permission] ...: waiting`, `[cancel] session/cancel
  sent`, `[stop] <reason>` and `[error] ...`. While the cursor is on its last
  line the view follows the output.
- The log is not modifiable. Changes give E21: a Normal or Visual command by
  its kind, and the Ex commands of `Editor::log_refusal`; `:w`, `:wq` and `:x`
  give E382 as Vim does for a `nofile` buffer. Output the editor writes is not
  a user change. The buffer is locked too (`Buffer::locked`: every text change
  is a no-op while it is set, and the editor's own output unlocks it for its
  write), so no path that misses the command gate can change the log. Visual
  operators and `.` are gated like the Normal changes.
- The leader keys, with the opt-in only (`[agent] enabled` or `--agent`): `\a`
  opens the prompt (Visual: `'<,'>Agent `); `\al` toggles the log; `\ac` is
  `:AgentCancel`. Because `\al` and `\ac` share the key after `\a`, `\a` alone
  waits 1000 ms (Vim's default `timeoutlen`, a deadline on `next_deadline`)
  before it opens the prompt. With the opt-in off `\` is Vim's key, and the
  keys after it are Vim's commands (`tests/compat_agent_keys.rs`).
- The indicator: `ai: <id> working` during a turn, `ai: <id> cancelling` after
  `:AgentCancel`, `ai: <id> PERMISSION n` while requests are queued.
- Cancel: `:AgentCancel` sends `session/cancel`; `acp.rs` answers each queued
  permission request as `cancelled`; the turn ends with stop reason `cancelled`.
- `acp.rs`: a `-32000` error on `session/prompt` enters the auth-required state
  as a failing `session/new` does (research M2): `NeedsAuth`, the connector
  stops the process and the message line says `not logged in`.
- Permission requests are not answered. The message line says the agent waits
  for a decision that is not supported yet, and `:AgentCancel` answers it.

Not in 5b-2 (left): context attachments (the cursor, the active file, the
selection and the open files are not sent; the prompt is the typed text only,
as PLAN "Prompting" allows for this step); `:AgentUse` and several connections;
permission answers and the grants (step 6); `<C-c>` in the agent log; the
split for `:AgentLog` (section 3, with splits); a name for the log in the
status line.

Not verified: the update and permission shapes in the tests follow the ACP
schema as step 5a read it, and the `-32000` code is the one the research names
for auth required. No real agent was run.

Tests: `tests/agent_turn.rs` (the in-process fake agent: a turn streamed in
order, a second prompt, the prompt text with `|`, `:Agent` alone, cancel
mid-turn, `-32000` on a prompt, a permission request answered as cancelled by
`:AgentCancel`, the log not modifiable and not written by `:w`, the leader keys
with the opt-in on a fake clock, and the leader keys off). `tests/compat_agent_keys.rs`
(differential) checks the leader keys against Vim with the opt-in off, and
the desired column around `\` (`lllljXj`-style cases). The unit test
`leader_keys_keep_the_cursor_and_the_desired_column` checks that `\ac` and
`\a<Esc>` keep the column with the opt-in on. `tests/acp_auth_prompt.rs` runs
`-32000` on `session/prompt` through the pure state machine (`NeedsAuth`, and
`Notice::AuthRequired`), and `the_log_refuses_every_change_and_still_takes_output`
covers the Visual operators, `.` and the Ex commands on the log.

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
possibly the ACP crate (not used; see the ACP row).

| Need | Choice | Notes |
|---|---|---|
| Unix signals (SIGHUP, SIGTERM) | `signal-hook` 0.3.18 (`cfg(unix)`) | Looked up 2026-11-01. Latest is 0.4.6, first published 2026-10-31, which is under two weeks old, so 0.3.18 is used. crossterm 0.29 already depends on 0.3 (through `signal-hook-mio`), so the lockfile gains no crate. Its iterator thread keeps all work out of the handler. Licence Apache-2.0 or MIT |
| Line diff, external-change undo step, diff view | `similar` 3.2.1 | Pure Rust, Myers/Patience. Its ops map to range edits. Three-way merge: write a small diff3 on top (or evaluate `diffy`, UNVERIFIED) |
| File changes | mtime/size polling | `notify` later, optional |
| Local IPC | `interprocess` 2.4.7 (`local_socket`) | One API for Unix socket and Windows named pipe; std alone has no named pipes. Set permissions, handle stale socket files |
| WebSocket (Claude Code IDE server) | `tungstenite` 0.30.0, blocking | One thread per client; bind 127.0.0.1 only and require a token |
| Terminal input | `crossterm` 0.29 | Bracketed paste, focus events and kitty flags are in the API; always feature-detect (`supports_keyboard_enhancement`). Whether 0.29 delivers paste and focus events on Windows (conhost vs Windows Terminal) is UNVERIFIED: test both and keep fallbacks. On Windows 0.29 reports no keyboard-enhancement support (`supports_keyboard_enhancement` returns `false`), so the kitty protocol is off there, including in Windows Terminal and WezTerm. Filter `KeyEventKind::Release` |
| Syntax highlighting | `tree-sitter` 0.27 with per-language `tree-sitter-<lang>` crates | Needs a C compiler (also in CI); each grammar adds roughly 0.3-3 MB (UNVERIFIED), so bundle a small set (about 10) and keep a regex highlighter as fallback. `tree-sitter-language-pack` bundles 372 languages; too large, loading model UNVERIFIED |
| Terminal pane | `portable-pty` 0.9 plus `vt100` 0.18 | ConPTY on Windows 10 1809+. `portable-pty` has had no release since February 2025; expect ConPTY quirks |
| Prediction providers | blocking HTTP (`ureq`, version UNVERIFIED) with small `serde_json` request types per provider | FIM endpoints (Ollama `/api/generate` with `suffix`, llama.cpp `/infill`, Codestral `/v1/fim/completions`, OpenAI-style `/v1/completions` with `suffix`) are UNVERIFIED. LSP 3.18 `textDocument/inlineCompletion` is preview and `lsp-types` 0.97 (2024) lacks it; async LSP stacks need tokio. Defer LSP inline completion |
| ACP | `agent-client-protocol` 3.6.0, not used | Its footprint was not checked. Step 5a writes the wire types with serde against the schema (`src/acp.rs`), so no async runtime is needed and the terminal-free core stays small. Revisit only if the schema outgrows the hand-written types |
| serde 1.0.229 (`derive`; `default-features = false`, `std` on) | Published 2026-07-18 (crates.io). Used by `src/acp.rs` for the ACP wire types (milestone 7 step 5a) | New in the lockfile with `serde_core` 1.0.229 (2026-07-18), `serde_derive` 1.0.229 (proc-macro, 2026-07-18) and `syn` 3.0.9 (proc-macro, 2026-10-12, required by `serde_derive`; `syn` 2.0.119 was already locked for another crate). Also new: `itoa` 1.0.18 (2026-03-20), `memchr` 2.8.3 (2026-07-08) and `zmij` 1.0.23 (2026-07-13), which `serde_json` uses. Already locked: `proc-macro2` 1.0.107, `quote` 1.0.47 and `unicode-ident` 1.0.27. The config loader's `toml` would bring the same `serde_derive` |
| serde_json 1.0.154 (`default-features = false`, `std` on) | Published 2026-10-11 (crates.io), the newest version at least two weeks before 2026-11-01 | Release binary (`cargo build --release`, Windows): 1,168,896 bytes before and after this step. The binary does not change because nothing in the binary calls the module yet; the new crates are compile-time only (`serde_derive` and `syn` run at build time) |
| Lua (milestone 9) | `mlua` (version UNVERIFIED) with Lua 5.4 `vendored` (UNVERIFIED); LuaJIT is the alternative | See "Lua extensions". Lua runs on one worker thread, so the `send` feature (UNVERIFIED) is not needed; footprint and build time to be measured |
| LSP (milestone 9) | JSON-RPC framing written in rvim, sharing code with the agent connectors; `lsp-types` optional (version UNVERIFIED) | See "Neovim-inspired features". Blocking I/O on a worker, as for agents |
| TOML config (milestone 6) | `toml` 1.1.8 with `default-features = false, features = ["std", "serde", "parse"]` | Checked 2026-11-01 against the crates.io API: 1.1.8 was published 2026-10-09 (23 days old, so it passes the two-week rule); its parser crate `toml_parser` 1.1.5 on 2026-10-09; `toml_datetime` 1.1.2 and `serde_spanned` 1.1.2 on 2026-10-08; `serde_core` 1.0.229 on 2026-07-18; `winnow` 1.0.4 on 2026-07-13. MIT OR Apache-2.0. `Table` and `Value` exist only with the `serde` feature. Cost, measured with `cargo build --release` on Windows x64: the release binary grows from 1,168,896 to 1,434,112 bytes (+265,216, +22.7%), main 7dcd971 against the verified branch head (an earlier measurement, 1,157,120 to 1,421,824, was taken before the marks rebase). Compiled new crates: `toml`, `toml_parser`, `toml_datetime`, `serde_spanned`, `serde_core`, `winnow`. Cargo.lock gains 12 package entries, including `serde_derive` and `syn` 3.0.9. Those two are not compiled for any target: `serde_core` declares `serde_derive` under `cfg(any())`, which never holds; `cargo tree --target all` shows them, the host tree does not. The `display` feature was dropped (no measurable size change). Not done, for a decision: a walker over `toml_parser` events, or a hand-written parser for the subset, would drop the serde crates and most of the size; full TOML is needed later for the approvals file, the Lua plugin approvals and the agent tables, which is why the crate was chosen. A syntax error is reported by its byte span, which rvim turns into a line number |

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
- The modified-version notice (condition II.3) has two texts in `src/lib.rs`.
  `NOTICE_SHORT` (`rvim <version>: modified Vim (not official)`, 39
  characters) is on the message line: the intro message and `:version`. The
  line holds only 61 cells on 80 columns, so the short text is needed there.
  `VERSION_MESSAGE` is the full text, with the license and the issue URL: it
  is printed by `--version` (stdout) and drawn in full in the start menu,
  where it is not cut. Tests: `tests/notice.rs`, `tests/config.rs`,
  `tests/cli_flags.rs`, `tests/start_menu.rs`. The wording was chosen by the
  implementer, not by counsel: the owner should confirm that the short notice
  and the full text satisfy the Vim license condition (this is not legal
  advice).

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
| Non-lowercase marks, `:marks`, `:delmarks` (mark.c) | `src/marks.rs` | Vim (conservative) |
| Event loop, keys, options, macros, recovery, text widths, view, lib, main | `src/event.rs`, `src/key.rs`, `src/options.rs`, `src/macros.rs`, `src/recovery.rs`, `src/text.rs`, `src/view.rs`, `src/lib.rs`, `src/main.rs` | Apache-2.0 |
| Tests (rvim's own differential cases and harness) | `tests/*.rs`, `tests/common/mod.rs`, `tests/license_headers.rs` | Apache-2.0 |
| Example | `examples/try.rs` | Apache-2.0 |

"Conservative" means the file's comments name a Vim function or model but the
owner has not checked it line by line against Vim's source. These eight files
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
present. `.github/workflows/ci.yml` runs on pushes to `main` and `ci-*` branches and on pull requests: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` on Linux, macOS and Windows, with `RVIM_REQUIRE_VIM=1` on all three, so a missing Vim fails the job. The "Vim present?" step prints the Vim version each runner used. Linux builds Vim 9.1.1914 from the `v9.1.1914` tag and caches it under `~/vim-9.1.1914`, because the tests are valid for that version. macOS uses Homebrew's Vim and Windows uses Chocolatey's (both 9.2.x); a difference there is a finding to report, not a reason to skip. Ubuntu's apt Vim (`9.1 (2024 Jan 02)`, patch level not printed in the log) is not used: on the first GitHub run (41298030211) three jump tests in `tests/compat_jumps.rs` differed from it on `''` after a `:N` line jump, while 9.1.1914 matches the tests. On run 41298351391 all three jobs passed: Vim 9.1.1914 on Linux, Vim 9.2 (2026 Feb 14) on macOS (Homebrew) and on Windows (Chocolatey, installed under `C:\tools\vim\vim92`). `.github/workflows/release.yml` is the release workflow. Where a document's
text differs from the rows below, the document is the source of truth.

**Makefile.** Thin wrappers over cargo, so that the commands in CLAUDE.md and
CONTRIBUTING.md have short names. `make help` lists the targets. Implemented:

| Target | Runs |
|---|---|
| `build` | `cargo build --release --locked` (locked, so packagers build what `Cargo.lock` pins) |
| `debug` | `cargo build` |
| `test` | `cargo test` |
| `test-vim` | `RVIM_REQUIRE_VIM=1 cargo test`, which fails when Vim is missing |
| `lint` | `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` |
| `fmt` | `cargo fmt` |
| `check` | `lint`, then `test-vim` (the pre-PR check) |
| `run` | `cargo run -- $(ARGS)`, for example `make run ARGS=file.txt` |
| `install` | `make build` first unless `SKIP_BUILD=1`, then copies `target/release/rvim` (or `BIN`) to `$(DESTDIR)$(PREFIX)/bin/rvim` (mode 755; `PREFIX` defaults to `/usr/local`, `DESTDIR` to empty), `LICENSE`, `LICENSE-VIM` and `NOTICE` to `share/licenses/rvim/`, and `README.md` to `share/doc/rvim/` |
| `uninstall` | removes those five files under the same `PREFIX` and `DESTDIR`, then `rmdir` on the directories it made, which removes them only if empty |
| `dist` | `git archive` of `HEAD` as `rvim-<version>-source.tar.gz` (version from `Cargo.toml`); no tag, no push, no upload |
| `cargo-install` | `cargo install --path . --locked` (into `~/.cargo/bin`): the former `install` |
| `clean` | `cargo clean` |
| `help` | the default target; lists the `##` comments |

Every target is `.PHONY`, and a failing command fails the target. The file
uses only POSIX make syntax (no `$(shell)` and no GNU conditionals), and was
run with mingw32-make on Windows (Git Bash has no `make` by default). The
install, uninstall and dist paths were run for real in WSL Ubuntu on
2026-11-01: a staged install under `DESTDIR` with `PREFIX=/usr`, `--version` of
the staged binary, an uninstall that left no files, and an archive containing
`Cargo.toml`, `src/` and the licence files. Not planned for the Makefile: a
release target. `make` does not replace CI, and `release.yml` stays the
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
- **Depends on** the release archives and checksums. The prerequisites are
  done on 2026-11-01: `make install` with `PREFIX`, `DESTDIR` and `SKIP_BUILD`,
  `make uninstall`, `LICENSE*` installed, and `INSTALL.md` ("For packagers").
  Still open: `make dist` archives `HEAD`, not a tag, so the `rvim` PKGBUILD
  must take the tag's archive; the tagged archive and its checksum are not
  published yet.
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

- Line numbers: the column follows Vim's rules, but the drawn screen is not
  compared with Vim (`vim -es` has no screen). The colours of `LineNr` and
  `CursorLineNr` are rvim's theme. With only 'relativenumber' the column is
  sized by the window height, as in Vim.
- `:checktime`, `:e` and `:w` read and stat files synchronously inside the Editor,
  against the CLAUDE.md rule of no blocking I/O. They are user-triggered and read
  one file, as Vim does. The automatic path (the file watcher, `src/watch.rs`)
  must not: it polls on a worker thread and delivers `FileChanged` events that
  carry the document version.
- Command-line flags: `-v` means `--version` in rvim, but Vim's `-v` starts
  Vi mode (rvim has no Vi mode). `-u` takes only `NONE` (skips the config
  file); any other `-u` value is a usage error, exit 2, where Vim reads that
  vimrc. `-N` is accepted and does nothing; `--clean` is `-u NONE`. Vim's
  other flags (`-R`, `-es`, and so on) are not parsed; an unknown `-x` is
  taken as a file name, as before.
- Message line width: the message line shows at most `width - 19` cells (it
  is cut at the ruler column, the width minus 18), so 61 cells on 80 columns.
  The intro message and `:version` use the short notice `NOTICE_SHORT` (39
  characters, `rvim 0.1.0: modified Vim (not official)`) plus, when the
  config had problems, ` [N config warnings]` (at most 21 characters); the
  total fits on 80 columns for any count up to 99. The full notice
  (`VERSION_MESSAGE`, about 170 characters) is in `--version` (stdout) and
  the start menu (drawn in the frame), where it is not cut. `:version`
  cannot show the full text: the message line holds one line and rvim has
  no multi-line output, so it shows the short notice only. Still cut at the
  ruler on narrow terminals: the file-info message and `:ConfigWarnings`
  (the list of warnings, joined with ` | `); a long file name can push the
  file-info message past the cut too.
- Agent detection (step 4a, `src/agent_detect.rs`; not Vim behaviour, recorded
  here so the later steps keep them): `Found` has no `version` yet (the
  package-manifest read is a later step, and the binary is never run). Registry
  ids are per CLI (`claude`, `codex`, `cursor`, `kimi`), while the approval
  example in "Agent experience" uses per-connection ids (`claude-acp`,
  `claude-ide`); the split is decided with the connector (step 5), and approvals
  will map to it. A config directory without an executable is reported as not
  found with a config note; the `Found` state of the status line (which covers
  it) is step 4b. Detection searches only absolute `PATH` entries (a relative
  one gives a warning), stops after 256 `PATH` entries and 4096 probes with a
  warning, and treats Cursor's generic `agent` as Cursor's only when "cursor" is
  in its path or in the path it resolves to. The scan is synchronous; it runs on
  the worker since step 4b (`:AgentScan`, and at startup with the opt-in).
- Agent scan and indicator (step 4b, not Vim behaviour; `agent_scan.rs`,
  `agent_detect.rs`). `:AgentScan` shows its result on the message line as a
  one-line summary (`found claude (PATH) | not found: codex, cursor, kimi`),
  cut at the ruler like other messages; it is not a scratch listing, and the
  full lines of `ScanReport::lines` are not shown. The indicator (`ai: files
  only`, `ai: none`, `ai: claude found`) sits left of the ruler and gives way
  to the message; it is dropped when the row is too narrow for it. The
  startup message `ai: found ... :AgentConnect` is not shown (there is no
  connection yet), and the scan reports no version (`version unknown`).
  `rvim --agent` is not added; the `[agent] enabled` key is the opt-in.
- hlsearch and incsearch (milestone 6). Vim's behaviour is checked where the
  harness can see it: text and cursor (`check` in `tests/compat_m6_search.rs`),
  and `v:hlsearch` after each step of a `-es` script
  (`v_hlsearch_after_each_step_matches_vim`). rvim's side of that test is the
  visible Search cells, which follow the flag; the flag itself is not
  exported. The screen is not compared in Vim, so the cells in
  `tests/search_highlight.rs` follow the rules as stated in the code, not a
  Vim screen dump. Not probed: `:g` (rvim turns hlsearch on for a `:g` with a
  pattern, as for `:s`). rvim's choices: an empty match is one cell (Vim's
  `next_search_hl` is the reference, not checked on screen); while the prompt
  is open the highlight is still the last used pattern, not the pattern being
  typed; `/offset` is ignored by incsearch; in Visual mode the Visual area
  follows the incsearch match while the prompt is open; the highlight of a
  match that starts more than one line above the top line is missed; the
  colours (`Search` black on dark yellow, `IncSearch` black on yellow) are
  rvim's, not Vim's default highlight groups, and there is no theme yet.
- Emoji use approximate block ranges instead of Vim's full `emoji_all` table
  for word motions; composing characters are separate characters.
- Option scopes (`options.rs`, "Options registry" in this file) follow Vim
  for the options listed there. Not done: `:set all`, `:setlocal all` and
  `:setglobal all` (show all values), the `!` argument of `:setlocal` and
  `:setglobal` (not probed); the window-local values are kept per window only,
  while Vim also keeps them per buffer (an `:e` back to a buffer restores the
  window options it had there) and gives each window a global value for its
  window-local options; a new window copies the global values, while Vim's
  `:split` copies the current window's local values. The `scroll` option is
  not in the registry (window-local in Vim); it is still `Editor`'s
  `scroll_opt`. The query messages of `:setlocal` and `:setglobal` are checked
  against Vim's probes in unit tests, not by the differential harness, which
  sees no messages.
- Agent connector, fixed in step 5b-1b (these were gaps in 5b-1): `Stop` could
  block forever when an agent closed stdout and kept running, because the
  reader held the child's lock across `wait`. The reader now checks for the
  exit without blocking, holding the lock only for each check, so `Stop`
  always kills (test `stop_returns_while_the_reader_waits_for_a_child_that_closed_stdout`).
  A retry after a lost connection now re-checks the approval; if it was
  removed, the retry is dropped with a message and nothing is spawned (test
  `a_retry_after_the_approval_is_revoked_does_not_start`).
- Agent connector, not fixed by design: `agent-approvals.toml` is read at
  startup even with the agent layer off. `:AgentConnect` works without the
  `[agent]` opt-in and needs the approvals to decide on consent, and a read on
  first use would be blocking I/O in the editor, so the file is read once in
  `main.rs`, as the config is.
- Agent protocol (milestone 7, not Vim behaviour): where ACP is silent, rvim's
  choices (handshake timeout, malformed-line rule, the permission and cancel
  rules) are listed under "ACP connector core". No Vim key changes.
  The connector process, consent and approvals (5b-1) are listed under
  "ACP connector, step 5b-1": one connection at a time, and the consent text.
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
  and the ruler is always shown. `number`,
  `relativenumber` and `wrap` are stored but have no effect yet. `autoread`
  acts only on `:checktime` and the reload it makes (the items at the end of
  this list).
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
  the `~`/`@` filler use them so far. The terminal is redrawn cell by cell:
  `redraw::diff` returns the changed runs as `DrawOp`s, and `main.rs`
  (`write_ops`) turns them into escape sequences. The `redraw.rs` tests
  check the ops; the `main.rs` tests check the bytes written to a `Vec<u8>`
  (a style reset before unstyled text, the default style at the end of a
  draw, clear-to-end after the move and the default style, and 1-based
  `MoveTo`). Still missing: matching parens, syntax (hlsearch and incsearch are drawn, see "Highlighting").
  Not tested: how a real terminal renders these sequences (manual check).
- A panic, SIGHUP or SIGTERM writes unsaved text to a recovery file
  (`recovery.rs`, `signals.rs`); there are no swap files, so Vim's swap-file
  behaviour on those signals is not matched. Not covered by the recovery
  files: SIGKILL and SIGSTOP (cannot be caught), a power loss or a crash of
  the machine, SIGQUIT (not handled, so the process dies without a file),
  and on Windows the console close, logoff and Ctrl-Break events (nothing is
  registered). SIGINT is untouched: in raw mode Ctrl-C is a key, not a
  signal. The tests raise SIGHUP and SIGTERM inside the test process
  (`signals.rs`); no test runs the real binary under a terminal, so the full
  terminal-close path (a pseudo-terminal closed by the emulator) needs a
  manual check on Linux and macOS. A verifier ran the binary under a pseudo-terminal
  in WSL on 2026-11-01: SIGHUP gave exit 129 and SIGTERM exit 143, each with a 0600
  recovery file in a 0700 directory holding the edited text and the path printed after
  the terminal was restored; an unmodified buffer wrote no file. Known gap: a signal
  that arrives before `signals::spawn` runs at startup (a few hundred milliseconds)
  takes the default action and writes no recovery file; nothing has been typed then.
- Recovery files are created owner-only on Unix: the directory with mode 0700
  (an existing directory the user owns is tightened to 0700; a symbolic link or
  a directory of another owner is not used, and the file goes to a fresh private
  directory under the temp dir, which the printed message names), each file with
  mode 0600 and `create_new`, so an existing name or link is never written
  through (`src/recovery.rs`). On Windows the default directory is under the
  per-user `%LOCALAPPDATA%` and no permissions are set; that is the only Windows
  protection. **Linux verified (2026-11-01):** in WSL (Ubuntu, Vim 9.1 patches up to 2141, run as a
  normal user) `RVIM_REQUIRE_VIM=1 cargo test` passed in full, including the six
  `recovery` tests (modes, symlinked directory, planted symlink), and clippy with
  `-D warnings` was clean. The CI macOS job also ran them (see the CI bullet below).
- CI (`.github/workflows/ci.yml`) first ran on GitHub on 2026-11-01 (run
  41298030211); it failed on the Vim setup and three jump tests. The Unix
  recovery-file tests (`cfg(unix)` in `src/recovery.rs`) passed on the Linux
  and macOS runners in that run. Run 41298351391 passed on all three runners
  (see "Repository and packaging").
- Alt, F-keys, Insert and Shift-Tab work (milestone 4). The kitty keyboard
  protocol is on where the terminal supports it (`RVIM_NO_KITTY=1` turns it
  off; with `TERM` unset, empty or `dumb` it is not queried at all, so the
  start does not block). Not tested by `cargo test`, which cannot run a real terminal: the
  manual checklist is to run `cargo run` in kitty, WezTerm and Ghostty (on
  macOS or Linux; crossterm reports no support on Windows, so the protocol is
  off there) and check that `<C-i>` and `<Tab>` match the same way in Vim's
  commands, that `<C-m>` and `<CR>` match, that `<C-[>` and `<Esc>` match,
  that `<A-x>` and Alt-Shift-letter work, that `<S-Tab>` works, that key
  repeat in Insert mode still types, and that the shell gets its keys back
  after `:q` and after a panic (the flags are popped, so the next program is
  not left with the protocol on). Shifted symbols with Alt (`<A-!>` on a US
  layout) are not matched: without `REPORT_ALTERNATE_KEYS` the terminal does
  not send the shifted symbol, so the key differs from legacy input. Not
  requested, to keep the protocol to the one flag.
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
- Visual, not fixed (found by the marks differential run; the `vo`/`vb` pair
  is identical at the base commit `bd9eef1`, as the verifier reported): (a)
  `vo<Esc>jix<Esc>` and `vb<Esc>jix<Esc>` on `"\tabc\n12345678901234567890"`
  and `"\tabc\tdef\n1234\n\t"` put the `x` on another column than Vim (Vim:
  line 2, column 1; rvim: column 8 on the first text); (b) `vl<Esc>3vd`, a
  count on the reselect after leaving Visual, differs from Vim on 13 texts,
  most without TABs (the list is the ignored test
  `known_difference_count_reselect_after_visual` in `compat_marks2.rs`). The
  sequences `vlld3vd` and `lv$<Esc>jix` on TAB texts do match Vim. Tests:
  `known_difference_visual_o_and_b_on_a_tab`, `known_difference_count_reselect_after_visual`.
- Marks: file marks `mA`-`mZ` are kept with the document they are in, and
  rvim has one document, so a file mark never names another file: jumping to
  one in another file (Vim's E37-style switch) and the file name column of
  `:marks` are not reachable yet. Undo does not restore a file mark (Vim's
  `uh_namedm` holds the lowercase marks only), so after `dd` on the marked
  line and `u` the mark is on the line after it, as in Vim. `viminfo` is not
  read or written, so the digit marks `0`-`9` and file marks from viminfo are
  never set (`'0` is E20). `'.` and `'^` (the last change and the last insert)
  do not exist either, so `:marks` does not list them (Vim lists them). `:marks` prints one message line: the rows of
  Vim's table (`' a " [ ] < >`, in Vim's format) separated by ` | `, not one
  row per line. `:delmarks` treats `<` and `>` as one Visual area (deleting
  either drops both), and `m<`/`m>` with no area yet set both ends to the
  cursor, where Vim leaves the other end unset. `'"` is always line 1, column
  0, since rvim has one buffer. At startup Vim sets the context mark to line
  1 (its window setup); rvim starts with none, as the differential harness
  does (it clears the mark before each case). So `:marks` has no `'` row and
  `:marks '` is E283 until a jump, where Vim shows `' 1 0`. Probes, `vim -N
  -u NONE -i NONE -es`: an empty buffer with `setline()` lists `' 1 0`; a
  three-line file given on the command line lists `' 3 0` (its cursor starts
  on the last line), and `normal! G` then `setpos("''", [0,0,0,0])` lists no
  `'` row. The Visual area does not follow line edits
  (Vim moves its ends with `mark_adjust()`; the ignored test
  `visual_marks_after_a_line_edit` in `compat_marks2.rs` shows it). After
  `<C-r>` of a linewise `p`, `']` is one line short of Vim's (the ignored
  test `redo_of_a_linewise_put_sets_end_mark`). The undo tree keeps one
  changed region per change, so `'[`/`']` after undoing a change made in
  several places (`:g`, `:s` on lines that are not next to each other) may
  differ from Vim's per-entry marks; this is not checked. The marks after
  `r`, `s`, `D`, `C`, block `I`/`A`, Visual `p` and `<C-a>`/`<C-x>` are not
  in the differential cases.
- Marks have no column adjustment for single-line edits, as in Vim (`x`, `D`,
  `r`, insert-mode typing do not move them). Joins (`J`, `gJ`, `:j`, `<BS>`
  and `<Del>` at a line end) and `:s` with a line break in the pattern keep
  them on the joined line with Vim's column mapping.
- The test harness cannot test state carried between cases (last search,
  last `:s`, `.`), since rvim starts each case fresh and Vim does not.
- Reload of a changed file (milestone 7 step 2a), cursor placement. Vim's
  `autoread` reload keeps the cursor's line number and column. Probed with
  Vim 9.1.1914 outside the harness, which cannot write files between keys:
  with `autoread` set, a cursor on line 4 stayed on line 4 when a line was
  inserted above it and the line itself was replaced, and a cursor on line 3
  stayed on line 3 when a line above it was deleted. rvim maps the cursor
  through the line diff, as PLAN.md "Reload as one undo step" says: a line
  equal in both texts goes with its text, and a changed line goes to the
  first new line of its replacement. Vim's exact rule is not known beyond
  these probes, so the difference stays until it is.
- The file watcher (milestone 7 step 2b, option `filewatch`, off by default:
  Vim does not poll files). It polls with `stat` every 750 ms, so a change
  made outside rvim is seen up to about 750 ms later, plus the read. It does
  not use inotify or the `notify` crate, so a poll costs one `stat` per open
  file, and a file is read only when its size or mtime moved. A change while
  the buffer is modified, or while `autoread` is off, keeps the buffer and
  warns (W11, W12, or W13 for a new file); the warning is shown once per change.
  A change while Insert mode or a command is pending waits for Normal mode,
  and the message says it is handled then, because the buffer may have become
  modified in the meantime (then it warns, W12). Focus is not a trigger: rvim
  does not enable terminal focus reporting. An unreadable file sends nothing to
  the editor; `:checktime` reports it (E211). `filewatch` is rvim-only:
  `:set filewatch` gives E518 in Vim.
- The watcher reads a changed file only up to 64 MiB (`MAX_WATCH_BYTES` in
  `src/watch.rs`). A larger file is not read: the buffer is kept, marked stale,
  and the message `changed on disk; too large to reload automatically, use :e!`
  is shown once per change. A touch of such a file is reported the same way,
  since there is no hash to compare. `:e!` loads it. `:checktime` and `:e` read
  the whole file, as before, because the user asked for it.
- Echo suppression covers rvim's own writes and touches: after `:w` the synced
  state is the written bytes' hash and mtime, so the watcher's read of them is
  not a change. It does not cover a file that changed and then went back to
  the synced bytes: the watcher does not report that return, so a stale warning
  stays until `:checktime` or the next change. A deleted file that comes back
  with its synced content keeps the Deleted stale flag for the same reason; the
  next `:checktime` or `:e!` clears it. A rewrite that keeps both the size and
  the mtime is seen by `:checktime` (it compares the contents), but the watcher
  does not read a file whose stat did not move, so it misses that rewrite. That
  matters on network and WSL file systems, where mtimes can be coarse or stale,
  so `:checktime` stays the check to run there.
- API surface (step 2b): `DocId` is now `pub` and re-exported from the crate
  root, and `pub mod watch` is exported from `lib.rs`, so `WatchCmd`, the
  `Event` variants `FileChanged`, `FileDeleted` and `FileTooLarge`, and the
  `FileSystem` trait are public.
- Where PLAN.md is silent, the rules in `src/reload.rs` are these. A pure
  deletion sends its lines to the line before it, but a deletion at the top
  of the file sends them to the first line after it. The scroll top follows
  its line when that line is equal; when it changed, the view keeps the
  cursor on the same row. A mark on a changed line goes where the cursor
  rule sends its line; Vim's `mark_adjust()` deletes such marks, but rvim
  keeps them. The desired column is kept on an equal line; a changed line
  takes it, clipped to the line.
- Above 20,000 lines, 4 MiB of text, or a changed middle part of 4,000,000
  cells (the LCS table), there is no line diff. The cursor and the scroll top
  keep their line numbers, clamped to the new text, and the message says
  "line diff too large".
- The reload message is `"f" changed on disk: +N -M lines`, not Vim's
  file-information message. The W11, W12, W13 and E211 messages are their
  first line only: Vim's "See :help" line and its prompt are not shown. A
  changed buffer is kept as it is; nothing asks.
- `:checktime {N}` takes Vim's buffer address, probed with Vim 9.1.1914.
  Rvim has one buffer, numbered 1: `1`, `%` and a part of the file name check
  it. Another number is skipped silently, as Vim skips a missing buffer, `0` is
  E939, and a name that matches nothing is E94. Vim's `#` (alternate buffer)
  is not supported. Vim checks every buffer when there is no argument, and
  rvim checks its one buffer. The command reads the file itself, as it is
  user-triggered; the watcher reads on a worker thread instead (`src/watch.rs`).
- `:checktime` decides by content: a file whose contents are the same is not
  a change even when its modification time moved. Vim warns on a new
  modification time; this was not probed. A rewrite that keeps the size and
  the time is still seen, because the contents are read and hashed.
- `:w` does not ask when the file changed on disk since it was read, without
  the agent opt-in: it overwrites it, and Vim asks with W11 ("WARNING: The
  file has been changed since reading it!!!"). With the opt-in (milestone 7
  step 3a, `[agent] enabled = true` or `--agent`) `:w` refuses instead, with
  W12 (`W12: Warning: File "f" has changed since reading (add ! to override)`),
  and `:w!` overrides. Vim's own prompt is not used: the refusal is rvim's
  wording, because the no-opt-in path was left as it was.
- The conflict prompt (milestone 7 step 3a, opt-in only) is rvim's own, and
  Vim has none: `"f" changed on disk and in rvim. [l]oad disk [k]eep mine
  [d]iff [m]erge` on the message line. `l` loads the disk text as one undo
  step tagged `external`; `k` and `<Esc>` keep the buffer (the prompt does not
  come back for that file state, but `:w` still refuses); `m` merges; `d` is
  not built yet (it needs the diff view), so it says so and the prompt stays.
  Other keys beep. Without the opt-in a modified buffer gets W12, as in Vim.
- The merge (`m`) is a line merge of the last text read or written (the
  baseline), the buffer and the file (`src/merge.rs`). Only conflicting groups
  get markers (`<<<<<<< mine`, `=======`, `>>>>>>> disk`), and `:w` refuses
  while a `<<<<<<< mine` or `>>>>>>> disk` line is in the text (E13; `:w!`
  writes). The merge is one undo step tagged `user`, and the buffer stays
  modified. Merged text is written with the buffer's line ends. Adjacent
  changes on the two sides conflict (as in Git). Above 4,000,000 cells of LCS
  table one side's change is the whole file, so the result is a conflict over
  it. Vim has no merge.
- With the opt-in, a deleted file gives `"f" was deleted on disk`, not E211;
  the buffer is kept and `:w` writes it again. The `[deleted]` mark in the
  status line is not drawn yet.
- The prompt's file reads (`conflict_load`, `conflict_merge`, `write_guard`)
  run on the editor thread, as `check_disk` and `:checktime` do. This breaks
  the rule that `Editor` does no blocking I/O (CLAUDE.md). It is a deliberate
  deviation for now: the key press asks for the read, and the file is small
  in practice. It moves to the worker with the rest of the file I/O later.
- Tests of the prompt itself are unit tests in `src/conflict.rs`, not
  differential cases: the harness cannot change a file on disk between keys,
  so a changed file cannot be set up in Vim. The differential cases in
  `tests/compat_reload.rs` cover the no-opt-in path with no file change (`:w`,
  `:checktime`, edits and undo around them).
- The baseline is kept for every file read or written, with or without the
  opt-in (one copy of the text per document, as lines). The memory cost is not
  yet limited for files near the 64 MiB watch cap.
- `u` restores the text of a reload, not the line-end style: a reload that
  changes CRLF to LF, or the reverse, keeps the new style after `u`.
- Undo sources are `user` and `external` only. The watcher, agent, paste and
  prediction sources come with their features.
- The harness covers `:checktime` only where the buffer has no file (no
  change is possible there), and checks that the text, the cursor and the
  modified state match Vim (`tests/compat_reload.rs`). A changed file is
  tested in unit tests in `src/ex.rs`, `src/editor.rs` and `src/reload.rs`.
- The agent log's read-only check (step 5b-2) is by command kind and by a
  list of Ex command names (`Editor::log_refusal`). An Ex command outside the
  list that changes the log is applied, not refused. The list covers the
  common change commands, not all of them.
- `\a` waits 1000 ms (or the next key) before it opens the prompt, because
  `\al` and `\ac` share its first key. Not configurable yet.
- `:AgentLog!` goes back one step only (no buffer list yet). The agent log is
  not in a split; it replaces the window's document.
