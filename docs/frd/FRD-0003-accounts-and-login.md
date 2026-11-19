# FRD-0003: Accounts and login

| | |
|---|---|
| Version | 1 |
| Status | Accepted 2026-11-01 (this Markdown text) |
| Date | 2026-11-01 |
| Sources | functional-design goal 11, 5.11, 7; technical-design 5.5 A, 5.7, 6.7; records CL13, CX11, OC8, PI9 |
| Governing ADRs | ADR-0031 v2 |
| Register sections | D5, CL13, CX11, OC8, PI9, DS9, HM-login rows, O1 |
| Related records | FRD-0002 (login in a box), FRD-0005 (login before a hop), FRD-0010 (title calls without credentials), FRD-0015 (the router key) |

## 1. Purpose and scope

The user wants Shinatama to "recognize automatically or perform the login for codex, claude,
etc.". This record fixes the one rule everything else follows: **detect, never collect**.
Shinatama asks each agent whether it is signed in, runs the agent's own login flow when it is
not, and holds no credential of its own.

## 2. Requirements

### 2.1 Detection

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Shinatama recognizes an existing login automatically for each supported agent, and the user can run each agent's own login from Shinatama. | U | X | D5 | 0031 |
| R02 | The adapter's `auth_status()` returns `SignedIn`, `SignedOut` or `Unknown` by asking the agent itself, through its own status command or credential check; an agent with no secret-free check reports `Unknown`. | A | X | D5, CL13, CX11, OC8, PI9 | 0031 |
| R03 | Shinatama never reads, copies, moves or stores an agent's credentials, never calls a command that prints stored secrets (`opencode auth export`, Pi's `print-*`, `--credentials`), and puts no credential in the sidecar's launch manifest, the database, snapshots, messages or summaries. | A | X | D5, SC4 | 0031, 0009 |
| R04 | Each adapter declares which sign-in states it can detect and which logins it can only launch; detectors are per agent and per version (Claude's exit code and JSON; Codex's stderr and exit code; OpenCode through its HTTP route or retries, since `opencode auth list` failed on the real home; Pi's `pi auth check --json --no-refresh --provider <p>`). | A | X | CL13, CX11, OC8, PI9 | 0031 |
| R05 | Hermes Agent reports sign-in without secrets through `hermes auth list` and `hermes auth status <provider>`. | A | V1 | D5 | 0031 |
| R06 | DeepSeek Harness has neither a login nor a status command: its state shows as `unknown` until DS9 finds a check that reads no secret. It also reads a project's `.env`, so a key there is used even in a box, and the setup screen says so. | U 2026-11-01 (unknown until DS9); A (`.env` notice) | V1 | DS9 | 0031 |

### 2.2 Login

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R07 | `login()` runs the agent's own flow (browser sign-in, device code or key entry) inside Shinatama's terminal; Shinatama only observes the outcome. | A | X | D5 | 0031 |
| R08 | Hermes Agent logs in with `hermes model` or `hermes auth add`. | A | V1 | D5 | 0031 |
| R09 | DeepSeek Harness's login opens its own web UI (`dsh web`) on the home the agent uses (the box's `DSH_HOME`, or the user's home with `system`) in the user's browser, where the key or the account is entered; never on another home. | U 2026-11-01 | V1 | DS9, DS14 | 0031 |
| R10 | In a box each agent's home starts signed out, so the first launch asks for one login through the agent's own flow; the login then persists in the box and is shared by every instance of that agent. With the `system` home the user's existing login is used as it is. | A | X | D15 (login persistence, waits on O1) | 0031, 0012 |
| R11 | In a box the login is stored where the agent stores it, inside the box: Claude Code as a file on Windows and Linux and a Keychain entry per box on macOS; OpenCode as a table in its database; Pi in `auth.json`; Hermes Agent in `.env` and `auth.json`; DeepSeek Harness in `.credentials.yaml`. Shinatama never opens those files. | A | X | D15 | 0031 |

### 2.3 Where the state is shown and used

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R12 | Login state appears at first run (setup screen), in `shinatama doctor`, in the project manager (an agent that cannot start shows why) and in the hop dialog. | A | X | D5 | 0031 |
| R13 | A hop destination that is not signed in offers login before the hop, because the destination is prepared in the background; if login cannot fix it, the hop strategy is `refuse`. | A | S2 | H5 | 0031, 0016 |
| R14 | The conformance suite covers each adapter's signed-out path. | S | X | — | 0011 |

### 2.4 What Shinatama does not hold

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R15 | Holding provider keys once and sharing them across agents is not in v1: it would make Shinatama a credential store that must be protected. | A | V1 | — | 0031 |
| R16 | Title calls to a provider (FRD-0010) use a model from the catalog of a provider the user is already signed in with and go through the sidecar's auxiliary-call path or the agent's own non-interactive mode, so Shinatama holds no credential for them. | A | V1 | SC10, I7 | 0028, 0031 |
| R17 | The sidecar's launch manifest holds argv and launch overrides only. | U | X | SC4 (SEC) | 0029 |
| R18 | The one credential Shinatama may store is the router key of FRD-0015, and only in the operating system's vault, read only by the router sidecar, never in the database, configuration, manifests or logs. This narrows R03 to agent credentials and is the subject of ADR-0031 v2. | U 2026-11-01 (a router key of the user's); A (vault) | V1 | — | 0044, 0031 v2 |
| R19 | Browser sessions receive no inherited agent credentials, and no cookie, token or authorization header from browser use reaches logs, SQLite, transcripts or summaries (FRD-0011). | P | V1 | B8 | 0036 |
| R20 | Test code never copies credential files; logins for tests are done by the user, one per scratch home; which accounts the tests use is the user's decision. | S | X | register 0, O1 | 0010 |

## 3. Open questions

1. Which accounts and scratch logins the tests may use, and the cost cap per run (register O1). This blocks login persistence in a box (D15) and every live-model row.

## 4. Acceptance

D5, CL13, CX11, OC8, PI9 and DS9 pass with records. Today CL13, CX11 and PI9 are partial, OC8
is running with its criterion not met on the real home, and nothing about login has passed:
the 2026-10-27 runs performed no login.
