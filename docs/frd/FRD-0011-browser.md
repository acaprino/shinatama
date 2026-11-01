# FRD-0011: Browser integration

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Proposed |
| Date | 2026-11-01 |
| Sources | functional-design goal 13, 2 (non-goals), 4, 5.15, 7; technical-design 7.9; browser-integration 1 to 12 |
| Governing ADRs | ADR-0035, ADR-0036 |
| Register sections | B1 to B10, O8 |
| Related records | FRD-0001 (preview panes, no embedded browser), FRD-0007 (authority rules of 7.6), FRD-0014 (no secrets in the database) |

## 1. Purpose and scope

Agents and the user get browser access without paying Chrome's cost when Chrome is unnecessary:
Moli for fast one-shot operations, local HTML, previews and MCP web interfaces; Patchright
driving a visible Chrome only on explicit request. This record states the six user requirements
of 2026-10-29, the routing contract, the engine boundaries, process ownership and lifecycle,
authority and privacy, configuration, and the gates that decide what ships.

Vocabulary (browser-integration 3): a **browser request** `br_…` has an immutable engine,
origin, operation, target scope and permission snapshot; a **browser session** `bs_…` is a live
one-shot operation or an explicit UI session owned by one window; a **controller** is `agent`
or `user`, exactly one for an interactive session.

## 2. Requirements

### 2.1 The six user requirements

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Moli is the default engine (`browser.default_engine = moli`) for fast operations, local HTML, previews and MCP web interfaces; layout and visual resources are enabled only when the requested operation needs them. | U 2026-10-29 | V1 | B1, B2 | 0035 |
| R02 | Patchright with a visible Chrome window is the graphical path for explicit UI use, login, MFA, CAPTCHA and pages the user chooses to open there. | U 2026-10-29 | V1 | B5 | 0035 |
| R03 | No automatic fallback: an error, timeout, empty page or anti-bot response never authorizes a switch to another engine. | U 2026-10-29 | V1 | B6 (SEC) | 0035 |
| R04 | No automatic transfer of pages, cookies, credentials, sessions or control between engines. | U 2026-10-29 | V1 | B8 (SEC) | 0035 |
| R05 | No resident Chrome or Patchright companion: the graphical path is loaded and started only on an explicit request; nothing is installed automatically. | U 2026-10-29 | V1 | B5 | 0036 |
| R06 | Intent-based routing: a preinstalled skill or `/browse` may choose the engine from the user's request; web content and agent output cannot change the engine or its permissions. | U 2026-10-29 | V1 | B6 | 0035 |

### 2.2 Surfaces and routing

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R07 | `/browse` (spelling is the author's call) offers `read` (bounded text or structured extraction through Moli), `preview` (a static visual artifact, layout enabled for that operation only, requiring the scope plus artifact permission) and `open-ui` (a Patchright session, explicit user request only); `--engine=moli|patchright` is an explicit override and not a security boundary. The selected engine, reason, operation and scope are shown before execution. | P | V1 | B1, B2, B5 | 0035 |
| R08 | Routing happens before the target loads and is immutable for that request; a page, redirect, script, error message, tool result or peer message is data, never routing authority; starting another engine is a new request, never continuation of the failed one. | P | V1 | B6 | 0035 |
| R09 | Decision table: read or extract local HTML or an authorized web target → Moli within the existing scope; static preview → Moli plus artifact permission; MCP web interface, bounded one-shot → Moli within an explicit local service scope; visible browser, login, MFA, CAPTCHA → Patchright on explicit request with the setting on, otherwise `unsupported`; ambiguous non-sensitive read → Moli, narrowest scope; ambiguous sensitive action or broad local access → no engine, ask; Moli unsupported or failed → keep Moli and stop. | P | V1 | B6 | 0035 |
| R10 | An agent may request `read` or `preview` only inside an existing user-authorized task and target scope, without asking again for navigation inside that grant; it cannot grant itself `open-ui`, broaden scope or answer its own permission request; it may ask the user to issue a new explicit UI command. | P | V1 | B6 | 0035 |
| R11 | The logical request carries `request_id`, schema version, `origin` (user or agent), instance, thread and project identity where applicable, `operation`, `engine`, `target`, `scope_id`, an immutable `permission_snapshot`, `reason` and requested capabilities; the result carries request and session ids, engine, outcome (`ok`, `unsupported`, `needs_user`, `denied`, `failed`), artifacts, error and cleanup state. The transport schema is the builder's. | P | V1 | — | 0035 |
| R12 | The runtime validates engine, operation, identity and scope; skill prose, `SHINATAMA_INSTANCE`, a caller-supplied instance id and `--engine` are not authentication; the per-instance integration authenticates its caller and derives identity server-side (that mechanism is unverified). | P | V1 | B6, M15 | 0035 |
| R13 | A result never converts `failed` into an implicit retry on the other engine; after a Moli failure the UI may show the exact new command the user can choose. A visual request is never reported as satisfied when only text was produced. | P | V1 | B2, B6 | 0035 |
| R14 | The session view and the overview show, per browser session, engine, target origin or root, operation, state, controller and reason. | P | V1 | — | 0035 |
| R15 | MCP web interfaces stay in the first integration only if B3 demonstrates them within one-shot `fetch`; otherwise they are explicitly deferred, never replaced by `moli serve` or general agent `--eval`. | U 2026-11-01 | V1 | B3 | 0035 |

### 2.3 Engine boundaries

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R16 | The fast path is the pinned Moli binary as a child for a bounded one-shot `fetch`, with fixed operations and output parsers; `moli serve` is not started; general `--eval` is not exposed to agents (a local probe may use it to measure the MCP use case). | P | V1 | B1 | 0035, 0036 |
| R17 | Defaults of a Moli operation: no layout, images, fonts or optional resources for text extraction; layout only where needed; bounded time, output, response size, redirects, artifact count and size; a temporary dedicated profile unless the user configures persistence; no inherited agent credentials or unrelated secrets; target and subresource policy applied to the initial URL, redirects and local files. | P | V1 | B1, B8 | 0035 |
| R18 | `moli serve`, persistent CDP or WebDriver automation and interactive Moli previews stay disabled until B4 proves a private authenticated transport or an equivalent verified boundary that untrusted pages cannot reach; loopback, an ephemeral port and a random path are not authentication; the same gate applies to Patchright transports. The source finding at Moli `dcae77e` (a TCP server on `127.0.0.1:9222` with no request credential check found) is a scoped reading, not a runtime result. | P | V1 | B4 (SEC) | 0036 |
| R19 | The Patchright path: a Shinatama companion (Node.js or Python, the builder's choice) starts visible Chrome through a persistent context only after an explicit UI request; the companion and its dependencies are lazy-loaded; no helper, Chrome process or update check runs at startup; the companion exposes only the typed session operations the UI path needs, never a generic local automation socket; its transport, ports, process tree and cleanup are inventoried and verified. | P | V1 | B5, B4 | 0036 |
| R20 | Patchright opens a real external Chrome window separate from the native workspace; the first integration embeds no interactive browser engine; static artifacts use native preview panes where supported, otherwise a private artifact plus a textual alternative. | P | V1 | B2 | 0035, 0005 |
| R21 | Interactive Moli previews, screencasting, a Moli-owned window, shared cookie jars, reuse of the user's personal Chrome profile, simultaneous agent and user control of one page, a universal browser abstraction, site-capability heuristics, anti-bot guarantees, arbitrary multi-step automation through a JavaScript escape, and automatic download, installation or update of Moli, Chrome or Patchright are outside the first integration, as deliberate simplifications. | P | — | — | 0035, 0036 |

### 2.4 Process ownership and lifecycle

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R22 | The Shinatama process of the current window owns each request and its whole child tree, recording OS process, group or job identity, not just a PID, so cleanup never kills an unrelated process through PID reuse; there is no cross-window browser daemon. A session belongs to a project and, when present, to an instance and thread; it is not a native agent session. | P | V1 | B7 | 0036 |
| R23 | States: `requested → authorized → starting → active → closing → closed`, with `denied` and `failed` terminal; `needs_user` and `unsupported` are explicit outcomes, never silent retries; every effect is idempotent by `request_id`. | P | V1 | B7 | 0036 |
| R24 | Normal close: stop accepting actions, close pages and context, wait with a timeout, terminate remaining descendants, close endpoints, remove temporary material by policy, then report `closed`. Cancellation and errors use the same path with the original error preserved; an owner crash is covered by OS job or process-group cleanup and restart reconciliation. `closed` is never reported while a browser, companion or endpoint is alive. After a UI close no background companion remains; only an explicit persistent profile may remain on disk. | P | V1 | B7 | 0036 |
| R25 | One-shot requests run independently within explicit resource limits, including a bounded number of concurrent fetches; a UI session has one owner and one controller; starting another UI session never silently steals focus, profile or control; while the user controls a page agent actions are suspended, control returns to the agent only explicitly and after a check against the current session and page state; closing or replacing a session invalidates outstanding actions, approvals and queued results; there is no exactly-once claim for external web effects. | P | V1 | B9 | 0036 |

### 2.5 Authority, local resources and privacy

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R26 | HTML, JavaScript, screenshots, downloads, redirects, MCP UI content and agent output are untrusted: they cannot authorize navigation outside scope, start Chrome, change the controller or request credentials; the rules of technical-design 7.6 apply to browser results as to peer messages. | P | V1 | B6 (SEC) | 0036 |
| R27 | Only explicit local roots are readable; authorizing one HTML file does not expose the project root; downloads, file URLs, symlink resolution and subresources follow the same canonical-path policy; `localhost` is not blanket permission, and local services, redirects and subresources each need explicit scope. | P | V1 | B8 (SEC) | 0036 |
| R28 | Network access is allowed only to the intended destinations and address classes, applied to the initial URL, redirects, subresources and local files; because Moli documents that proxy-resolved targets escape client-side blocking, a proxy enforces the equivalent. | P | V1 | B8 | 0036 |
| R29 | Profiles are temporary per session by default; persistent profiles are separate per engine and explicitly consented; no personal Chrome profile, concurrent sharing or cross-engine cookie migration. | P | V1 | B8 | 0036 |
| R30 | No cookie, token, authorization header or credential enters logs, SQLite, transcripts, summaries or error reports; screenshots and extracted pages can contain secrets, so they are stored privately with bounded retention and never sent to a title or summary provider by default. | P | V1 | B8 | 0036, 0009 |
| R31 | Login and MFA are user-driven; purchases, submissions, messages, downloads with side effects and permission changes need the applicable user authorization, which no page or agent can supply. | P | V1 | B6 | 0036 |
| R32 | Policy enforcement is claimed only for the managed path: an agent with an arbitrary shell and equal OS privileges can bypass this broker, and OS isolation against it is not claimed. | P | V1 | X5 | 0036 |

### 2.6 Configuration and consent

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R33 | Settings: `browser.default_engine = moli`; `browser.patchright.enabled = false` (user opt-in); `browser.patchright.runtime` (builder's packaging); `browser.local_roots = []`; `browser.services = []`; `browser.profiles = temporary`; `browser.max_concurrent_fetch` bounded. | P | V1 | — | 0035 |
| R34 | Consent for the UI path has two levels: the one-time opt-in of `browser.patchright.enabled`, then an explicit request for each UI session. While the setting is off, `open-ui` returns `unsupported` explaining how to enable it, and nothing is downloaded or started; enabling it starts nothing until a session is requested. | U 2026-11-01 | V1 | B5 | 0035, 0036 |
| R35 | Versions, checksums, licences and update behaviour of the browser components are pinned and recorded; an optional component is never downloaded because `/browse` was invoked; a missing UI component returns an explanatory `unsupported`. | P | V1 | B5, X3 | 0036 |
| R36 | The base startup gate is observable without a baseline: no Moli, Patchright, companion or Chrome process starts until requested, and enabling browser support does not start the UI path. | P | V1 | B5 | 0036 |

### 2.7 Evidence and gates

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R37 | No speed or memory saving is claimed before B10 measures equivalent workloads on the whole process tree, companion included, with frozen versions, hardware, OS, fixture and cold/warm protocol; upstream Moli figures and arbitrary percentages are not acceptance evidence. | P | V1 | B10 | 0036 |
| R38 | If Moli fails the priority use cases or B10, the gate defers rather than withdraws: Moli stays the default, its fast path is not enabled until it passes, meanwhile only the explicit Patchright UI or no browser is available, and no other engine becomes the default. | U 2026-11-01 | V1 | B1, B3, B10 | 0035 |
| R39 | If only the UI path fails, it is deferred and the graphical requirement recorded as open; a failed `serve` gate limits persistent Moli automation and does not invalidate successful one-shot `fetch` cases. | P | V1 | B4, B5 | 0036 |
| R40 | Restoring a saved layout never reopens an interactive browser session (FRD-0001 R30). | U 2026-10-29 | V1 | TL3 | 0036 |
| R41 | In the suggested order of work the browser fast path comes first and the UI path follows its gates; neither blocks the first usable release slice. | A; U 2026-10-31 (not blocking) | V1 | — | 0040 |
| R42 | B1 to B10 are normative: local HTML extraction and optional rendering correct (B1, B2); the MCP use case within one-shot `fetch` or deferred (B3); `serve` and companion transports isolated before persistent automation (B4); Patchright absent at startup and lazy (B5); routing and authority unchangeable by content (B6); whole-tree lifecycle on three systems (B7); profile, scope and artifact privacy (B8); UI concurrency and control (B9); reproducible whole-tree performance (B10). | P | V1 | B1 to B10 | 0035, 0036 |

### 2.8 Open decisions that stay the user's

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R43 | Whether the reduced first integration (one-shot `fetch` plus explicit Patchright UI, without `serve` or interactive Moli previews) is sufficient, and the performance budget after a baseline exists, are the user's decisions; overall approval is O5. | S (open O8) | — | O8 | 0035 |
| R44 | Builder choices subject to the gates: Node.js or Python for the companion, the private transport, exact schema names, OS process cleanup, artifact viewer and retention defaults. | P | V1 | — | 0036 |

### 2.9 What was not verified

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R45 | No Moli, Patchright or Chrome binary has been installed or run for the proposal; no endpoint, exploit, traffic, rendering, compatibility, sandbox, cleanup or performance claim has been verified locally; source reading is not a pass. | S | — | B1 to B10 | 0010 |
| R46 | `dsh web` (FRD-0001 R07) is an "open in the user's browser" action and not a browser request of this record; it is never embedded. | A | V1 | DS14 | 0007 |
| R47 | The companion's web grounding (FRD-0012) uses the companion harness's own web tool, not this broker, and is governed by ADR-0043. | U 2026-11-01 | V1 | CS-19 | 0043 |
| R48 | Hostile-content tests include hostile pages, redirects and errors trying to change engine or scope, open Chrome or answer approvals, and a post-error switch that must be a new explicit request. | P | V1 | B6 | 0036 |
| R49 | Lifecycle tests cover normal close, cancellation, startup failure, owner crash and PID reuse on Windows, macOS and Linux. | P | V1 | B7 | 0036 |
| R50 | Scope tests cover symlinks, subresources, redirects, local services, proxies, temporary and persistent profiles, cookies and screenshots. | P | V1 | B8 | 0036 |
| R51 | Control tests cover a second session, user takeover, agent resume and a stale queued action after close or control change. | P | V1 | B9 | 0036 |
| R52 | The threat model of register X5 covers the browser broker with the emulator, the database, MCP and hooks, handoff files, local sockets and the widget. | S | V1 | X5 (SEC) | 0036 |

## 3. Open questions

1. Sufficiency of the reduced first integration and the performance budget (register O8).
2. Overall design approval (register O5).

## 4. Acceptance

AT-11 (browser boundaries) and B1 to B10. Nothing has run.

## 5. Relation to the JSON record

FRD-0011.json holds 72 extracted sentences; this record covers them all, folding the
non-goals into R21 and the gate rows into 2.7 and 2.9.
