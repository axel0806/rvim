# CLAUDE.md

rvim: a Vim clone in Rust. Read `PLAN.md` first (architecture, milestones,
known differences). Keep it current when finishing work.

## Commands

```bash
cargo test                          # unit + differential tests against real Vim
RVIM_REQUIRE_VIM=1 cargo test       # use this when verifying; otherwise a missing vim silently skips
cargo test --test compat_m1 -- motions
cargo clippy --all-targets && cargo fmt --check
cargo run -- file.txt
```

On Windows Git Bash, cargo is in `~/.cargo/bin` (add it to PATH).

## Rules

- Vim is the specification. Every behaviour gets cases in `tests/compat_*.rs`
  (text + keys in Vim notation). When rvim and Vim differ, fix rvim; never
  "fix" a case by dropping it. Port Vim's algorithm (its C source) when the
  behaviour has quirks, as `motion.rs` does for word motions.
- Keep `editor.rs`/`motion.rs`/`buffer.rs` terminal-free; only `main.rs` uses
  crossterm.
- Desired column: every command resets `curswant` unless it sets
  `keep_curswant` (vertical motions, counts, prefixes, failed `h`/`l`,
  `<Esc>`, `<C-g>`, Ex commands that do not move the cursor). Check new
  commands with `lllljXj`-style cases.
- Columns are char indices into a line; display columns come from `text.rs`.
- Extensions beyond Vim (agent integration, prediction, modern defaults) are
  additive: with no config and no flag rvim must behave as `vim -N -u NONE`,
  and an extension never changes what a Vim key sequence does while it is
  off. New Ex commands start with a capital letter. See "Extensions and the
  Vim contract" in `PLAN.md`.
- `Editor` stays terminal-free and does no blocking I/O. Anything slow or
  external (file watching, agent socket, prediction, highlighting, LSP) runs
  on a worker thread and reaches the editor as an event. Asynchronous results
  carry the document version they were computed for and are dropped if the
  document has changed.
- Nothing is sent off the machine unless the user configured a provider or
  command that does it. No telemetry; prediction and the agent endpoint are
  off by default.
- Agent detection is read-only and local (existence checks only). Connecting
  to an agent needs the user's approval, per agent and project, and only
  local endpoints. Verify an agent's protocol against its current docs before
  writing a connector; do not rely on memory.
- Rendering goes through the styled `Frame` (cells and styles); do not print
  text with colours from `main.rs` directly. Test layout and styles on the
  `Frame`.
- Changes from outside the user's typing (external reload, prediction, paste)
  apply as a single undo step tagged with their source.
