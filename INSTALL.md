# Installing rvim

rvim is an alpha. Source and releases: https://github.com/axel0806/rvim
(releases page: https://github.com/axel0806/rvim/releases).

## Prebuilt binaries

`.github/workflows/release.yml` runs on each `v*` tag and attaches one archive
per platform to a GitHub prerelease (`v0.1.0-alpha.N`):

| Platform | Asset |
|---|---|
| Linux x86_64 (glibc) | `rvim-<tag>-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `rvim-<tag>-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `rvim-<tag>-x86_64-pc-windows-msvc.zip` |

Each archive holds one folder with the binary (`rvim` or `rvim.exe`) and the
`LICENSE*` and `README*` files. The workflow publishes no checksum file, so
compare the download with the release page. There is no Intel macOS asset.

Unpack on Linux or macOS:

```
tar xzf rvim-v0.1.0-alpha.2-x86_64-unknown-linux-gnu.tar.gz
```

On Windows, unzip with Explorer or `Expand-Archive`. Then put the binary on
`PATH`: on Linux and macOS copy it to `~/.local/bin` (or `/usr/local/bin`
with `sudo`); on Windows copy `rvim.exe` to a folder on `PATH`, such as
`%USERPROFILE%\bin`.

Not checked: whether macOS Gatekeeper accepts the unsigned binary. The
workflow does no signing or notarization.

## Building from source

Install Rust with rustup (https://rustup.rs). `Cargo.toml` uses edition 2024
and sets no `rust-version`, so use a recent stable toolchain.

```
git clone https://github.com/axel0806/rvim.git
cd rvim
cargo build --release     # target/release/rvim, or target\release\rvim.exe
cargo install --path .    # or install into ~/.cargo/bin
```

`cargo install --git https://github.com/axel0806/rvim` also works. There is no
crates.io release yet.

Dependencies: `crossterm`, `ropey` and `unicode-width` (plus `tempfile` for
tests). None builds C code, so no C compiler is needed today. The planned
tree-sitter highlighting will need one (PLAN.md, "Dependency choices").

## Running

```
rvim path/to/file.txt
rvim --start-menu
```

`--start-menu` shows a start menu (New file, Open file, Quit) when no file is
given. It is off by default; `--no-start-menu` is the default and can be
given explicitly. Any other argument is the file name. Without a config file,
rvim behaves as `vim -N -u NONE`.

Platform notes:

- Windows: Windows Terminal is recommended. In Git Bash, add `~/.cargo/bin`
  to `PATH` for `cargo` and for an `rvim` installed with `cargo install`.
- Linux and macOS: any terminal emulator.

## Tests

The differential tests need Vim 9 (they use `charcol()`); PLAN.md records
testing against Vim 9.1.

```
RVIM_REQUIRE_VIM=1 cargo test
RVIM_REFERENCE_VIM=/path/to/vim RVIM_REQUIRE_VIM=1 cargo test
```

`RVIM_REFERENCE_VIM` names the Vim binary; otherwise `vim` is taken from
`PATH`. Without `RVIM_REQUIRE_VIM=1`, a missing Vim skips the differential
tests silently.

## Uninstalling

- Prebuilt binary: delete the copied `rvim` (or `rvim.exe`) and the unpacked folder.
- `cargo install`: `cargo uninstall rvim`.
- Source build: delete the clone.

rvim registers nothing with the system. There is no `make uninstall` yet.

## Files rvim writes

- Config: none in this version.
- Recovery files: written only when rvim panics with unsaved changes, and the
  path is printed. The directory is `$XDG_STATE_HOME/rvim/recovery` if
  `XDG_STATE_HOME` is an absolute path; otherwise `%LOCALAPPDATA%\rvim\recovery`
  if `LOCALAPPDATA` is set (on any platform); otherwise
  `~/.local/state/rvim/recovery` (from `HOME`, or `USERPROFILE`); otherwise the
  temp directory. Files are named `<file name>.<unix seconds>` (`unnamed` for a
  buffer with no file name). rvim never deletes them and does not offer them at
  the next start.
- Not implemented: recovery on terminal close (SIGHUP) and swap files.

## Packaging status

- GitHub Releases: available (above).
- AUR package: planned, not available yet (PLAN.md, "Future goals").
- Makefile with `install` and `uninstall` targets: planned, not present.
- crates.io, Homebrew, Windows installer or winget package: not available yet.

## Licence

rvim is dual licensed. See the README "License" section, `LICENSE`
(Apache-2.0, rvim's own code), `LICENSE-VIM` (the parts derived from Vim) and
`NOTICE` (which files fall under each). Keep the `LICENSE*` files with any copy
you redistribute.
