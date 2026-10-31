# rvim plan

Goal: a Vim-compatible modal editor in Rust. "Compatible" means: the same keys
on the same text give the same buffer and cursor as Vim 9 started with
`vim -N -u NONE` (Vim defaults, no vimrc). Every behaviour is checked against
a real Vim by the differential harness in `tests/common/mod.rs`.

## Status: read this first

Last updated 2026-10-31. **rvim is an alpha, roughly 25-30% of this plan.**
Released as `v0.1.0-alpha.1` (that release predates everything below the
"Since the alpha" line). All tests pass against Vim 9.1 and clippy/fmt are
clean at the last commit.

| Milestone | State |
|---|---|
| 1 Core, 2 Vim grammar | Done |
| 3 Search and Ex | ~95%. Left: `:s` flag `c` and `\=`, `gn`, `:r`, `:k` |
| 4 Foundation | ~70%. Left: the Document/Window/Editor split (the biggest item; see "Foundation"), kitty keyboard protocol, cell-level diff redraw, `:setlocal`/`:setglobal`, recovery on terminal close (SIGHUP) |
| 5 Windows and buffers | ~10%. Done: macros (`q`, `@`, `@@`, `@:`). Left: splits, buffer list, marks (and `'a` in Ex addresses), jumplist, scrolling commands, `scrolloff`, swap files |
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
and history (also in `cmdline.rs`).

**Suggested order for what is left:** (1) Document/Window split, (2) marks +
splits + buffers + scrolling, (3) the ACP connector and file-change reload
(milestone 7), because those are the goals beyond Vim, (4) hlsearch/incsearch/
syntax, (5) prediction. Cut a new release (`git tag v0.1.0-alpha.N`) when a
milestone lands; `.github/workflows/release.yml` builds the binaries.

### Working notes (things that cost time)

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

## Architecture

| Module | Role |
|---|---|
| `buffer.rs` | Lines in a `ropey` rope (LF-only line breaks), file load/save, CRLF and missing-EOL detection |
| `text.rs` | Display widths and virtual columns (tabstop 8, `^X` control chars, wide chars) |
| `motion.rs` | Positions and motions; word motions are ports of Vim's `fwd_word`/`bck_word`/`end_word` with its `inc`/`dec` cursor walk and `utf_class` table |
| `editor.rs` | Mode state machine (Normal, Insert, Command-line), counts, desired column (`curswant`) |
| `ex.rs` | Ex commands: ranges (`%`, `.`, `$`, marks `'<` `'>`, `/pat/`, `;`), file commands, `:d :y :pu :m :t :j :> :<`, `:s` (port of `do_sub`), `:g`/`:v`, `:normal`, `:set` |
| `normal.rs`, `ops.rs`, `textobject.rs`, `register.rs`, `undo.rs` | Normal/Visual parsing and motions, operators, text objects, registers, undo tree |
| `regex.rs` | Vim regex (`\v \m \M \V`, `\{}`, look-around, `\zs`, `\_x`, collections, `\%` items) as a backtracking matcher over buffer lines |
| `search.rs` | Port of `searchit()` / `do_search()` (offsets, `;`, `cpo-c`, wrapscan), `*`/`#` word lookup |
| `view.rs` | Scrolling, wrapping, `~`/`@` rows, message line and ruler as plain strings |
| `main.rs` | crossterm terminal front end (only file that touches the terminal) |

The library has no terminal dependency: tests drive `Editor::feed("keys")`.

Today `Editor` owns one `Buffer`, one cursor and one undo tree, and the
scroll position (`View`) lives in `main.rs`. Milestone 4 changes this; the
target structure is described under "Foundation".

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
   Vim). Left: `:s` flags `c` and `\=` expressions, `gn`, `:r`, `:k`.
   Marks in Ex addresses (`'a`) move to milestone 5 with the marks.
4. **Foundation (in progress).** Done: bracketed paste, Alt/F-keys/Insert/Shift-Tab, Visual area drawn via highlight groups, row-level diff redraw, options registry (`autoindent`, `expandtab`, `shiftwidth`), panic recovery files, channel-based event loop with an input thread. Left: the Document/Window/Editor split, kitty keyboard protocol, cell-level diff, `:setlocal`, SIGHUP recovery. Original description: Restructure before adding more features, because windows,
   agents and prediction all depend on it. Order, each step landing with the
   differential tests green: (a) Document/Window/Editor split, (b) internal
   event loop, (c) styled frame with the Visual area drawn, (d) diff redraw,
   (e) input (bracketed paste, modifiers, kitty protocol), (f) options
   registry, (g) emergency recovery files. Details in "Foundation".
5. **Windows and buffers.** (macros `q`/`@` are done.) Splits, buffer list, marks (also in Ex
   addresses), jumplist, macros (`q @`), `<C-f> <C-b> <C-d> <C-u> zt zz zb
   H M L`, 'scrolloff'; then swap files (see "Recovery").
6. **Modern editing experience.** hlsearch, incsearch, matching parens,
   syntax highlighting (tree-sitter), line numbers, a config file, a command
   palette, mouse. Details in "Editing experience".
7. **Coding agent integration.** Automatic detection of installed agents
   (Claude Code, Codex, Cursor, Kimi Code, ...) and connection to them over
   ACP or the agent's own IDE integration, with a files-only fallback. File-change detection and reload as an
   undoable step, a conflict flow, a diff view with per-hunk accept/revert,
   editor context for the agent, a terminal pane. Details in "Agent
   integration".
8. **Prediction.** Inline suggestions (ghost text) from a pluggable provider,
   accepted with a key. Details in "Prediction".
9. **Extras (optional).** LSP client, a Vimscript subset, plugin API.

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

## Known differences from Vim

- Emoji use approximate block ranges instead of Vim's full `emoji_all` table
  for word motions; composing characters are separate characters.
- Options come from the registry in `options.rs` (all global for now).
  `tabstop` is fixed at 8, there is no `softtabstop`/`smarttab`/`smartindent`,
  `backspace=indent,eol,start`, `whichwrap=b,s` and `startofline` are fixed,
  and the ruler is always shown. `hlsearch`, `incsearch`, `scrolloff`, `number`,
  `relativenumber`, `wrap` and `autoread` are stored but have no effect yet.
- An error does not flush pending typeahead (only matters for pasted key
  sequences).
- `n` after `/pat/e+1` when the cursor is on the last char of the buffer
  wraps in Vim but not in rvim (unexplained; see `compat_m3.rs`).
- After `:%s/\n//` Vim leaves the cursor at the last join point; rvim puts
  it on the first non-blank.
- `\=` expressions, `:s///c` and the `\n` (NUL) replacement are not
  implemented. `:p`, `:nu`, `:=` only set a message.
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
- The test harness cannot test state carried between cases (last search,
  last `:s`, `.`), since rvim starts each case fresh and Vim does not.
