# Shinatama: rules for agents

- Never create backups of `D:\Software\Shinatama` (no `Shinatama-backup-*` folders, no copies of its exes). Update the install in place by copying the built exes over it.
- Shinatama must accept any version of every supported harness (Claude Code, Codex, OpenCode, Pi). Never gate launch or qualification on an exact or allow-listed harness version; a new build is requalified, not refused.
- Two branches, and only two: `main` (what is released) and `develop` (where work is integrated).
- Work on `develop`, or on a short branch from `develop` that is merged back into `develop` and then
  deleted. Only `develop` goes into `main`, with a merge, at a release.
- Leave no other branch, locally or on GitHub.
