# TE13 — Shinatama's own tool-pruning evidence on OpenCode

Date: 2026-11-02 · Status: **passed, with the script corrected for Code Mode** · Owner row:
register TE13 · Source: `docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md` section
7.1; ADR-0045 rule 4. Scripts: `verify/TE13/opencode_prune.py`, `verify/TE13/fake_provider.py`.

## Question

TokenEconomizer gathers no pruning evidence on OpenCode (TE12). Can Shinatama compute the proposal
from the exports TokenEconomizer writes, with the same rule and the protected set, and do the
resulting `permissions` deny rules remove the tools from the request on 2.2.5?

## Setup

- OpenCode 2.2.5, Windows 11, TokenEconomizer commit `c53a4a6`, plugin loaded from
  `<OPENCODE_CONFIG_DIR>/plugins/` through the re-export wrapper of TE5.
- Two stub MCP servers: `bili` (`compress`, `decompress`) and `acme` (`lookup`). `bili` stands for
  BillionContext by name only; the real server was not used.
- The gateway answered 502 `provider_unavailable` during the run. Three exports come from the real
  model (TE5 and one later session); the other two sessions and every request capture below come
  from `fake_provider.py`, a scripted local provider. What is measured is the request OpenCode
  builds and how it executes tool calls, neither of which depends on the model. Model behaviour
  with the tools removed was not exercised.
- Every headless run had stdin closed (see the stall below).

## Evidence and proposal

Five exports, five prompts, tools used `shell`, `read`, `glob`; `enough_evidence` true by the
session arm (5 sessions).

| Set | Tools |
| --- | --- |
| Catalog | `acme_lookup`, `bili_compress`, `bili_decompress`, `edit`, `execute`, `experimental_image_generate`, `glob`, `grep`, `question`, `read`, `shell`, `skill`, `subagent`, `webfetch`, `websearch`, `write` |
| Never used, protected, kept | `bili_compress`, `bili_decompress`, `edit`, `execute`, `grep`, `question`, `skill`, `subagent`, `webfetch`, `websearch`, `write` |
| **Proposal** | `acme_lookup`, `experimental_image_generate` |

The proposal became two rules of the shape `{"action": "acme_lookup", "resource": "*", "effect":
"deny"}`. `Permission.Rule` admits no extra key, so the rules the script owns are named in a sidecar
next to the config, and a person's rules survive a later apply (self-test).

## Results

| Case | MCP servers | Rules | Tools in the request | Bytes of `tools` | Calling `acme` |
| --- | --- | --- | --- | --- | --- |
| Direct, base | `codemode: false` | none | 16 | 16516 | runs, `acme_lookup` completed |
| Direct, denied | `codemode: false` | two | 14: both denied tools absent, `bili_*` present | 14387 | not offered, no call possible |
| Code Mode, base | default | none | 13: MCP tools absent from the list | 16030 | runs through `execute` |
| Code Mode, denied | default | two | 12 | 14059 | `Unknown tool 'acme.lookup'`; `tools.bili.compress` still runs |

One trial per case. The byte figures are the tool list only and are no saving claim (ADR-0045
INV-005).

## Findings

1. **Deny works on 2.2.5 for MCP and native tools, in both modes.** This closes the point TE12 left
   open. Under Code Mode the denied tool also leaves the catalog in the system prompt (`acme` and
   `lookup` occur 0 times in the request).
2. **Code Mode hides MCP use from a plain reading of the export; the first script would have pruned
   a tool in use.** MCP tools are not in the request's tool list but in a catalog in the system
   prompt (`tools.acme.lookup()`), and a call is exported as the tool `execute`, with its target only
   in `state.metadata.toolCalls` (`{"tool": "acme.lookup", "status": "completed"}`). The script now
   counts those under the permission key (`acme_lookup`); on a real Code Mode export the used set is
   `acme_lookup`, `bili_compress`, `execute`. The catalog cannot come from the request's tool list in
   this mode: Shinatama supplies it from the instance's set.
3. **`opencode run` hangs when stdin is inherited, not closed.** With the same config, three runs
   with stdin closed exited 0 in 7 s with 2 provider requests; three runs with stdin inherited were
   killed at 35 s with 0 requests. The log of a hung run stops at `cli starting`. Earlier runs in the
   same shell did not always hang, so inheriting stdin is not always fatal; closing it was enough
   every time. Shinatama's launcher closes stdin on every headless `opencode run`.
4. **This corrects TE5.** The stall TE5 attributed to the `plugins` array has the same signature. With
   stdin closed, that array with a `file:///` URL to the plugin file exited 0 three times out of
   three, and the log says why it had no effect: `configured plugin path must be a directory`. The
   route does not stall; it skips the file. Pointing the array at the plugin's directory also loaded
   nothing (one trial). The `plugins/` directory route stays the one that works.
5. **`mcp.<name>` is not read on 2.2.5.** Servers go under `mcp.servers.<name>`; the other form gives
   `No MCP servers configured`.

## Not covered

- BillionContext's real server and the name it registers on OpenCode: `bili_*` is still assumed.
- Whether MCP servers keep running when all their tools are denied.
- The threshold through the prompts arm of `enough_evidence`, exercised only by the self-test.
- Isolation is partial. `OPENCODE_CONFIG_DIR` and `TOKENECONOMIZER_HOME` were separate, but TE5 and
  most TE13 runs wrote to the user's `~/.local/share/opencode` database and snapshots, and OpenCode
  watched `~/.claude/skills`. Only four runs used separate `XDG_*` folders.
