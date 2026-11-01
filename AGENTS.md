# AGENTS.md

Entry point for AI coding agents (Codex, Cursor, Claude Code and others)
working on rvim, a Vim clone in Rust.

## Read first

1. `CLAUDE.md`: the project rules. It is the single source of truth. If this
   file and `CLAUDE.md` ever differ, `CLAUDE.md` wins.
2. `PLAN.md`: the "Status" section at the top (milestone table, "In progress",
   "Suggested order"), then "Working notes", which record pitfalls that already
   cost time. Read the section for your task before you start.
3. `README.md`: user-facing behaviour and the known differences from Vim.

## Non-negotiables (summary only; `CLAUDE.md` has the full rules)

rvim is a modal editor that matches Vim 9 started with `vim -N -u NONE`. Vim
is the specification. Extensions are additive and off by default.

- Every behaviour has a differential case in `tests/compat_*.rs`. Never drop a
  case to make a test pass; when rvim and Vim differ, fix rvim.
- `editor.rs`, `motion.rs` and `buffer.rs` stay free of terminal code, and the
  editor does no blocking I/O. Only `main.rs` uses crossterm.
- No telemetry. Nothing leaves the machine unless the user configures it.
- Every `.rs` file starts with an SPDX line (`tests/license_headers.rs`).
- Keep `PLAN.md` current: update its Status section when a milestone moves.

## Commands

On Windows Git Bash, put cargo on the path first:
`export PATH="$HOME/.cargo/bin:$PATH"`.

```bash
RVIM_REQUIRE_VIM=1 cargo test --no-fail-fast   # full suite; without the env var a missing Vim silently skips
cargo test --test compat_m1 -- motions         # one test binary, optionally filtered
cargo clippy --all-targets && cargo fmt --check
```

Before you claim work is done, the test, clippy and fmt commands must all pass.
Report their exact results.

Windows Git Bash pitfalls:

- Never run `python` or `python3`: the Microsoft Store stub hangs.
- Use `perl -0pi` for multi-line scripted edits. Do not write Rust through
  shell heredocs or `sed`, which mangle backslashes (`\t`, `\n`, `\x16`). Use
  the file editor tool, and run `cargo build` after any scripted edit.

## Working in parallel

- One git worktree and one branch per task, based on `main`.
- Put new behaviour in new modules where possible, so parallel edits do not
  collide in the same large file.
- Rebase on `main` before you finish. Do not push to `main` unless asked.
- The git stash stack is shared by every worktree. Never use bare
  `git stash` or `git stash pop`. Use a temporary WIP commit instead.

## Avoiding loops

- Run each long command at most twice. Redirect its output to a file and read
  only the summary lines (`test result:`, `warning:`, `error`).
- Do not poll a background command. Wait for its completion notice.
- If the same failure comes back after one fix, stop and diagnose it (the
  Vim output for that case, and the harness notes in `PLAN.md`) before editing
  again.

## Leave alone unless asked

- `LICENSE*`, `NOTICE`, release tags (`v0.1.0-alpha.N`), `.github/workflows/`,
  and the text of `VERSION_MESSAGE` (the Vim license requires it).
- Documents outside your task, such as `CLAUDE.md` and `README.md`.

When finished, report the branch, the commit hash, the files changed, the exact
test, clippy and fmt results, and anything left unfinished and why.

## Repository map

`src/` (declared in `src/lib.rs` unless noted):

| File | Role |
|---|---|
| `main.rs` | crossterm terminal front end; the only file that touches the terminal |
| `lib.rs` | crate root; declares the modules |
| `editor.rs` | `Editor`: mode state machine, counts, desired column, event handling |
| `buffer.rs` | Lines in a rope; file load and save; CRLF and EOL detection; line edits log `PosEdit` |
| `text.rs` | Display widths and virtual columns for tabstops, control and wide characters |
| `motion.rs` | Positions and motions; word motions port Vim's C code |
| `normal.rs` | Normal and Visual key parsing and motions |
| `ops.rs` | Operators (`d c y > < ~ g~ gu gU` and friends) |
| `textobject.rs` | Text objects (`iw`, `a"`, `i(`, ...) |
| `register.rs` | Registers |
| `undo.rs` | Undo tree |
| `ex.rs` | Ex commands, ranges, `:s`, `:g`/`:v`, `:normal`, `:set` |
| `cmdline.rs` | Command-line editing and history for `:`, `/` and `?` |
| `search.rs` | Port of `searchit()` and `do_search()`; `*`, `#`, `gn` |
| `regex.rs` | Vim regex as a backtracking matcher over buffer lines |
| `scroll.rs` | Window top line, `scrolloff`, wrapped lines, scroll commands |
| `jumplist.rs` | Jumplist and context mark |
| `macros.rs` | Macro recording and playback |
| `options.rs` | Typed option registry |
| `view.rs` | Pure layout: `frame(&Editor)` builds styled cells and rows |
| `startmenu.rs` | Start menu state, layout and keys (opt-in) |
| `event.rs` | Internal event enum for the channel-based event loop |
| `key.rs` | `Key` and Vim notation parsing (`parse_keys`) |
| `recovery.rs` | Panic recovery files |

`tests/`:

| File | Role |
|---|---|
| `common/mod.rs` | Differential harness: runs the same keys in rvim and in Vim, compares the results |
| `compat_*.rs` | Differential tests against Vim, by milestone and area (`compat_m1` to `compat_m5_scroll`, `compat_gn`, `compat_jumps`, `compat_marks`, `compat_options`) |
| other `*.rs` | Rust-only tests for behaviour Vim cannot be compared on (prompts, paste, window layout, start menu), plus `license_headers.rs` (SPDX check) |

`examples/try.rs` is a small example program.
