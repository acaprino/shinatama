# OpenCode and Pi: a session id chosen by the launcher (plan task 0, step 1)

Date: 2026-11-17. Machine: the owner's. Programs: OpenCode v2.2.5, Pi 1.4.0. Mukade: the release v0.1.1
`mukade.exe` (sha256 `e2e727bc8714411e5d74c8f4b3b35ad961cea6d8cbd853978e0df95a7f7df07b`, from the GitHub Release),
with its own data folder, room `vroom`, and OpenCode's state kept in a scratch folder (XDG variables), so that
nothing of Shinatama's or the owner's was written to.

Question: does the id the launcher passes (`--session`, `--session-id`) come back as the id Mukade's plugin keys
its adapter record by (`mukade_chat::adapter_member(base, harness, native)`)?

## OpenCode

| Tried | Result |
|---|---|
| `mukade chat launch opencode --room vroom -- run --standalone --session <UUID> "say only: ready"` | refused by OpenCode before any plugin ran: `Expected a string starting with "ses"` at `["sessionID"]` |
| the same with `ses_f4a4700c4988qHjfJkARDz8LLp` (`ses_` + 12 hex + 14 letters or digits, the shape of its own ids) | accepted. The plugin joined the room as `opencode-1` and wrote `adapters/opencode/ses_f4a4700c4988qHjfJkARDz8LLp.json`, keyed by the id passed |

The model call then failed for lack of credits on the free model that was the default in the scratch
configuration; that is not what was asked.

**Consequence for the plan:** OpenCode's id must start with `ses_`; a UUID is refused. Shinatama generates
`ses_` + 12 lowercase hex + 14 characters of `[A-Za-z0-9]`.

## Pi

`mukade chat launch pi --room vroom -- --session-id 0e8a7129-1f2c-486d-bb80-3f2e8ac8a864 -p "say only: ready"`:
Pi printed `No project session found with id '…'; creating a new session with that id.`, the extension joined
the room as `pi-1` and wrote `adapters/pi/0e8a7129-1f2c-486d-bb80-3f2e8ac8a864.json`, keyed by the id passed.
The model call failed for lack of credits; not what was asked.

## Not checked

- The interactive interface (`opencode --standalone --session <id>` without `run`, and Pi without `-p`): the
  plugin and extension are the same ones; the owner's real run is task 9.
- That the id a tab's session records in Shinatama's database is the one passed: that is the code of task 0.

The plan's stop condition ("if either does not report the id it was given, stop") did not apply: both do.
