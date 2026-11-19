# T6 — does the output-steering directive keep the prompt cache stable? (Anthropic, via Claude Code)

Date: 2026-11-02 · Status: **passed for Anthropic through Claude Code on a small prefix; other wires
and the full configuration not run** · Owner row: register T6 · Source: `docs/specs/token-savings.md`
7.2; FRD-0009 R27. Scripts: `verify/T6/probe.py`, `verify/T6/evaluate.py`; per-turn data:
`verify/T6/results.jsonl`.

Machine: Windows 11 Home 10.0.26200. Claude Code 2.1.316. BillionContext sidecar 0.1.207. Account:
the user's logged-in Claude Code, no API key. Models as resolved by the aliases `sonnet`, `haiku`,
`opus`, `fable`. Steering block: `{"enabled": true, "verbosityLevel": 2, "effortRouting": false}`.

## What was asked

The register asks whether the directive appended to the system prompt keeps the cache stable after
its first turn, on a harness that places its own cache breakpoint in the system prompt. The user
had already turned steering on in the live configuration and asked for the measurement afterwards.

## Method

One trial is one fresh Claude Code session of six turns, through its own temporary sidecar with its
own state folder and the installed build, so live sessions were not touched. The prefix is the small
one of T5 (`--strict-mcp-config`, `--tools Read`, `--setting-sources project`,
`--exclude-dynamic-system-prompt-sections`, `--effort high`); turn 1 adds about 15K tokens of filler
with a per-trial seed. Trials ran round-robin across three arms: `control` (no steering), `steered`
(from turn 1), `enabled` (no steering on turns 1 and 2; config switched and
`POST /__bili/config/reload` before turn 3).

Fixed before the first run. Hit rate is read / (read + created + input). Pass: over turns 4 to 6
(turn 3 too, for `steered`), every trial's hit rate is at least the lowest control hit rate of the
same turn minus 1 percentage point. The turn where steering is enabled may miss, and that miss is
recorded. A control turn that misses or errors makes the trial inconclusive.

## Results

Lowest hit rate over the turns named, after the exclusions below:

| Model | Trials control / steered / enabled | Control, turns 3-6 | Steered, turns 3-6 | Enabled, turns 4-6 | Pass |
| --- | --- | --- | --- | --- | --- |
| Sonnet 5.5 | 3 / 3 / 3 | 0.9976 | 0.9976 | 0.9976 | yes |
| Haiku 5.5 | 2 / 2 / 2 | 0.9958 | 0.9976 | 0.9976 | yes |
| Opus 5.5 | 1 / 1 / 1 | 0.9976 | 0.9976 | 0.9976 | yes |
| Fable 5.1 | 2 / 1 / 1 | 0.9977 | 0.9977 | 0.9977 | yes |

From turn 3 on, each turn wrote 56 tokens in every steered trial and in the control trials, except
one Haiku control session (102, 58, 58 on turns 4 to 6). The log shows the directive applied on every request of the steered
turns, and no directive on the control turns.

The miss at the turn it is enabled, once per session:

| Model | Read | Created | Trials |
| --- | --- | --- | --- |
| Sonnet 5.5 | 4840 | 19819 to 19925 | 3 |
| Haiku 5.5 | 5619 | 19826, 19880 | 2 |
| Opus 5.5 | 4953 | 19833 | 1 |
| Fable 5.1 | 6727 | 19874 | 1 |

The read part is the tools and system prefix before the directive, the same amount a steered and an
unsteered session read on their first turn. So steered and unsteered sessions share that cache, and
the miss is every message of the session. On a live session the size of the miss is its whole
history, which is rewritten once at the cache-write price.

## Excluded trials

- **Fable control 1.** Turns 3 to 5 ended in Claude Code's `API Error: Fable 5.1's safeguards flagged
  this message` on the prompts `Reply with exactly: C` to `E`. No steering was in that session. Each
  of those turns made two requests, so its read value doubled and its hit rates mean nothing. Replaced by Fable control 2 and 3,
  both clean. One occurrence in three control sessions; its cause was not investigated.
- **Haiku enabled 1.** A defect of the rig: the seed gave it the same filler as the pilot run minutes
  earlier, so it read the pilot's cache (0 created on every turn, no miss at turn 3). It says nothing
  about steering. Replaced by Haiku enabled 3. The pilot itself matches the table: read 5619, created
  19856, then 56 per turn. Reruns now take new trial numbers.

## Not covered

- **The full configuration.** T5 saw unexplained full misses at 172K to 180K prefix with no effort
  change, and that may recur here for reasons unrelated to steering.
- **Other wires and harnesses.** The register asks per wire: Codex, OpenCode and Pi remain open.
- **The saving itself.** Prompts of the form `Reply with exactly: A` cannot show whether the directive
  shortens answers. T6 measures only that it does not cost the cache after the one miss.
- **Repetition.** Opus and steered or enabled Fable have one trial each.
- **Test residue.** Test sessions remain in the user's `~/.claude/projects/D--tmp-te-steer-test-wd`.
