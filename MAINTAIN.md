# Maintaining rvim

For maintainers. Code rules are in `CLAUDE.md`; plan and status are in
`PLAN.md`. Repository: `github.com/axel0806/rvim`.

## Branch model

- Work lands on `main`. Each task uses a feature branch or a worktree
  (agent worktrees live in `.claude/worktrees/`).
- Merge by fast-forward or rebase onto `main`. No merge commits for routine
  work.
- Every commit that moves a milestone updates `PLAN.md` (Status table,
  "In progress", "Last updated", and any "Known differences" entry it changes),
  as CONTRIBUTING.md requires.
- Commit subject: imperative, with the milestone in brackets when it applies
  (`Start menu: ... (milestone 6)`).

## Review checklist

1. `RVIM_REQUIRE_VIM=1 cargo test --no-fail-fast`. Without the variable a
   missing Vim skips the differential tests silently.
2. `cargo clippy --all-targets` and `cargo fmt --check`.
3. No differential case was dropped or weakened. When rvim and Vim differ,
   rvim is fixed. New behaviour has cases in `tests/compat_*.rs`.
4. `PLAN.md` is updated; `README.md` still matches "Where rvim differs".
5. Every new `.rs` file under `src/`, `tests/` and `examples/` has an SPDX
   line first (`tests/license_headers.rs` enforces this).
6. A Vim-licensed file keeps `SPDX-License-Identifier: Vim`. A new file that
   ports Vim code gets the Vim header, and the commit says so.
7. Nothing leaves the machine and there is no telemetry. Network, provider
   and agent behaviour runs only when the user configured it.
8. Extensions are off with no config and no flag, and never change what a
   Vim key sequence does while off.
9. `Editor`, `motion.rs` and `buffer.rs` contain no terminal code.

## Releases

Tags are `v0.1.0-alpha.N`; the next is the highest existing `N` plus 1.
Existing: `v0.1.0-alpha.1` (89663ee) and `v0.1.0-alpha.2` (756da7d), so the
next is `v0.1.0-alpha.3`. Check with `git tag --list 'v*'`.

Pushing a `v*` tag starts `.github/workflows/release.yml`. It builds `rvim`
for x86_64 Linux, aarch64 macOS and x86_64 Windows, packs each as
`rvim-<tag>-<target>.tar.gz` (Windows: `.zip`) with `LICENSE*` and `README*`,
creates a prerelease and uploads the archives. It runs no tests and no lint,
so the checklist above is the only gate.

1. On `main`, `git status -sb` shows a clean tree with no `ahead` or `behind`
   against `origin/main`. Run the checklist on the commit to be tagged.
2. Refresh the release notes in `.github/release-notes.md`. `release.yml`
   passes that file to `gh release create --notes-file`. Write it from the
   Status table and "Since the alpha" in `PLAN.md`, commit it on `main`, and
   tag a commit that contains it: the workflow reads the file from the tagged
   checkout. If the file is missing, `gh release create` fails silently
   (`2>/dev/null || true`), and the upload step then fails the job. The file
   is not packed into the archives.
3. `git tag -a v0.1.0-alpha.N -m "v0.1.0-alpha.N"`, then
   `git push origin v0.1.0-alpha.N`. Push only the tag.
4. `gh run list --workflow release.yml --limit 3`, then `gh run watch <id>`.
5. `gh release view v0.1.0-alpha.N` should list three archives. The workflow
   hides `gh release create` errors (`|| true`) and overwrites assets with
   `--clobber`, so a green run is not proof on its own.
6. If a build fails, fix it on `main` and cut the next alpha. Never move or
   delete a published tag or re-tag a version. A wrong release can be
   corrected with `gh release edit`.

`Cargo.toml` keeps `version = "0.1.0"`; the tag process does not change it.

**When to cut one:** when a milestone in `PLAN.md` becomes done, or a large
user-visible feature set lands together. Not for a small feature or a fix.

## Triage

- Labels are suggestions. No label set is defined in the repository.
- Reproduce a bug as a differential case in `tests/compat_*.rs` (keys in Vim
  notation), run with `RVIM_REQUIRE_VIM=1`, then fix rvim.
- A missing Vim behaviour goes into "Known differences" in `PLAN.md` and
  `README.md`, or into the milestone that will build it.
- An extension request is checked against the Vim contract first.
- Security reports follow [SECURITY.md](SECURITY.md). Do not discuss exploit
  details in public; ask the reporter to use the private route it describes.

## Keeping PLAN.md truthful

- Update the Status row of a milestone when it moves.
- Set "Last updated" to the date of the last status change.
- "In progress" lists work on branches not yet on `main`; remove the entry
  when the branch lands.
- When a "Known differences" item is fixed, remove it there and in
  `README.md`, and check the ignored `known_gap_*` tests.

## Licensing upkeep

- `LICENSE` is Apache-2.0 (new code). `LICENSE-VIM` is the Vim license text,
  unchanged. `NOTICE` lists the Vim-licensed files.
- Vim-licensed files use `SPDX-License-Identifier: Vim` and name Vim as origin
  and as a modified version. New files use `Apache-2.0`.
- Vim-derived ports include `motion.rs`, `textobject.rs`, `ops.rs`, `normal.rs`
  and `search.rs`. Seven files are "conservative" in `PLAN.md` (`buffer.rs`,
  `cmdline.rs`, `editor.rs`, `regex.rs`, `register.rs`, `scroll.rs`,
  `undo.rs`) and stay Vim until the owner checks them against Vim's source.
  Do not re-header a file on your own judgement.
- When a file's status changes, update its header, `NOTICE`, the `PLAN.md`
  "Licensing" table and the README licence section in one commit.
- Legal review is open (`PLAN.md`, "Open questions"). This is not legal advice.

## Dependencies

- Current (`Cargo.toml`): `crossterm` 0.29, `ropey` 1, `unicode-width` 0.2.3;
  dev-only: `tempfile`.
- A new dependency needs a licence compatible with Apache-2.0 and the Vim
  license. Check the crate metadata.
- No async runtime in the core (threads and `mpsc`, per `PLAN.md`
  "Dependency choices"). Record each choice, with version and reason, there.

## Packaging

- `INSTALL.md`, `CONTRIBUTING.md`, `AGENTS.md`, `SECURITY.md` and this file are
  in the repository (written 2026-11-01). The `Makefile` exists (`make help` lists
  its targets; `make check` runs lint and the Vim tests). It has no `uninstall`,
  `dist` or `PREFIX`/`DESTDIR` support yet.
- AUR packages are planned as `rvim` (from a tagged source archive) and
  `rvim-bin` (the Linux release binary). Nothing is published yet; see
  `PLAN.md` "Future goals".
- Keep `LICENSE*` in every archive and package. Do not change the
  `VERSION_MESSAGE` text in `src/lib.rs`; the Vim license requires it.
