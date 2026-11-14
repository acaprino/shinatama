# Moli browser pane: design

| | |
|---|---|
| Date | 2026-11-14 |
| Status | Proposed, awaiting the user's approval |
| Decision record | ADR-0056 (Proposed) |
| Changes | FRD-0011 (v2), functional-design 5.15, technical-design 7.9 |

## 1. Intent

The user, 2026-11-14: "shinatama deve avere un browser embedded, molto low memory per agenti e
navigazione". After an online survey (WebView2 child webview, Servo, Lightpanda, Moli) the user
chose **Moli** as the engine.

Shinatama gets a browser pane inside the workspace. The user browses in it; agents drive it
through fixed operations; the whole thing costs a fraction of Chrome's memory. Moli
(lexmount/moli, Rust, Apache-2.0 OR MIT) is headless by design (no GUI, no persistent window,
no GPU compositor), so **Shinatama draws the pane**: it shows Moli's software-rendered frames and
forwards mouse and keyboard over CDP.

## 2. Goals, non-goals, anti-goals

Goals
- A browser pane in a thread's layout (ADR-0055), next to terminals: address bar, back, forward,
  reload, page area, controller badge.
- Agents open, read, click, type and screenshot pages through MCP tools of `shinatama mcp`,
  in a pane the user can watch.
- One Moli process per window, started only when the first page opens, holding every page of
  that window.
- No TCP port: the browser is reachable only by the Shinatama process that started it.

Non-goals
- Pixel parity with Chrome, video, high-fidelity Canvas/WebGL (Moli's declared limits).
- Replacing Patchright for login-heavy sites, CAPTCHA and media: it stays the explicit external
  path of FRD-0011.
- Linux and macOS (Windows only, ADR README 2026-11-02).

Anti-goals (a failure even if it works)
- Any listener another local process or a web page can reach without the per-launch secret.
- A Moli or Chrome process alive at Shinatama startup or after its window closes.
- Page content executing inside Shinatama's own WebView2: pages arrive only as images.
- Automatic switch to Chrome when Moli fails (FRD-0011 R03 stays).

## 3. Approaches considered

| | Approach | Trade-off | Outcome |
|---|---|---|---|
| A | WebView2 child webview in a separate profile | Full compatibility, ~20-30 MB per window plus page, but pages run in the same runtime as Shinatama's UI | Rejected by the user in favour of Moli |
| 1 | `moli serve` on a random loopback port | No code on Moli, but any local process can drive it: Moli checks Host and Origin, no credential (`moli-protocol-server/src/protocol_server/request_guard.rs`) | Rejected: violates FRD-0011 R18 |
| 2 | Moli crates linked into `Shinatama.exe` | No port, but V8 and libcurl in the app: a page crash kills the app, heavy build | Rejected |
| **3** | **`shinatama-moli.exe`, our host built from Moli's crates, serving Moli's protocol router over a private named pipe** | No port, crash isolated, same memory; we track Moli updates | **Chosen** |

## 4. Design

### 4.1 Components

```
Shinatama.exe (window)                         shinatama-moli.exe (one per window, Job Object)
  browser pane (TS)  --Tauri events-->           named pipe listener (DACL: current user only,
  browser broker (Rust) --CDP over pipe-------->   remote clients rejected, token handshake)
     ^                                           Moli ProtocolServer router (CDP)
     | broker pipe (instance token)              Moli kernel: DOM, Stylo, V8, libcurl,
  shinatama mcp (agent) browser_* tools            Taffy/Parley layout on demand, CPU paint
```

- **`sidecars/moli-host/`**: a separate Cargo workspace, outside the root workspace, so V8,
  libcurl and Moli's lockfile never enter `Cargo.lock` of Shinatama. It builds
  `shinatama-moli.exe` from Moli crates pinned to one commit.
- **Moli patch**: `ProtocolServer::serve_with_control` binds a `TcpListener` itself and
  `serve_listener` is private (`moli-protocol-server/src/protocol_server/mod.rs`). The host needs
  one public entry, `serve_on<L: axum::serve::Listener>(listener, local_addr, control)`, with a
  synthetic loopback `local_addr` for `RequestGuard`. The patch is offered upstream as a PR; until
  merged, the pin points at a fork commit carrying only that patch.
- **`crates/browser`** gains the broker: the pipe client, the CDP session per page, screencast
  and input relay, the controller state and the agent operations. It still decides routing as
  today (F17).

### 4.2 Transport (replaces the B4 gate for this path)

- The desktop creates the pipe name `\\.\pipe\shinatama-moli-<128-bit random>` and a 256-bit
  token, starts the host with both on stdin (never on the command line or environment), inside a
  Job Object with `KILL_ON_JOB_CLOSE` owned by the window.
- The host creates the pipe with `first_pipe_instance(true)`, `reject_remote_clients(true)` and a
  DACL granting only the current user's SID.
- Every connection must send the token as its first 32 bytes; the host's listener compares it in
  constant time and drops the connection otherwise, before any byte reaches the Moli router.
- Only the desktop broker connects. Agents never see the pipe name or the token.
- `moli serve` and the `moli` CLI binary are never started for this path.

### 4.3 The pane

- New pane kind `browser` in the workspace layout of a thread (ADR-0055); restoring a layout
  shows the last URL with a "Riapri" button and never reopens the page (FRD-0001 R30, FRD-0011
  R40).
- Page area: a canvas showing frames from `Page.screencastFrame`; the viewport follows the pane
  size times the device pixel ratio (`Emulation.setDeviceMetricsOverride`).
- Input: mouse and wheel through `Input.dispatchMouseEvent`, keys through
  `Input.dispatchKeyEvent` and `Input.insertText`; after each input batch the broker asks
  `Page.captureScreenshot` for an immediate fresh frame, because the screencast interval is fixed
  at host start (`--screencast-interval-ms`, default 1000 in `moli/src/cli.rs`) and only emits
  when the page changed.
- Hidden panes (another thread open, pane collapsed) stop their screencast; the page stays.
- Glass chrome and in-place sheets as every other pane (ADR-0039 v2); no popups.

### 4.4 Lifecycle and memory

- The host starts on the first page of the window and closes when the window closes, or after
  `browser.moli.idle_minutes` (default 10) with no page open.
- All pages of the window are CDP targets of the same host: one process, at most
  `browser.moli.max_pages` (default 6).
- Default Moli policy: no real layout until the page is shown or a screenshot is asked (the host
  runs with `LayoutPolicy::OnDemand`, `--layout` equivalent); optional resources (images, fonts)
  on for user-visible pages, off for agent reads.
- Profile: a temporary storage partition per host by default; a persistent profile per project
  under `SHINATAMA_HOME` only by explicit setting, never shared with Patchright (FRD-0011 R29).
- Memory figures are claimed only after measurement of the whole tree (FRD-0011 R37).

### 4.5 Agents

New tools of `shinatama mcp`, all scoped by FRD-0011 R10 (inside a user-granted task and target
scope):

| Tool | Effect |
|---|---|
| `shinatama_browser_open(url)` | Opens a page in a browser pane of the agent's thread, controlled by the agent; returns `page_id`. |
| `shinatama_browser_read(page_id, format)` | `markdown`, `text` or `tree` (accessibility-like snapshot with element refs). |
| `shinatama_browser_click(page_id, ref)` | Clicks an element ref from the last `tree`. |
| `shinatama_browser_type(page_id, ref, text, submit)` | Focuses and types. |
| `shinatama_browser_screenshot(page_id)` | Returns a PNG (layout on demand). |
| `shinatama_browser_close(page_id)` | Closes the page. |

- `shinatama mcp` reaches the window through a **broker pipe** per window; it presents
  `SHINATAMA_INSTANCE_ID` and `SHINATAMA_INSTANCE_TOKEN`, which the desktop verifies the way
  `shinatama mcp` verifies them today; identity is derived server-side (FRD-0011 R12).
- No raw CDP, no `eval` for agents (FRD-0011 R16 spirit).
- One controller per page (FRD-0011 R25): when the user acts on an agent page, an inline strip
  "L'agente sta usando questa pagina · Prendi il controllo" appears; taking control suspends the
  agent's actions (they return `needs_user`); "Restituisci" hands it back explicitly.
- Page content returned to agents is untrusted data (FRD-0011 R26).
- One-shot `read` without a pane stays `moli fetch` (F18), unchanged.

### 4.6 What stays from FRD-0011

Moli is the default engine; Patchright with visible Chrome only on explicit request and opt-in;
no automatic fallback; no cookie or session transfer between engines; no secrets in logs or
SQLite; window-owned process tree with whole-tree cleanup.

## 5. Constraints

- Windows only; Tauri 2.9; the WebView2 binding is the one Tauri links.
- Moli is a component, not a harness: it is pinned by commit and checksum (FRD-0011 R35); an
  update bumps the pin and requalifies.
- `shinatama-moli.exe` ships next to `Shinatama.exe` and updates in place in
  `D:\Software\Shinatama`, never backed up (CLAUDE.md).
- Licences: Moli Apache-2.0 OR MIT, plus its bundled V8 (BSD-3), libcurl (curl licence) and the
  notices under Moli's `licenses/`, added to `THIRD_PARTY_NOTICES.md`.

## 6. Gates (spike first)

| Gate | Proof |
|---|---|
| M1 | `shinatama-moli.exe` builds on Windows x64 from the pinned commit with the `serve_on` patch; size recorded. |
| M2 | Memory of the whole tree: idle host, one page, four pages, measured against Chrome on the same fixtures (replaces nothing of R37: it is R37's measurement). |
| M3 | Input latency click → fresh frame on a local fixture and two real sites. |
| M4 | Pipe security: another user's process, a remote client, a wrong token and a second pipe instance are all refused; no TCP listener in `netstat`. |
| M5 | Window close, host crash and Shinatama crash leave no `shinatama-moli.exe`. |
| M6 | Hostile page tests of FRD-0011 R48 against the pane and the agent tools. |

## 7. Left to the builder

CDP client crate, frame encoding to the TS canvas (Tauri channel with JPEG bytes), the exact
`tree` format and ref scheme, the broker pipe wire format, IME handling, cursor shapes, the
default screencast interval for visible panes, retention of screenshots (FRD-0011 R30).
