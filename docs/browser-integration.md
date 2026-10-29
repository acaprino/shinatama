# Browser integration: Moli and Patchright UI

Date: 2026-10-29
Status: proposed, not approved. Nothing is built. This document amends the
[functional design](functional-design.md) (goal 13, section 5.15) and the
[engine design](technical-design.md) (section 7.9). Verification rows B1–B10 in
[the register](verification-register.md) are part of this specification.

## 1. Intent and decisions

The user wants browser access for both hosted agents and the user, without paying
Chrome's cost when Chrome is unnecessary. The ordinary path must be light: local
HTML documents, extraction, static previews, web interfaces of MCP servers and
other bounded browser operations.

The following are **user requirements**, decided 2026-10-29:

1. **Moli is the default** for fast operations, local HTML, previews and MCP web
   interfaces. Layout and visual resources are enabled only when the requested
   operation needs them.
2. **Patchright with a visible Chrome window** is the graphical path for explicit
   UI use, login/MFA/CAPTCHA and pages the user chooses to open there.
3. **No automatic fallback.** Shinatama must not infer that an error, timeout,
   empty page or anti-bot response authorizes a switch to another engine.
4. **No automatic transfer** of pages, cookies, credentials, sessions or control
   between engines.
5. **No resident Chrome or Patchright companion.** The graphical path is loaded
   and started only on an explicit request. Nothing is installed automatically.
6. **Intent-based routing.** A preinstalled skill or `/browse` command may choose
   the engine from the user's request. Web content and agent output cannot change
   the engine or its permissions.

The shape below is the proposed implementation of those requirements, refined by
an Ultra Nerd review. It is not yet an approved build plan.

## 2. Scope and non-goals

### In the first integration

- One-shot Moli operations through `moli fetch`: text/structured extraction and,
  where verified, screenshots or other bounded artifacts.
- Read-only access to explicitly authorized local HTML roots and explicitly
  authorized local or remote HTTP(S) targets.
- Static preview artifacts shown where the terminal supports them, otherwise
  exposed as an artifact with a clear textual alternative.
- A separate Patchright UI session started only by the user, with its own profile
  and explicit lifecycle.
- A small common request/result contract that preserves unsupported capabilities
  instead of pretending the engines are interchangeable.

### Outside the first integration

- `moli serve` and persistent CDP/WebDriver automation.
- Interactive Moli previews embedded in the TUI, continuous screencasting or a
  browser window owned by Moli.
- Automatic fallback, automatic engine migration, shared cookie jars and automatic
  reuse of the user's personal Chrome profile.
- Simultaneous agent/user control of one page.
- A universal browser abstraction, site-capability heuristics, anti-bot guarantees
  or arbitrary multi-step agent automation through a general JavaScript escape.
- Automatic download, installation or update of Moli, Chrome or Patchright.

These exclusions are intentional simplifications, not features silently missing
from an otherwise complete browser integration.

## 3. Vocabulary and session types

- **Browser request** `br_…`: one accepted request with an immutable engine,
  origin, operation, target scope and permission snapshot.
- **Browser session** `bs_…`: a live one-shot operation or an explicit UI session,
  owned by one Shinatama window. A session belongs to a project and, when present,
  to an instance/thread; it is not a native agent session.
- **Controller**: `agent` or `user`, exactly one for an interactive session. A
  one-shot `fetch` has no interactive controller.
- **Scope**: the local roots, destination rules, subresources, redirects and
  operations authorized for that request. `localhost` is not itself a scope.
- **Fast path**: Moli `fetch`, without `moli serve` or a Chrome/Patchright process.
- **UI path**: Patchright driving visible Chrome, started only for an explicit UI
  request.

A one-shot operation ends with its result and process cleanup. A UI session may
survive multiple user actions until it is closed, its owner exits or its lease is
revoked. Neither creates a shared resident service.

## 4. User and agent surfaces

### 4.1 `/browse`

`/browse` is the proposed single entry point. Its final spelling is a builder's
choice, but its semantics are fixed:

- `read`: extract bounded text or structured content through Moli.
- `preview`: produce a static visual artifact through Moli, enabling layout only
  for that operation.
- `open-ui`: open a Patchright/Chrome UI session; requires an explicit user
  request.
- `--engine=moli|patchright`: optional explicit override, not a security boundary.
- A free-form request may be classified by the preinstalled skill. The selected
  engine, reason, operation and scope are shown before execution.

An agent may request `read` or `preview` only inside an existing user-authorized
task and target scope. It need not ask again for every navigation already inside
that grant. It cannot grant itself `open-ui`, broaden scope or answer its own
permission request. It may ask the user to issue a new explicit UI command.

### 4.2 TUI and overview

The session view and overview show engine, target origin/root, operation, state,
controller and reason. A Moli failure offers no automatic Chrome action; it may
show the exact new command the user can choose. A visual request is not reported
as visually satisfied when only text was produced.

Patchright opens a real external Chrome window. It does not turn Shinatama's TUI
into a graphical window and does not depend on terminal image support.

## 5. Routing and request contract

### 5.1 Decision table

| Request | Engine | Authorization |
|---|---|---|
| Read/extract local HTML or an authorized web target | Moli | Existing task/request scope |
| Static preview | Moli | Same scope plus artifact permission |
| MCP web interface, bounded one-shot operation | Moli | Explicit local service/target scope |
| Visible browser, login, MFA, CAPTCHA | Patchright UI | Explicit user request |
| Ambiguous, non-sensitive read | Moli | Narrowest applicable scope |
| Ambiguous sensitive action or broad local access | None | Ask for clarification |
| Moli reports unsupported/failed | Keep Moli; stop | A new explicit user command is required to use Patchright |

Routing happens before loading the target and is immutable for that request. A
page, redirect, script, error message, tool result or peer message is data, never
routing authority. Starting another engine is a **new request**, not continuation
of the failed request.

### 5.2 Logical input and result

The transport schema is left to the builder; these fields are required:

```
BrowseRequest {
  request_id, schema_version, origin: user|agent,
  instance_id?, thread_id?, project_id,
  operation: read|preview|open_ui,
  engine: moli|patchright,
  target, scope_id, permission_snapshot,
  reason, requested_capabilities[]
}

BrowseResult {
  request_id, session_id?, engine,
  outcome: ok|unsupported|needs_user|denied|failed,
  artifacts[], error?, cleanup_state
}
```

The runtime validates engine, operation, identity and scope. The skill's prose,
`SHINATAMA_INSTANCE`, a caller-supplied instance id and an `--engine` flag are not
authentication. The MCP/per-instance integration must authenticate its caller and
derive identity server-side; that mechanism is unverified.

Results distinguish unsupported functionality, required user intervention,
permission denial and execution failure. They never convert `failed` into an
implicit retry on the other engine.

## 6. Engine boundaries

### 6.1 Moli fast path

Start the pinned Moli binary as a child for a bounded `fetch`; do not start
`moli serve`. Use fixed operations and output parsers. General `--eval` access is
not exposed to agents in the first integration; a probe may use it locally to
measure what the MCP use case would require.

Default behavior:

- no layout, images, fonts or other optional resources for text extraction;
- layout/resources only for operations that need them;
- bounded time, output, response size, redirects and artifact count/size;
- a temporary dedicated profile unless the user explicitly configures persistence;
- no inherited agent credentials or unrelated environment secrets;
- target and subresource policy applied to the initial URL, redirects and local
  files, including the documented proxy limitation below.

Moli's upstream performance figures are not Shinatama acceptance evidence.

### 6.2 `moli serve` gate

At upstream commit `dcae77e5aa27ef645b819773f8420750bc06e291`, source inspection
found a TCP HTTP/WebSocket server, default `127.0.0.1:9222`, exposing CDP,
WebDriver Classic and BiDi. The inspected CLI/config/router files expose host,
port, timeout and screencast settings; no request-credential check was found on
the routes inspected. This is a scoped source finding, not a completed security
audit or a runtime exploit demonstration.

Therefore `serve` stays disabled until B4 proves a private authenticated transport
or an equivalent verified boundary, and proves that untrusted pages cannot reach
or control the endpoint. Loopback, an ephemeral port and a random path are not by
themselves authentication. This gate applies to Moli as well as Patchright; the
availability of `moli fetch` means a `serve` failure does not require rejecting
the entire fast path.

### 6.3 Patchright UI path

Patchright is a patched Playwright driver, not a browser UI by itself. A Shinatama
companion, using the supported Node.js or Python package chosen by the builder,
starts visible Chrome/Chromium through a persistent context only after an explicit
UI request. The companion and its dependencies are lazy-loaded; no Node/Python
helper, Chrome process or update check runs at Shinatama startup.

The companion exposes only the typed session operations needed by the UI path. It
does not expose a generic local automation socket. Its transport, opened ports,
parent/child process tree and cleanup must be inventoried and verified; calling a
transport “internal” is not evidence that no listener exists.

## 7. Process ownership and lifecycle

The Shinatama process for the current window owns the browser request and its
whole child tree. There is no cross-window browser daemon. Ownership records the
OS process/group/job identity, not just a PID, so PID reuse cannot make cleanup
kill an unrelated process.

Required states:

```
requested -> authorized -> starting -> active -> closing -> closed
                   |            |        |
                   v            v        v
                 denied       failed   failed
```

`needs_user` and `unsupported` are explicit outcomes, not silent retries. Every
effect is idempotent by `request_id`: a repeated result or shutdown does not open
another browser or delete another session's artifacts.

- Startup: validate policy, create the temporary profile and scope, then spawn.
- Normal close: stop accepting actions, close pages/context, wait with a timeout,
  terminate remaining descendants if necessary, close endpoints, remove temporary
  material according to policy, then report `closed`.
- Cancellation/error: the same cleanup path, with the original error preserved.
- Owner crash: OS-specific job/process-group cleanup and restart reconciliation
  must prevent or identify orphans. Merely being a child process is not proof.
- UI close: no background companion remains. An explicit persistent profile may
  remain on disk; a temporary one is handled by the retention policy.

Cleanup status is reported honestly: `closed` is not claimed while a browser,
companion or endpoint is still alive.

## 8. Authority, local resources and privacy

1. **Untrusted content.** HTML, JavaScript, screenshots, downloads, redirects,
   MCP UI content and agent output cannot authorize navigation outside scope,
   start Chrome, change controller or request credentials. The rules of engine
   section 7.6 apply to browser results as well as peer messages.
2. **Local files.** Only explicit roots are readable. Do not expose the project
   root by implication when a single HTML file was authorized. Downloads, file
   URLs, symlink resolution and subresources require the same canonical-path
   policy.
3. **Network.** Allow only the intended destinations and address classes. Moli's
   `--block-private-networks` and `--block-cidrs` are available upstream, but its
   CLI documents that proxy-resolved targets are outside client-side network
   blocking. A proxy therefore needs its own equivalent enforcement.
4. **Profiles.** Temporary per-session profiles by default. Persistent profiles
   are separate per engine and explicitly consented scope; no personal Chrome
   profile, concurrent profile sharing or cross-engine cookie migration.
5. **Secrets and artifacts.** No cookies, tokens, authorization headers or
   credentials in logs, SQLite, transcripts, summaries or error reports. Screenshots
   and extracted pages can contain secrets: store them privately, bound their
   retention and never send them to a title/summary provider by default.
6. **Sensitive effects.** Login/MFA is user-driven. Purchases, submissions,
   messages, downloads with side effects and permission changes require the
   applicable user authorization; a page or agent cannot supply it.
7. **Local hostile agents.** An agent with an arbitrary shell and the same OS
   privileges can bypass this broker. Resisting such an agent requires verified
   OS isolation, not a stronger skill prompt. The first integration claims policy
   enforcement only for its managed path.

## 9. Concurrency and control

One-shot Moli requests may run independently within explicit resource limits. A
UI session has one owner and one controller. Starting another UI session must not
silently steal focus, profile or control; refuse it or require an explicit new
session according to the UI policy.

When control is `user`, agent actions on that page are suspended. Returning
control to the agent is explicit and checked against the current session and page
state. Closing or replacing a session invalidates outstanding actions, approvals
and queued results. There is no exactly-once claim for external web effects.

## 10. Configuration, installation and performance evidence

Proposed settings:

```
browser.default_engine = moli
browser.patchright.enabled = false        # user opt-in
browser.patchright.runtime = <managed>   # builder's packaging decision
browser.local_roots = []
browser.services = []                     # explicit MCP/local targets
browser.profiles = temporary
browser.max_concurrent_fetch = <bounded>
```

Versions, checksums, licenses and update behavior are pinned and recorded. An
optional component is never downloaded merely because `/browse` was invoked; a
missing UI component returns an explanatory `unsupported` result.

The base startup gate is observable without a performance baseline: no Moli,
Patchright, companion or Chrome process is started until requested, and enabling
browser support does not itself start the UI path.

For performance comparisons, first freeze Moli/Patchright/Chrome versions,
hardware, OS, fixture, cold/warm protocol and equivalent workloads. Then measure
startup, end-to-end latency and memory of the **whole process tree**, including
the companion. No arbitrary saving percentage or latency target is accepted as
evidence before that baseline exists.

## 11. Acceptance gates

B1–B10 in the register are normative. In short:

- local HTML extraction and optional static rendering are correct;
- the MCP UI use case is either demonstrated within the one-shot boundary or
  explicitly deferred, never silently replaced by `serve`;
- `serve` and companion transports pass endpoint isolation and hostile-content
  tests before persistent automation is enabled;
- routing and permissions cannot be changed by pages/errors/agent output;
- Patchright starts only explicitly, with profile separation and no startup
  residency;
- close, cancellation and crash leave no managed descendant or endpoint;
- concurrent UI actions cannot steal control or apply stale approvals;
- performance is reported from reproducible whole-tree measurements.

If Moli fails the priority use cases or shows no accepted benefit, do not make it
the default. If only the UI path fails, defer it and record that the graphical
requirement remains open. A failed `serve` gate limits persistent Moli automation;
it does not invalidate successful one-shot `fetch` cases.

## 12. Evidence and open decisions

Evidence read for this proposal:

- Shinatama: [functional design](functional-design.md), especially sections 2,
  5.1 and 5.14; [engine design](technical-design.md), sections 3, 7.5 and 7.6.
- Moli README and source at commit
  [`dcae77e5aa27ef645b819773f8420750bc06e291`](https://github.com/lexmount/moli/tree/dcae77e5aa27ef645b819773f8420750bc06e291):
  [`moli/src/cli.rs`](https://github.com/lexmount/moli/blob/dcae77e5aa27ef645b819773f8420750bc06e291/moli/src/cli.rs),
  [`moli/src/config.rs`](https://github.com/lexmount/moli/blob/dcae77e5aa27ef645b819773f8420750bc06e291/moli/src/config.rs),
  [`moli-protocol-server/src/config.rs`](https://github.com/lexmount/moli/blob/dcae77e5aa27ef645b819773f8420750bc06e291/moli-protocol-server/src/config.rs),
  [`protocol_server/mod.rs`](https://github.com/lexmount/moli/blob/dcae77e5aa27ef645b819773f8420750bc06e291/moli-protocol-server/src/protocol_server/mod.rs),
  [`cdp_socket.rs`](https://github.com/lexmount/moli/blob/dcae77e5aa27ef645b819773f8420750bc06e291/moli-protocol-server/src/protocol_server/cdp_socket.rs).
- Patchright official [Python](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright-python)
  and [Node.js](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright-nodejs)
  READMEs.

No Moli, Patchright or Chrome binary was installed or run. No endpoint, exploit,
traffic, rendering, compatibility, sandbox, cleanup or performance claim has been
verified locally.

Builder choices, subject to the gates: Node.js versus Python for the companion,
private transport implementation, exact command/schema names, OS process cleanup,
artifact viewer and retention defaults. User decisions still open: whether the
reduced first integration without `serve`/interactive Moli previews is sufficient,
and the performance budget after a baseline exists. Overall design approval remains
open in O5.
