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

With `make` (GNU make or BSD make on Linux and macOS; on Windows, install a
make such as mingw32-make, or use `make` from Git Bash if it is installed):

```
make build                # cargo build --release --locked (target/release/rvim)
make cargo-install        # cargo install --path . --locked, into ~/.cargo/bin
make install              # into /usr/local (see "For packagers"; needs root)
make help                 # list all targets
```

`make install` copies the release binary and the licence files into
`$(PREFIX)/bin`, `$(PREFIX)/share/licenses/rvim` and `$(PREFIX)/share/doc/rvim`
(PREFIX defaults to `/usr/local`). It is for Unix. To keep the older behaviour,
use `make cargo-install`, which runs the `cargo install` line above. The
`Makefile` also has `debug`, `test`, `test-vim`, `lint`, `fmt`, `check`, `run`,
`clean`, `uninstall` and `dist` targets. See CONTRIBUTING.md.

`cargo install --git https://github.com/axel0806/rvim` also works. There is no
crates.io release yet.

Dependencies: `crossterm`, `ropey` and `unicode-width` (plus `tempfile` for
tests). None builds C code, so no C compiler is needed today. The planned
tree-sitter highlighting will need one (PLAN.md, "Dependency choices").

## For packagers

For Unix distribution packages (AUR, Homebrew, distro builds). Install into a
staging directory with `DESTDIR`, under a `PREFIX`:

```
make install DESTDIR="$pkgdir" PREFIX=/usr
```

- Installs `$(DESTDIR)$(PREFIX)/bin/rvim` (mode 755),
  `$(DESTDIR)$(PREFIX)/share/licenses/rvim/` (`LICENSE`, `LICENSE-VIM`,
  `NOTICE`) and `$(DESTDIR)$(PREFIX)/share/doc/rvim/README.md`. `PREFIX`
  defaults to `/usr/local` and `DESTDIR` to empty.
- `install` runs `cargo build --release --locked` first. If the package builds
  in its own step, pass `SKIP_BUILD=1` so that `install` only copies:
  `make install SKIP_BUILD=1 DESTDIR="$pkgdir" PREFIX=/usr`. Without it the
  binary must already exist.
- `BIN=path` names another binary to copy (default `target/release/rvim`), for
  example when `CARGO_TARGET_DIR` puts the build elsewhere.
- `make uninstall` with the same `PREFIX` and `DESTDIR` removes those five files,
  then removes `bin`, `share/licenses/rvim`, `share/doc/rvim`, `share/licenses`,
  `share/doc` and `share` only if they are empty. `PREFIX` itself is never removed.
- `make dist` writes `rvim-<version>-source.tar.gz` in the repository root. It
  is `git archive` of `HEAD`, with the top folder `rvim-<version>/`. Uncommitted
  changes are not in it. It creates no tag and uploads nothing. For a release,
  package the archive of the tag (`git archive --prefix=rvim-<version>/ <tag>`),
  not `HEAD`.
- The targets use `install(1)`, `mkdir -p` and `rmdir`, so they run on Linux,
  macOS and the BSDs. Windows packages use the release archive.
- Keep the `LICENSE*` and `NOTICE` files with any copy of rvim you redistribute.

## Running

```
rvim path/to/file.txt
rvim --start-menu
rvim --help
rvim --version
```

`--start-menu` shows a start menu (New file, Open file, Quit) when no file is
given. It is off by default; `--no-start-menu` is the default and can be
given explicitly. `--help` (or `-h`) prints the options and `--version` (or
`-v`) prints the version and the modified-Vim notice; both exit at once.
Any other argument is the file name, and `--` ends the options. Without a
config file, rvim behaves as `vim -N -u NONE`.

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
- `make install`: `make uninstall`, with the same `PREFIX` and `DESTDIR`.
- Source build: delete the clone.

rvim registers nothing with the system. `make uninstall` removes only the files
that `make install` copies.

## Files rvim writes

- Config: none in this version.
- Recovery files: written only when rvim panics, when the terminal is closed
  (SIGHUP) or on SIGTERM on Unix, with unsaved changes; the path is printed. The directory is `$XDG_STATE_HOME/rvim/recovery` if
  `XDG_STATE_HOME` is an absolute path; otherwise `%LOCALAPPDATA%\rvim\recovery`
  if `LOCALAPPDATA` is set (on any platform); otherwise
  `~/.local/state/rvim/recovery` (from `HOME`, or `USERPROFILE`); otherwise the
  temp directory. Files are named `<file name>.<unix seconds>` (`unnamed` for a
  buffer with no file name). rvim never deletes them and does not offer them at
  the next start. On Unix the directory is created with mode 0700 and the files
  with mode 0600 (owner-only); an existing directory you own is set to 0700. A
  symbolic link, or a directory owned by someone else, is not used: the text
  goes to a fresh private directory under the temp dir and the printed message
  says so.
- Not implemented: swap files. SIGKILL cannot be caught, so a killed process
  writes no recovery file.

## Packaging status

- GitHub Releases: available (above).
- AUR package: planned, not available yet (PLAN.md, "Future goals").
- Makefile: `install` with `PREFIX`, `DESTDIR` and `SKIP_BUILD`, `uninstall`
  and `dist` (see "For packagers"), plus the cargo wrappers.
- crates.io, Homebrew, Windows installer or winget package: not available yet.

## Licence

rvim is dual licensed. See the README "License" section, `LICENSE`
(Apache-2.0, rvim's own code), `LICENSE-VIM` (the parts derived from Vim) and
`NOTICE` (which files fall under each). Keep the `LICENSE*` files with any copy
you redistribute.
