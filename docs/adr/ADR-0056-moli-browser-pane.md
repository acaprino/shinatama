---
adr_id: ADR-0056
version: 1
title: An embedded browser pane drawn by Shinatama over a per-window Moli host reachable only through a private, token-checked named pipe
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - crates/browser
    - sidecars/moli-host
    - crates/cli/src/mcp.rs
    - apps/desktop/src-tauri/src
    - apps/desktop/src
    - docs/frd/FRD-0011-browser.md
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - browser
    - browser-security
    - process-lifecycle
    - workspace-layout
governs_symbols:
depends_on:
  - ADR-0035
  - ADR-0036
  - ADR-0039
  - ADR-0055
supersedes:
created_from_task: user-decision-2026-11-14-embedded-low-memory-browser
proposed_on: 2026-11-14
proposed_at_sha: 9795ed14de900be18e9ca4758c89b91c94a379d8
accepted_on:
accepted_at_sha:
---

# ADR-0056: An embedded browser pane drawn by Shinatama over a per-window Moli host reachable only through a private, token-checked named pipe

## Context

On 2026-11-14 the user asked for an embedded, very low-memory browser for agents and navigation,
and after an online survey chose Moli. Until now interactive browsing was external Chrome only
(functional-design 5.15: "tiling does not implicitly approve an embedded browser engine") and
`moli serve` was gated off because it binds a TCP listener with no credential (ADR-0036; at
today's `main` Moli checks Host and Origin, still no credential).

Moli has no GUI by design; with on-demand layout it renders software frames and accepts CDP
input. Its own figures (73-92 MiB against 697-773 MiB for headless Chrome) are vendor
measurements, not acceptance evidence.

## Decision

1. **Embedded pane.** Shinatama has a `browser` pane kind in a thread's layout. Shinatama draws
   it from Moli's screencast and screenshot frames and forwards input over CDP. Page content never
   runs in Shinatama's WebView2.
2. **Own host, no port.** `shinatama-moli.exe`, built in `sidecars/moli-host/` from Moli crates
   pinned by commit, serves Moli's protocol router over a named pipe with a random name, a
   current-user-only DACL, remote clients rejected, single first instance, and a 256-bit per-launch
   token checked before any byte reaches Moli. Name and token travel on stdin. `moli serve` is
   never started. This satisfies the B4 gate of ADR-0036 for this path; it does not open `moli
   serve` for any other use.
3. **One host per window** in a kill-on-close Job Object, started on the first page, ended with
   the window or after an idle timeout, holding every page of that window.
4. **Agents** use fixed `shinatama_browser_*` MCP tools (open, read, click, type, screenshot,
   close) through a per-window broker that authenticates the instance token; no raw CDP, no
   eval. One controller per page; the user takes control in place and hands it back explicitly.
5. **Links open inside.** Every http(s) link and HTML document the user opens from Shinatama
   (Ctrl+click in a terminal, the Files panel, `dsh web`) opens in a browser tab of its own,
   user-controlled; a local HTML document grants only its own folder, for that tab
   (user, 2026-11-14). Each tab can hand its URL to the system browser.
6. **Unchanged from ADR-0035/0036:** Moli default; Patchright visible Chrome only on explicit
   request and opt-in; no automatic fallback or transfer between engines; temporary profiles by
   default; untrusted content; evidence before performance claims.

## Consequences

- functional-design 5.15 line on external-only interactive browsing, FRD-0011 R18, R43 and R46 and
  technical-design 7.9 are revised by FRD-0011 v2.
- Shinatama carries a small patch on Moli (`ProtocolServer::serve_on`) until upstream accepts it,
  and requalifies on each Moli pin bump.
- The package grows by `shinatama-moli.exe` (size measured at gate M1).
- Sites that need full Chrome fidelity still go to the explicit Patchright path.

## Rejected alternatives

- **RA-001** WebView2 child webview: full compatibility but the user chose Moli for memory.
- **RA-002** `moli serve` on loopback: any local process could drive the browser.
- **RA-003** Moli linked into `Shinatama.exe`: a page crash would take down the app.
- **RA-004** Lightpanda: no native Windows build. Servo: embedding API pre-1.0, no memory data.

## Gates

M1 build, M2 whole-tree memory, M3 input latency, M4 pipe security, M5 cleanup, M6 hostile pages, M7 links
(design document `docs/superpowers/specs/2026-11-14-moli-browser-pane-design.md`, section 6).
