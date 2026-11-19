# Moli Browser Tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Browser tabs inside Shinatama, drawn from a per-window Moli host reached only over a private named pipe, opened by every http(s) link and HTML document, tileable like terminals, and drivable by agents through fixed MCP tools.

**Architecture:** `shinatama-moli.exe` (own Cargo workspace `sidecars/moli-host/`, Moli crates pinned) serves Moli's CDP router over a token-checked named pipe. The desktop owns one host per window in a Job Object; `crates/browser` holds the CDP client, the page set and the broker; the TS `browser` view paints screencast frames on a canvas and forwards input. Links from terminals, the Files panel and `dsh web` open new user-controlled browser tabs.

**Tech Stack:** Rust 2024 (Moli crates, axum 0.8 `serve::Listener`, tokio named pipes, tokio-tungstenite), Tauri 2.9 IPC channels, TypeScript + xterm link providers.

**Spec:** `docs/superpowers/specs/2026-11-14-moli-browser-pane-design.md` (ADR-0056)

## Global Constraints

- Windows x64 only; no TCP listener anywhere on the browser path; `moli serve` and the `moli` binary are never started.
- Pipe: `\\.\pipe\shinatama-moli-<128-bit random hex>`, `first_pipe_instance(true)`, `reject_remote_clients(true)`, DACL = current user's SID only; 256-bit token sent as the first 32 bytes of every connection, compared in constant time; name and token on stdin only.
- One host per window, Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; started on the first page; ends with the window or after `browser.moli.idle_minutes` = 10 with no page.
- `browser.moli.max_live_pages` = 8; the 9th discards the least recently shown hidden page (tab keeps title and URL, reloads when shown).
- Moli pinned by commit and SHA-256 of the built exe; a bump requalifies (FRD-0011 R35). Moli is not a harness: the CLAUDE.md any-version rule does not apply to it.
- Page content never executes in Shinatama's WebView2; it arrives only as image bytes.
- Only `http`, `https` and local `.html`/`.htm` open; a local document grants read of its own folder subtree for that tab only.
- No automatic fallback to Chrome; "Apri nel browser di sistema" hands the URL only.
- No popups (ADR-0039 v2): strips and sheets in place, glass chrome.
- New Rust dependencies go through `docs/implementation/dependency-policy.md` and `python scripts/dependencies.py check`; licences into `THIRD_PARTY_NOTICES.md`.
- Install: copy built exes over `D:\Software\Shinatama`, never back it up.

## Review Focus

1. A web page or terminal output trying to reach the host: any other process connecting to the pipe without the token must get nothing (Task 2 test `foreign_client_gets_no_bytes`).
2. The host outliving its window after a Shinatama crash or kill (Task 4 test `killing_the_owner_kills_the_host`).
3. A Ctrl+click on an OSC 8 link whose visible text is a different URL: the hover and the address bar must show the real target (Task 7 test `osc8_hover_shows_the_real_target`).
4. Typing fast or pasting into a page: keystrokes must arrive in order and none dropped while a screenshot is in flight (Task 5 test `input_batches_keep_order_under_capture`).
5. Resizing a tile repeatedly: one viewport update after the resize settles, never a frame drawn at the wrong scale (Task 6 test `resize_settles_to_one_metrics_call`).

---

### Task 1: Moli `serve_on` patch and host skeleton (gate M1)

**Files:**
- Create: `sidecars/moli-host/Cargo.toml`, `sidecars/moli-host/rust-toolchain.toml` (copy Moli's toolchain file), `sidecars/moli-host/src/main.rs`, `sidecars/moli-host/moli-pin.json`
- Create (fork, branch `shinatama-serve-on`): `moli-protocol-server/src/protocol_server/mod.rs` patch in `acaprino/moli`
- Test: `sidecars/moli-host/tests/smoke.rs`

**Interfaces:**
- Produces (Moli patch): `impl ProtocolServer { pub async fn serve_on<L>(&self, listener: L, local_addr: SocketAddr, control: BrowserInstanceControl) -> anyhow::Result<()> where L: axum::serve::Listener }`. It builds `AppState` exactly as `serve_with_control` does with `local_addr`, then runs today's `serve_listener` body made generic over `L` (the `tap_io` TCP options apply only to the TCP path). `serve_with_control` becomes `TcpListener::bind` + `serve_on`.
- Produces (host CLI): `shinatama-moli.exe` reads one line of JSON on stdin: `{"pipe": "<name>", "token": "<64 hex>", "screencast_interval_ms": 250, "profile_dir": null}`; writes `{"ready": true}` on stdout once the pipe exists; exits when the pipe server ends or stdin closes.
- `moli-pin.json`: `{"repository": "https://github.com/acaprino/moli", "commit": "<sha>", "upstream_base": "<lexmount/moli sha>", "exe_sha256": "<filled by Task 10>"}`.

- [ ] **Step 1:** Fork `lexmount/moli` at today's `main`, apply the `serve_on` patch, run Moli's own `cargo nextest run -p moli-protocol-server`; expected: same pass count as unpatched. Open the upstream PR; record both SHAs in `moli-pin.json`.
- [ ] **Step 2: Write the failing test** `sidecars/moli-host/tests/smoke.rs::version_over_pipe`: spawn the built exe, send the stdin JSON with a random pipe and token, wait for `{"ready":true}`, connect a `NamedPipeClient`, write the token, send `GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n`, assert status 200 and a JSON body with key `"Browser"`.
- [ ] **Step 3:** Run `cargo test --manifest-path sidecars/moli-host/Cargo.toml --test smoke`; expected FAIL (no binary behaviour).
- [ ] **Step 4:** Implement `main.rs`: parse stdin, build `ProtocolServer::new_with_storage_partition_and_runtime_config` with `LayoutPolicy::OnDemand`, a temporary partition (`StoragePartitionState::open(None)`) or `profile_dir`, and `screencast_interval_ms`; call `serve_on(PipeListener, SocketAddr::from(([127,0,0,1], 9)), control)`. `PipeListener` is a stub here (accepts without the token check, Task 2 hardens it).
- [ ] **Step 5:** Run the test; expected PASS. Record the release exe size and the list of CDP domains Moli answers (`Schema.getDomains` or Moli's `moli-protocol-cdp-defs`) in `docs/verification/2026-11-14-M1-moli-host.md`, explicitly noting whether `Fetch.enable`/`Fetch.requestPaused` and `Page.startScreencast` exist (Task 8 depends on `Fetch`).
- [ ] **Step 6: Commit** `feat(browser): shinatama-moli host serving Moli over a named pipe (M1)`.

### Task 2: Pipe listener security (gate M4)

**Files:**
- Create: `sidecars/moli-host/src/pipe_listener.rs`
- Test: `sidecars/moli-host/tests/pipe_security.rs`

**Interfaces:**
- Produces: `pub struct PipeListener { /* name, token: [u8; 32], next: NamedPipeServer */ }`, `impl PipeListener { pub fn create(name: &str, token: [u8; 32]) -> io::Result<Self> }`, `impl axum::serve::Listener for PipeListener { type Io = NamedPipeServer; type Addr = SocketAddr; }`.

- [ ] **Step 1: Write the failing tests** in `pipe_security.rs`:
  - `foreign_client_gets_no_bytes`: connect, write 32 wrong bytes, then an HTTP request; assert the read returns 0 bytes or an error within 2 s.
  - `short_token_is_dropped`: write 31 bytes and close; the next correct client still works.
  - `second_server_instance_is_refused`: `ServerOptions::new().first_pipe_instance(true).create(name)` from the test on the same name fails with `ERROR_ACCESS_DENIED`.
  - `dacl_names_only_the_current_user`: `GetSecurityInfo` on a client handle; the DACL has exactly one ACE, for the current token user SID.
  - `no_tcp_listener`: while the host runs, `GetExtendedTcpTable` holds no row owned by its PID.
- [ ] **Step 2:** Run `cargo test --manifest-path sidecars/moli-host/Cargo.toml --test pipe_security`; expected FAIL.
- [ ] **Step 3:** Implement: create each instance with `ServerOptions` (`first_pipe_instance(true)` for the first, `reject_remote_clients(true)` always) and `create_with_security_attributes_raw` using an SDDL `D:P(A;;GA;;;<user SID>)`; in `accept`, read exactly 32 bytes with a 2 s timeout, compare with `subtle::ConstantTimeEq`, drop and loop on mismatch; pre-create the next instance before returning.
- [ ] **Step 4:** Run the tests; expected PASS.
- [ ] **Step 5: Commit** `feat(browser): token-checked, current-user-only pipe for the Moli host (M4)`.

### Task 3: CDP client over the pipe

**Files:**
- Create: `crates/browser/src/cdp.rs`
- Modify: `crates/browser/Cargo.toml` (tokio with `net`, `sync`, `time`, `io-util`, `macros`, `rt`; tokio-tungstenite; base64), `crates/browser/src/lib.rs` (`pub mod cdp;`), root `Cargo.toml` workspace dependencies
- Test: inline `#[cfg(test)]` in `cdp.rs`

**Interfaces:**
- Produces: `pub struct Cdp`, `impl Cdp { pub async fn connect<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(stream: S, token: &[u8; 32], path: &str) -> Result<Self, CdpError>; pub async fn call(&self, session: Option<&str>, method: &str, params: serde_json::Value) -> Result<serde_json::Value, CdpError>; pub fn events(&self) -> tokio::sync::broadcast::Receiver<CdpEvent> }`, `pub struct CdpEvent { pub session: Option<String>, pub method: String, pub params: serde_json::Value }`, `pub enum CdpError { Closed, Protocol { code: i64, message: String }, Timeout }`.
- `connect` writes the token, then performs the WebSocket upgrade on `path` (from `/json/version` `webSocketDebuggerUrl` path part).

- [ ] **Step 1: Write the failing tests** with a fake server on `tokio::io::duplex`: `call_matches_replies_by_id_out_of_order` (two calls, replies in reverse order, each gets its own result), `protocol_error_is_typed` (`{"id":1,"error":{"code":-32601,"message":"x"}}` → `CdpError::Protocol { code: -32601, .. }`), `events_carry_session` (an event with `sessionId` reaches `events()` with `session: Some`), `call_times_out_after_10s` (paused tokio time).
- [ ] **Step 2:** Run `cargo test -p shinatama-browser cdp`; expected FAIL.
- [ ] **Step 3:** Implement with one writer task and one reader task; pending calls in a `HashMap<u64, oneshot::Sender>`; 10 s timeout per call.
- [ ] **Step 4:** Run; expected PASS. Run `python scripts/dependencies.py check`; expected clean after `generate`.
- [ ] **Step 5: Commit** `feat(browser): CDP client for the Moli host`.

### Task 4: Host lifecycle in the desktop (gate M5)

**Files:**
- Create: `apps/desktop/src-tauri/src/browser_host.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs` (module, state, window-close hook), `apps/desktop/src-tauri/Cargo.toml` (`shinatama-browser`)
- Test: inline tests in `browser_host.rs` plus `crates/engine/tests/job_containment.rs` pattern for the kill test

**Interfaces:**
- Consumes: `shinatama_engine::resources::process::{NativeCommand, spawn_contained}` and the admission provider the sidecar supervisor uses (`crates/engine/src/sidecar_supervisor.rs:465-495`); `Cdp::connect` (Task 3).
- Produces: `pub struct BrowserHost`, `impl BrowserHost { pub async fn ensure(&mut self) -> Result<&Cdp, HostError>; pub fn touch(&mut self, pages_open: usize); pub fn shutdown(&mut self) }`, managed as `tauri::State<Mutex<BrowserHosts>>` keyed by window label. Executable path: `shinatama-moli.exe` beside the desktop exe.

- [ ] **Step 1: Write the failing tests:** `ensure_starts_once` (two `ensure` calls → one process key), `idle_ten_minutes_with_no_page_shuts_down` (injected clock: 9:59 alive, 10:00 gone), `killing_the_owner_kills_the_host` (spawn a helper process that owns a `BrowserHost`, kill it with `TerminateProcess`, assert no process with the host's key remains within 2 s), `window_close_leaves_no_host`.
- [ ] **Step 2:** Run `cargo test -p shinatama-desktop browser_host`; expected FAIL.
- [ ] **Step 3:** Implement: random pipe name and token from `getrandom`; spawn through `spawn_contained` with stdin JSON; wait for `{"ready":true}` (10 s); connect `Cdp`; `on_window_event(CloseRequested|Destroyed)` calls `shutdown`.
- [ ] **Step 4:** Run; expected PASS.
- [ ] **Step 5: Commit** `feat(desktop): per-window Moli host in a kill-on-close job (M5)`.

### Task 5: Page set, frames and input

**Files:**
- Create: `crates/browser/src/pages.rs`, `crates/browser/src/input.rs`
- Modify: `apps/desktop/src-tauri/src/browser_host.rs` (Tauri commands), `apps/desktop/src-tauri/src/main.rs` (register commands)
- Test: inline tests in `pages.rs` and `input.rs`

**Interfaces:**
- Produces (pure): `pub struct PageSet { max_live: usize }`, `impl PageSet { pub fn open(&mut self, url: Url, origin: Origin) -> (PageId, Option<PageId> /* discarded */); pub fn shown(&mut self, id: PageId); pub fn hidden(&mut self, id: PageId); pub fn controller(&self, id: PageId) -> Controller; pub fn take_control(&mut self, id: PageId); pub fn give_back(&mut self, id: PageId) }`, `pub enum Controller { User, Agent(String) }`, `pub enum PageState { Live { target: String }, Discarded { url: Url, title: String } }`.
- Produces (input): `pub enum InputEvent { Mouse { kind, x, y, button, buttons, modifiers, click_count }, Wheel { x, y, dx, dy }, Key { kind, key, code, text, modifiers }, Text(String) }`, `pub fn to_cdp(event: &InputEvent) -> (&'static str, serde_json::Value)`.
- Produces (Tauri commands, all `async`, page ids as strings): `browser_open(url: String) -> PageInfo`, `browser_navigate(page: String, action: "go"|"back"|"forward"|"reload", url: Option<String>)`, `browser_input(page: String, events: Vec<InputEvent>)`, `browser_resize(page: String, width: u32, height: u32, dpr: f64)`, `browser_visibility(page: String, visible: bool)`, `browser_close(page: String)`, `browser_external(page: String)`, `browser_frames(page: String, channel: tauri::ipc::Channel<Vec<u8>>)`. `PageInfo { id, url, title, controller, live }`.
- Frames: JPEG bytes from `Page.screencastFrame` (ack each with `Page.screencastFrameAck`) and from `Page.captureScreenshot` after every input batch; `browser_visibility(false)` sends `Page.stopScreencast`.

- [ ] **Step 1: Write the failing tests:** `ninth_page_discards_least_recently_shown_hidden` (8 live, open a 9th → the discarded id is the oldest hidden one, never a shown one), `discarded_page_reloads_when_shown`, `agent_actions_need_user_give_back` (after `take_control`, `controller` is `User` until `give_back`), `key_maps_to_dispatch_key_event` (`Key{kind: Down, key:"a", code:"KeyA", text:"a"}` → `("Input.dispatchKeyEvent", {"type":"keyDown","key":"a","code":"KeyA","text":"a",...})`), `input_batches_keep_order_under_capture` (a fake `Cdp` records calls: two batches sent while a capture is pending arrive in submission order, none dropped).
- [ ] **Step 2:** Run `cargo test -p shinatama-browser pages input`; expected FAIL.
- [ ] **Step 3:** Implement. Per page, one `tokio::sync::mpsc` queue serialises input batches and the capture that follows each batch.
- [ ] **Step 4:** Run; expected PASS.
- [ ] **Step 5: Commit** `feat(browser): page set with live-page limit, frames and input relay`.

### Task 6: Browser tab in the workspace, tiling and restore

**Files:**
- Create: `apps/desktop/src/browser-tab.ts`, `apps/desktop/src/browser-tab.css`, `apps/desktop/src/browser-input.ts`
- Modify: `apps/desktop/src/workspace-model.ts:306-309` (`panelAvailability`: a `browser` view is available when its `resourceId` is a live page id), `apps/desktop/src/workspace-renderer.ts` (mount `browser-tab` for `kind: 'browser'`), `apps/desktop/src/main.ts` (`openBrowserTab`)
- Test: `apps/desktop/tests/browser-input.test.ts`, `apps/desktop/tests/workspace-model.test.ts`

**Interfaces:**
- Consumes: Task 5 commands.
- Produces: `export function openBrowserTab(target: { url: string } | { file: string }): Promise<void>` in `main.ts` (creates a `browser` view; first one of the thread goes into a new group via the existing `split` action beside the focused group, later ones `add-view` into that group); `export function domToInputEvents(event: MouseEvent | WheelEvent | KeyboardEvent, rect: DOMRect, dpr: number): InputEvent[]` in `browser-input.ts`.
- Tab chrome: back, forward, reload, address field, controller badge, menu with "Apri nel browser di sistema"; inline strip "L'agente sta usando questa pagina · Prendi il controllo" / "Restituisci". A restored layout shows the saved URL and title with "Riapri" and no live page.

- [ ] **Step 1: Write the failing tests:** `browser-input.test.ts`: `click_maps_to_pressed_and_released_in_page_pixels` (clientX 110 in a rect at x 10, dpr 1.5 → x 150), `ctrl_a_keeps_modifiers`, `wheel_keeps_deltas`; `resize_settles_to_one_metrics_call` (ten resize notifications within 100 ms → one `browser_resize` call after 150 ms of quiet, with the last size); `workspace-model.test.ts`: `browser_view_is_available_only_when_live`, `restored_browser_view_is_not_live`.
- [ ] **Step 2:** Run `npm test`; expected FAIL.
- [ ] **Step 3:** Implement; canvas draws each JPEG via `createImageBitmap`; `ResizeObserver` debounced 150 ms; visibility from the renderer's `visible`/`hidden` flags calls `browser_visibility`.
- [ ] **Step 4:** Run `npm test` and `npm run build`; expected PASS.
- [ ] **Step 5:** Verify in the real app per memory "Shinatama UI verification" (build dist + exe with custom-protocol, drive over CDP with `SHINATAMA_HOME` outside git): open a page, split it beside a terminal, drag it to another group, maximize, collapse, restore the thread. Screenshot each state into `docs/verification/2026-11-14-browser-tab.md`.
- [ ] **Step 6: Commit** `feat(desktop): browser tabs that tile like terminal tabs`.

### Task 7: Links open in a browser tab (gate M7)

**Files:**
- Create: `apps/desktop/src/terminal-links.ts`
- Modify: `apps/desktop/src/main.ts:382-384` (register the link provider; OSC 8 is parsed into link ranges instead of being swallowed), `apps/desktop/src/files-panel.ts:18` (a "Browser" mode button shown for `.html`/`.htm`, calling `openBrowserTab({ file })`), the desktop handler of `dsh web` (locate with `grep -rn "web" crates/cli/src`; route its target to `openBrowserTab`)
- Test: `apps/desktop/tests/terminal-links.test.ts`

**Interfaces:**
- Consumes: `openBrowserTab` (Task 6).
- Produces: `export function findLinks(line: string): { start: number; end: number; target: string }[]`, `export function classifyTarget(target: string, cwd: string): { kind: 'web'; url: string } | { kind: 'file'; path: string } | { kind: 'refused'; reason: string }`.

- [ ] **Step 1: Write the failing tests:** `finds_http_and_https_and_trims_trailing_punctuation` (`"see https://a.b/c)."` → target `https://a.b/c`), `keeps_balanced_parentheses` (`https://en.wikipedia.org/wiki/X_(y)`), `refuses_mailto_and_custom_schemes` (`mailto:x@y`, `vscode://x` → `refused`), `html_path_becomes_file` (`docs\\a.html` with cwd `D:\\p` → `{ kind: 'file', path: 'D:\\p\\docs\\a.html' }`), `osc8_hover_shows_the_real_target` (OSC 8 with URI `https://evil.example` over text `https://good.example` → the hover text is `https://evil.example`).
- [ ] **Step 2:** Run `npm test`; expected FAIL.
- [ ] **Step 3:** Implement with `terminal.registerLinkProvider`; activation only with `event.ctrlKey`; the file case checks existence through the existing files command before opening.
- [ ] **Step 4:** Run `npm test`; expected PASS. In the real app: Ctrl+click a URL in a PowerShell tab, the Files panel button on an `.html`, a `mailto:` (refused on hover).
- [ ] **Step 5: Commit** `feat(desktop): http(s) links and HTML documents open in a browser tab`.

### Task 8: Local HTML confined to its folder

**Files:**
- Modify: `crates/browser/src/pages.rs` (per-page `local_root: Option<PathBuf>`), `apps/desktop/src-tauri/src/browser_host.rs` (`Fetch.enable` with pattern `file://*` per page with a root)
- Test: inline tests in `pages.rs`; fixture `crates/browser/tests/fixtures/local/{index.html,style.css}` and `crates/browser/tests/fixtures/outside.txt`

**Interfaces:**
- Produces: `pub fn local_allowed(root: &Path, requested: &Url) -> bool` (canonicalised, symlinks resolved, `..` rejected, case-insensitive on Windows).

- [ ] **Step 1:** If Task 1 recorded that Moli lacks the `Fetch` domain, stop: record local HTML as deferred in the M1 note and in FRD-0011 v2 (Task 10), leave the Files panel button hidden, and skip to Task 9.
- [ ] **Step 2: Write the failing tests:** `sibling_css_is_allowed`, `parent_file_is_refused`, `symlink_out_of_root_is_refused`, `other_drive_is_refused`.
- [ ] **Step 3:** Run `cargo test -p shinatama-browser local`; expected FAIL.
- [ ] **Step 4:** Implement; `Fetch.requestPaused` → `Fetch.continueRequest` when `local_allowed`, else `Fetch.failRequest` with `AccessDenied`.
- [ ] **Step 5:** Run; expected PASS. In the app, the fixture page shows its CSS and cannot load `outside.txt`.
- [ ] **Step 6: Commit** `feat(browser): a local HTML document reads only its own folder`.

### Task 9: Agent tools over a broker pipe

**Files:**
- Create: `crates/browser/src/broker.rs`, `crates/browser/src/tree.rs`
- Modify: `crates/cli/src/mcp.rs:19-40` (six tools), `apps/desktop/src-tauri/src/browser_host.rs` (broker server per window, `Origin::Agent` pages opened as tabs of the agent's thread)
- Test: inline tests in `broker.rs`, `tree.rs`, and `crates/cli/src/mcp.rs` tests

**Interfaces:**
- Consumes: `PageSet`, `Cdp`, `shinatama_messaging` instance authentication as in `mcp.rs:61` (`directory.authenticate(&instance, &token)`).
- Produces: broker pipe `\\.\pipe\shinatama-broker-<window random>` advertised to instances through the existing per-instance environment as `SHINATAMA_BROWSER_PIPE`; line-delimited JSON requests `{ "instance", "token", "op", "args" }` → `{ "outcome": "ok"|"needs_user"|"denied"|"failed", "result" }`. MCP tools: `shinatama_browser_open(url)`, `shinatama_browser_read(page_id, format: "markdown"|"text"|"tree")`, `shinatama_browser_click(page_id, ref)`, `shinatama_browser_type(page_id, ref, text, submit)`, `shinatama_browser_screenshot(page_id)`, `shinatama_browser_close(page_id)`. `tree.rs`: `pub fn snapshot(dom: &serde_json::Value) -> (String, HashMap<String, i64>)` mapping refs `e1, e2, …` to backend node ids.
- Routing: every op passes through `shinatama_browser::route` (existing F17) with `Caller` from the authenticated instance; out of scope → `denied`; page under user control → `needs_user`.

- [ ] **Step 1: Write the failing tests:** `wrong_token_is_denied`, `agent_cannot_act_on_a_user_page` (`needs_user`), `agent_cannot_reach_out_of_scope_url` (`denied`, nothing sent to CDP), `read_markdown_wraps_untrusted` (result wrapped like peer messages, FRD-0011 R26), `mcp_lists_ten_tools` (four existing + six), `tree_refs_are_stable_within_a_snapshot`.
- [ ] **Step 2:** Run `cargo test -p shinatama-browser broker tree` and `cargo test -p shinatama-cli`; expected FAIL.
- [ ] **Step 3:** Implement. Markdown via Moli's `moli-html2md` crate if exposed through CDP, else `DOM.getOuterHTML` converted in the broker with the same crate added to `crates/browser`.
- [ ] **Step 4:** Run; expected PASS. In the app: a Claude Code session opens `https://example.com` with `shinatama_browser_open`; the tab appears, the user takes control, the agent's next click returns `needs_user`.
- [ ] **Step 5: Commit** `feat(browser): agents drive browser tabs through fixed MCP tools`.

### Task 10: Measurements, records, packaging and install (gates M2, M3, M6)

**Files:**
- Create: `docs/verification/2026-11-14-M2-M3-moli-memory-latency.md`
- Modify: `docs/frd/FRD-0011-browser.md` (v2: R18 met by the pipe for this path, R43 answered, R46 superseded, new requirements for tabs, links, tiling and agent tools, citing ADR-0056), `docs/functional-design.md:914-916`, `docs/technical-design.md` 7.9, `docs/adr/ADR-0056-moli-browser-pane.md` (status Accepted with SHA) and `docs/adr/README.md`, `scripts/package-local.ps1:247,298,320` (add `shinatama-moli.exe` with its pinned SHA-256), `sidecars/moli-host/moli-pin.json` (`exe_sha256`), `THIRD_PARTY_NOTICES.md` (Moli, V8, libcurl, Moli's `licenses/`)

- [ ] **Step 1:** M2: measure the whole host tree (working set and private bytes, `Get-Process` summed over the job) idle, one page, four pages on the fixtures `example.com`, `en.wikipedia.org/wiki/Rust_(programming_language)`, `github.com/lexmount/moli`, a local HTML fixture; same pages in headless Chrome. Record hardware, OS build, Moli commit, cold and warm runs.
- [ ] **Step 2:** M3: time from `browser_input` (click) to the first frame on the canvas, 20 samples per page; record median and p95.
- [ ] **Step 3:** M6: hostile fixtures of FRD-0011 R48 (page that tries `window.open` to Chrome, redirects out of scope for an agent page, prints a fake permission prompt): record outcomes.
- [ ] **Step 4:** Update the records listed above; normalise line endings before exact-string edits (memory "Shinatama docs line endings").
- [ ] **Step 5:** `npm run desktop:build`, build `sidecars/moli-host` in release, run `scripts/package-local.ps1`, copy the exes over `D:\Software\Shinatama` (no backup), start `Shinatama.exe`, open a link from a terminal.
- [ ] **Step 6:** `npm run verify`; expected PASS.
- [ ] **Step 7: Commit** `docs(browser): FRD-0011 v2, measurements and packaging for Moli browser tabs`.
