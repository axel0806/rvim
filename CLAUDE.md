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
