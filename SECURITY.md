# Security policy

rvim is an alpha editor. It is not yet hardened for untrusted input.

## Supported versions

| Version | Security fixes |
|---|---|
| `main` | Yes |
| Latest `v0.1.0-alpha.N` release (list with `git tag --list 'v*'`) | Yes |
| Older alpha releases | No. Upgrade to the latest release |

At the time of writing the latest release is `v0.1.0-alpha.2`.

## Reporting a vulnerability

Please do not open a public issue, pull request or discussion for a
vulnerability. Report it privately with GitHub's private vulnerability
reporting: in the repository, open **Security**, then **Report a
vulnerability**. The report is a private security advisory that only the
maintainer can see.

<!-- UNCONFIRMED: the owner must enable private vulnerability reporting on
https://github.com/axel0806/rvim (Settings, Code security). Remove this comment
once it is confirmed. -->

Please include the rvim version or commit, your operating system, the keys or
input files that reproduce the problem, and the impact you expect.

## What counts as a security issue

- **File handling.** Reading or writing a file other than the one the user
  opened or named, including through symbolic links.
- **Recovery files.** When rvim panics, it writes the unsaved text of modified
  buffers to the state directory (PLAN.md, "Recovery"). rvim does not yet set
  restrictive permissions on these files, so on Unix they follow the umask and
  may be readable by other users. The fix (owner-only, mode 0600) is planned;
  see PLAN.md, "Known differences".
- **Command execution.** Running a shell command from text the user only opened
  or pasted. The code runs no external command today (there is no `:!` or
  filter). Any future one that runs file or clipboard text is in scope.
- **Modelines.** None exist. If they are added, options read from a file must
  not run commands or write files.
- **Terminal escape sequences** in file contents or pasted text that change the
  terminal or the editor's behaviour. Bracketed paste is treated as data.
- **The agent endpoint** (planned: context export and agent commands on a local
  socket or named pipe, and the Claude Code IDE server). Anything that lets
  another local user or process act without the user's approval is in scope.
- **Plugin and Lua permissions** (planned, PLAN.md "Lua extensions"). The
  planned permission checks are a policy in rvim's API, not an operating-system
  sandbox, so a bypass of those checks is in scope.
- **Path handling**: paths from the command line, `:e`, `:r`, the agent
  endpoint, and project root detection.

Crashes and wrong output that do not cross these lines are ordinary bugs; report
them as normal issues.

## The project's stance

From CLAUDE.md and PLAN.md:

- No telemetry. Nothing leaves the machine unless the user configures a provider
  or a command that sends it.
- Prediction is off by default and sends nothing until a provider is configured.
- The agent endpoint is off by default. Agent connections are only to local
  endpoints, and only after the user approves that agent for that project.
- Agent detection is local and read-only: it checks that files exist and does
  not start agents, send data or read credentials.
- Permission requests from agents are never approved automatically; only a key
  pressed by the user answers them.
- With no configuration and no flag, rvim behaves as `vim -N -u NONE`.

## Planned socket and pipe permissions

These are plans, not yet code:

- A Unix socket is readable and writable only by the user (mode 0600). A Windows
  named pipe has an ACL for the current user only.
- The Claude Code IDE server binds `127.0.0.1` on an ephemeral port and requires
  a random 128-bit token.
- Nothing listens unless the user turns the endpoint on.

## Response goals and disclosure

<!-- MAINTAINER TO CONFIRM: the 7-day and 90-day goals and the credit policy
below are proposals written by the drafting agent. They are not yet agreed by
the maintainer. Confirm or edit them, then remove this comment. -->

These are the maintainer's goals, not a promise or a service level:

- Acknowledge a report within 7 days.
- Agree a fix and a disclosure date with the reporter once the problem is
  confirmed, aiming to publish within 90 days of the report.
- Publish the fix in a new `v0.1.0-alpha.N` release, and a security advisory.

Please give the maintainer time to fix the problem before you publish details.
Reporters are credited in the advisory and release notes, using the name or
handle they give; tell the maintainer if you prefer to stay anonymous.

<!-- end of items to confirm -->


## Vim-derived code

Much of rvim is translated from Vim's C source (see NOTICE). Bugs in that code
that also exist in Vim may depend on fixes from Vim's own security advisories,
published by the Vim project at https://www.vim.org.
