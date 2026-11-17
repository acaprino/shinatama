# Shinatama: rules for agents

- Never create backups of `D:\Software\Shinatama` (no `Shinatama-backup-*` folders, no copies of its exes). Update the install in place by copying the built exes over it.
- Shinatama must accept any version of every supported harness (Claude Code, Codex, OpenCode, Pi). Never gate launch or qualification on an exact or allow-listed harness version; a new build is requalified, not refused.
- The main branch is `main`, the only one (there is no `master`): branch from `main` and merge back into `main`.
