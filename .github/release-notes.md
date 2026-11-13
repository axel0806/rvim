# rvim v0.1.0-alpha.4

rvim is a Vim-compatible modal text editor written in Rust. It is an alpha:
usable for editing files, but large parts of Vim are still missing. The goal is
that the same keys on the same text give the same buffer and cursor as Vim 9
started with `vim -N -u NONE`. The test suite checks this against a real Vim.

## New since alpha.3

- Line numbers: `number`, `relativenumber` and `numberwidth`.
- `hlsearch` and `incsearch`, with `:nohlsearch`.
- Marks: uppercase file marks `A`-`Z`, the `'[ '] '< '> '"` marks as motions
  and Ex addresses, `m[ m] m< m>`, `:marks` and `:delmarks`.
- Option scopes: `:setlocal` and `:setglobal`, with buffer-local and
  window-local options.
- A config file, `config.toml`, with `[options]`. `-u NONE` and `--clean`
  skip it, and `:ConfigWarnings` lists its problems.
- `:checktime`, and `autoread` to reload a changed file as one undo step. The
  `filewatch` option (off by default) watches open files for changes.
- Agent integration, all opt-in (`--agent` or `[agent] enabled = true`). With
  it off, rvim behaves as Vim.
  - `:AgentScan` finds the coding agents installed on the machine, checking
    only that they exist.
  - `:AgentConnect` starts an agent through the Agent Client Protocol (ACP). It
    asks you first, once per agent and project.
  - `:Agent {text}` sends a prompt. `:AgentLog` shows the read-only agent log,
    and `:AgentCancel` stops a turn.
  - Permission requests are answered with `y` `a` `n` `N` or `<Esc>`. They are
    only taken in Normal mode, so your typing is never read as an answer.
    `a` and `N` create grants that last for the session only.
  - The agent can read files inside the project, getting unsaved text for
    files that are open. Writes are applied as one undo step tagged with the
    agent's name. Writes to sensitive files (`.git/`, CI workflows, editor and
    agent settings, `CLAUDE.md`/`AGENTS.md`) ask first. `.env*`, keys and
    certificates are never shared.
  - A buffer you have modified is never overwritten silently: a conflict
    prompt offers load, keep and merge.
  - A change gutter marks lines that differ from the last `:e` or `:w`.
    `:AgentChanges` lists recent changes and their source, and `:AgentBase`
    shows the text before them.

The agent features have been tested only against a scripted fake agent so
far, not against a real Claude Code, Codex or Gemini session.

## What is missing

- No windows or splits, no tabs, and no buffer list or `:b`. There is one
  buffer. Splits and the buffer list are in progress.
- No syntax highlighting yet (in progress).
- `:s` has no `\n` (NUL) replacement. `:r !cmd` and `:!` shell commands do not
  work.
- No plugins, no Vimscript, no `:help`, no folds, no completion and no spell
  checking.
- No inline code prediction yet.

See [PLAN.md](https://github.com/axel0806/rvim/blob/main/PLAN.md) for the full
list of known differences from Vim.

## Install

Each archive holds one folder with the `rvim` binary (`rvim.exe` on Windows),
`LICENSE*` and `README*`. Installation steps are in
[INSTALL.md](https://github.com/axel0806/rvim/blob/main/INSTALL.md), including
building from source with `cargo install --git`.

## Licensing

rvim's own code is under the Apache License 2.0 ([LICENSE](https://github.com/axel0806/rvim/blob/main/LICENSE)).
Files derived from Vim are under the Vim license
([LICENSE-VIM](https://github.com/axel0806/rvim/blob/main/LICENSE-VIM)). Each
source file's first line says which license applies, and
[NOTICE](https://github.com/axel0806/rvim/blob/main/NOTICE) lists the
Vim-derived files. As the Vim license requires, `:version` and the startup
message say that this is a modified Vim.

## Known caveats

- This is an alpha. Expect bugs and missing Vim features. Report problems at
  https://github.com/axel0806/rvim/issues.
- The owner-only permissions for recovery files on Linux and macOS are not yet
  tested: the tests for them have not run on those systems. On Windows no
  permissions are set on recovery files.
