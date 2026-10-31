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

## License

rvim is a modified version of Vim: parts of it (for example the word
motions and the word-class table, with more as they are added) are
translated from Vim's C source. It is therefore distributed under the
[Vim license](LICENSE), which also applies to the rest of the code. As the
license requires, `:version` and the startup message say that this is a
modified Vim, not the official one.

Vim is by Bram Moolenaar and the Vim contributors (https://www.vim.org).

Contact for this modified version: open an issue at
https://github.com/axel0806/rvim/issues.
