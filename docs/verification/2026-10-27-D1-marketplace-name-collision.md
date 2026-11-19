# D1 — role of `acaprino/shinatama` and name collision

Item: D1 (G0) — "The role of `acaprino/shinatama` after the 42.0.0 reunification (spec Q1); name collision with the product; install instructions." Pass when: "Statements in the specs and README corrected."
Initial status, before the later resolution below: **running (partial)** — facts recorded from both repositories; the corrections and the naming decision were the user's.
Date: 2026-10-27.
Runner: JanuraCode agent; public GitHub data only, cost 0.

Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/D1/run.ps1` (GitHub API; rerunnable without edits).
Raw output: `verify/D1/out/2026-10-27/{shinatama-repo.json, daodan-repo.json}`; README and commit evidence fetched the same day.

## Initial result (before the later resolution)

**`acaprino/shinatama` — public, alive, not archived**
- MIT, created 2026-10-17, last push 2026-10-25T10:22:59Z, branch `master`, 1 011 commits, 30.5 MB, language Python, 0 open issues.
- Latest commit (2026-10-25): "Move text-humanizer here from Daodan" — text-humanizer 2.2.0 now lives in Shinatama; business and digital-marketing require it from Shinatama, and "Daodan's project-knowledge requires it as text-humanizer@shinatama".
- README (master, fetched 2026-10-27): "Shinatama is the companion marketplace to Daodan… Shinatama holds 24 independent domain verticals, research, office workflows and specialized framework toolkits", with install commands for Claude Code, Copilot, Codex, Pi and OpenCode V2.

**`acaprino/daodan` — public, actively pushed**
- MIT, created 2026-01-06, last push 2026-10-26T23:50:54Z, 1 119 commits, 53.8 MB, language Python.
- README (master, fetched 2026-10-27): "Daodan is one marketplace: the development harness, the language and stack specialists, the Office methods, the domain verticals and web research all live here. The 24 plugins extracted to `acaprino/shinatama` in marketplace 36.0.0 returned…". Its specialist table also lists the verticals as independent plugins.

**Conflict found**: the two READMEs disagree on where the 24 verticals live today. Shinatama's README presents itself as their marketplace and its latest commit moves a plugin *into* Shinatama (2026-10-25); Daodan's README says they returned to Daodan (repo pushed 2026-10-26). Q1's "the role of `acaprino/shinatama` today is unclear" is therefore confirmed as a fact, not a suspicion. The deliverables of D1 (correct the statements, settle the name collision with the terminal-runtime product named `shinatama` on 2026-10-26, install instructions) need the user's decision.

## Criterion at the initial observation

Not met (no spec/README edit; decision pending). Evidence above is the base for it.

## Spec before / after

Before: Q1 in the register + local README note ("obsolete Daodan marketplace checkout… deleted on 2026-10-26").
After: unchanged. The register's description "obsolete… checkout" now needs a second look: the local checkout is gone, but the remote repository is live and receiving commits.

## Resolution (2026-10-27, later the same day)

User decision: "solo daodan è il repository marketplace plugin"; the remote `acaprino/shinatama` was to be deleted if it existed.

- Before deleting: mirror clone and verified bundle at `C:\Users\alfio\Backups\acaprino-shinatama-2026-10-27.{git,bundle}` (1 011 commits on `master`, 17 tags, head `21ab27cd`, 30.6 MB, `git bundle verify`: complete history).
- Comparison of its `.claude-plugin/marketplace.json` (24 plugins, marketplace 3.5.0) with Daodan's (48 plugins, 42.7.3): no plugin missing from Daodan, none newer there than in Daodan.
- No local clone under `D:\Projects` had it as a remote.
- `gh repo delete acaprino/shinatama --yes` exit 0; `gh api repos/acaprino/shinatama` then answers 404.
- Not touched: prose in Daodan `README.md` (line 981) and `AGENTS.md` (line 3) naming the repository, and the link in Mukade `README.md` line 8.
- Restore path: `git clone C:\Users\alfio\Backups\acaprino-shinatama-2026-10-27.bundle` and push to a new repository.
