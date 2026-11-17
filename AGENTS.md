# Shinatama: rules for agents

- Never create backups of `D:\Software\Shinatama` (no `Shinatama-backup-*` folders, no copies of its exes). Update the install in place by copying the built exes over it.
- Shinatama must accept any version of every supported harness (Claude Code, Codex, OpenCode, Pi). Never gate launch or qualification on an exact or allow-listed harness version; a new build is requalified, not refused.
- Two branches, and only two: `main` (what is released) and `develop` (where work is integrated).
- Work always on `develop`, or on a short branch from `develop` that is merged back into `develop` and then
  deleted. Never merge or push into `main`, and leave no other branch, locally or on GitHub.
- A release is the pull request `develop` → `main`, opened and merged by the owner: every merge into `main`
  is a release (`.github/workflows/release.yml`, the same in Shinatama, Mukade, Xiox, Muro, Ethel and Qemille).
  The version goes up by a patch, or by a minor or major step when the pull request has the label
  `release:minor` or `release:major`; `build-release.bat` builds on Windows; the tag `v<version>` and the GitHub
  Release (`release\` zipped, its programs, its installers) are published, and `main` is merged back into
  `develop`. Nothing is published when the build fails.
- Never change the version or create release tags by hand: the workflow does it (`.github/release.json` lists
  the version files, `.github/scripts/release.mjs` writes them).
