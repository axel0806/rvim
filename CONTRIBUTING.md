# Contributing to rvim

rvim is a Vim clone in Rust. Vim is the specification: the same keys on the
same text must give the same buffer and cursor as Vim 9.1 started with
`vim -N -u NONE`. [PLAN.md](PLAN.md) has the architecture, milestones and
known differences. [CLAUDE.md](CLAUDE.md) has the rules for code.

## Build and run

```bash
make debug                     # cargo build
make run ARGS=file.txt         # cargo run -- file.txt
make help                      # list the targets
```

The `Makefile` wraps cargo; each target is a thin cargo command. The packaging
targets (`install` with `PREFIX`/`DESTDIR`, `uninstall`, `dist`) are for
distribution packagers; see the "For packagers" section of INSTALL.md. Needs a Rust
toolchain that supports edition 2024 (the minimum version is not recorded
yet). On Windows with Git Bash, put `~/.cargo/bin` on `PATH` first. Plain
`cargo` works too.

## Tests

```bash
make test                                    # cargo test: unit and differential tests
make test-vim                                # RVIM_REQUIRE_VIM=1 cargo test: fail if Vim is missing
RVIM_REFERENCE_VIM=/path/to/vim cargo test   # choose the reference binary
cargo test --test compat_m1 -- motions       # one test
```

The differential tests run each (text, keys) case in Vim and in rvim, and
compare lines, cursor, modified flag and registers. They need Vim 9.1.1914 (the
version the tests are valid for) on `PATH` or in `RVIM_REFERENCE_VIM`. Vim 9.2
should pass too; if it does not, report the difference. Without `RVIM_REQUIRE_VIM=1`, a missing Vim
only prints a warning and the comparisons are skipped, so a green run proves
nothing about them. Use `RVIM_REQUIRE_VIM=1` when you report a result.

```bash
make lint      # cargo clippy --all-targets -- -D warnings, then cargo fmt --check
make fmt       # cargo fmt, to fix formatting
make check     # lint, then test-vim: run this before you open a PR
```

## Vim is the specification

- Every behaviour gets cases in `tests/compat_*.rs`: text and keys in Vim
  notation, run through `check()` or `cross()` in `tests/common/mod.rs`.
- When rvim and Vim differ, fix rvim. Never fix a case by dropping it.
- When a behaviour has quirks, port Vim's algorithm from its C source, as
  `src/motion.rs` does for the word motions.
- Desired column: every command resets `curswant` unless it sets
  `keep_curswant` (vertical motions, counts, prefixes, failed `h`/`l`, `<Esc>`,
  `<C-g>`, Ex commands that do not move the cursor). Check new commands with
  `lllljXj`-style cases.
- Columns are char indices into a line. Display columns come from `src/text.rs`.

The harness runs Vim with `-N -u NONE -i NONE -n -es` (Ex silent mode, 24x80
screen). Write cases with these limits in mind:

- Never run `:x` or `:q` in a case; they end the Vim process.
- Cases share no state; `SCRIPT_BODY` resets options, history and `curswant`.
- Window and screen state is not compared. Scroll cases compare cursors and
  lines; top lines are tested in `tests/scroll_window.rs`.
- `<Esc>` and `<C-c>` at a `:` or `/` prompt are not compared
  (`tests/search_prompt.rs` covers them). Messages and file I/O are not
  compared either.

## Architecture rules

From CLAUDE.md:

- `src/editor.rs`, `src/motion.rs` and `src/buffer.rs` do not use the terminal.
  Only `src/main.rs` uses crossterm.
- `Editor` does no blocking I/O. Slow or external work (file watching, agent
  socket, prediction, highlighting, LSP) runs on a worker thread and reaches
  the editor as an event.
- Asynchronous results carry the document version they were computed for and
  are dropped if the document has changed.
- Coloured output goes through the styled `Frame`, never printed from `main.rs`.
- A change from outside the user's typing (external reload, prediction, paste)
  is one undo step, tagged with its source.

## Extensions beyond Vim

- Extensions are additive. With no config file and no flag, rvim behaves as
  `vim -N -u NONE`, and an extension never changes what a Vim key sequence
  does while it is off.
- New Ex commands start with a capital letter (`:Agent`, `:PredictStats`).
- Nothing leaves the machine unless the user configured a provider or command
  for it. There is no telemetry. Prediction and the agent endpoint are off by
  default.
- Agent detection is read-only and local. Connecting to an agent needs the
  user's approval, per agent and project, and only local endpoints are allowed.
  Check an agent's protocol against its current documentation first.

## Licensing

This is not legal advice, and counsel review is still open. Read the
"Licensing" section of PLAN.md and [NOTICE](NOTICE) before you add a file.

- New code is Apache-2.0 ([LICENSE](LICENSE)). A new `.rs` file starts with
  `// SPDX-License-Identifier: Apache-2.0`.
- Code ported from Vim is under the Vim license ([LICENSE-VIM](LICENSE-VIM)),
  and so is the whole file that contains it. Such a file starts with
  `// SPDX-License-Identifier: Vim` and the notice used in `src/motion.rs`.
- A contribution to an Apache-2.0 file is Apache-2.0. A contribution to a
  Vim-licensed file stays under the Vim license.
- Porting Vim code must be declared in the commit message and the PR.
- `tests/license_headers.rs` (run by `cargo test`) fails if a file under `src/`,
  `tests/` or `examples/` has no SPDX line.
- Whether a contributor agreement (CLA) or DCO sign-off is needed is not decided.

## Picking work

- The "Status" section of [PLAN.md](PLAN.md) has the milestone table. Each row
  has a "Left" list of what is not done.
- "In progress" lists work under way. An entry is removed when its branch
  lands on `main`.
- "Suggested order for what is left" gives the order of the larger items.

## Commits and pull requests

- Keep commits small and focused, with tests in the same commit as the
  behaviour they check.
- Subject: a short description, such as `Jumplist and context mark (milestone 5)`
  or `Options: tabstop, softtabstop, smarttab, shiftround (milestone 4f)`. Put
  the milestone in brackets when one moves. PLAN.md-only changes start with
  `PLAN:`.
- The body says why, and names any Vim source or behaviour the change follows.
- Update [PLAN.md](PLAN.md) in the same commit when a milestone moves: the
  status table, the "Left" lists, "Known differences" and the "Last updated"
  line (CLAUDE.md states this rule). Keep the README's "Where rvim differs
  from Vim" in step with "Known differences".
- Before opening a PR, run `make check` (`make lint` and `make test-vim`). CI
  (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings` and the
  tests on Linux, macOS and Windows for every PR and push to `main`, with
  `RVIM_REQUIRE_VIM=1` on all three, so the differential tests run on every OS.
  Linux uses Vim 9.1.1914 built from source; macOS and Windows use the package
  manager's Vim.

## Reporting security problems

Do not open a public issue for a security problem. Report it as described in
[SECURITY.md](SECURITY.md). Its private reporting route (GitHub's private
vulnerability reporting) is not yet confirmed to be enabled for this repository.

## Coding agents

Read [CLAUDE.md](CLAUDE.md) for the rules and [PLAN.md](PLAN.md) for the
status. [AGENTS.md](AGENTS.md) is the entry point for coding agents; it
summarises the rules and points to CLAUDE.md, which wins on any difference.
