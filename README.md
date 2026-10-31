# rvim

A Vim-compatible modal text editor written in Rust, checked keystroke by
keystroke against a real Vim.

```
cargo run --release -- path/to/file
```

Milestone 1 is done: files, Normal/Insert/Command-line modes, the basic
motions with counts, inserting, `x`, and `:w :q :wq :x :e`. See
[PLAN.md](PLAN.md) for the architecture, the milestone list and the known
differences from Vim.

## Tests

```
cargo test                      # unit tests + Vim differential tests
RVIM_REQUIRE_VIM=1 cargo test   # fail instead of skip if `vim` is not runnable
RVIM_REFERENCE_VIM=/path/to/vim cargo test
```

The differential tests need Vim 9 (`charcol()`) on PATH or in
`RVIM_REFERENCE_VIM`.
