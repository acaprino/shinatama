# FRD-0009: BillionContext sidecar

| | |
|---|---|
| Version | 1 |
| Status | Accepted 2026-11-01 (this Markdown text) |
| Date | 2026-11-01; R24 amended 2026-11-02 at the user's request; R31 added 2026-11-17 at the user's request |
| Sources | functional-design goal 6, 2 (anti-goal), 3, 5.6, 5.9; technical-design 9; specs/token-savings 7.2 (baseline); records of 2026-10-31 and 2026-11-01 on proxy stalls; [2026-11-02 storage fix](../notes/2026-11-02-sidecar-storage-latency.md) |
| Governing ADRs | ADR-0029, ADR-0030 |
| Register sections | SC1 to SC16, Q5, Q7, HM12, DS12 |
| Related records | FRD-0005 (seed through the proxy), FRD-0007 (participants through the proxy), FRD-0010 (title calls), FRD-0013 (adapters inside the request path), FRD-0016 (TokenEconomizer beside it) |

## 1. Purpose and scope

Every launch keeps the full resilience, security and compression of the BillionContext
sidecar. This record states what "the same sidecar" means (one source, unmodified), the
isolation properties that are acceptance, the supervision rules learned from the failures of
2026-10-31 and 2026-11-01, and the baseline configuration Shinatama sets.

## 2. Requirements

### 2.1 One source, unmodified

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Every agent launch goes through the BillionContext sidecar with its full resilience, security and compression. In v1 it covers Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness, with Gemini launch-only; Hermes Agent and DeepSeek Harness count as covered only once the proxy carries their provider traffic, and a provider whose traffic does not pass is marked uncovered. | U (goal 6); S (coverage rule) | X | SC7, SC11, HM12, DS12 (SEC) | 0029 |
| R02 | Shinatama runs the same BillionContext code from the single source (`sidecar/source-build`, Musashi Terminal's), supervised by Shinatama: not forked, not the precompiled npm package, no auto-update; the owned source upgrades only through Shinatama. | U | X | SC1 | 0029 |
| R03 | "Unmodified" means no fork or patch inside Shinatama: anything Shinatama needs on the request path (the token-savings adapters for Codex and Hermes Agent, the bypass marker of R16, the watchdog fix) goes into the single source and Shinatama inherits it. How the source is brought in (copy with provenance, submodule, published package) is the builder's call, but it stays one source. | U 2026-11-01 | X | — | 0029 |
| R04 | No measured saving percentage is claimed for compression. | S | X | — | 0029, 0010 |
| R05 | How Node is provided for the sidecar on macOS and Linux is the builder's call within the install story (FRD-0017). | S | X | SC13 | 0029 |

### 2.2 Isolation (each item is acceptance, verified under Shinatama)

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R06 | A private root under which the sidecar's configuration, data, cache and state paths resolve, prepared on Windows, macOS and Linux. | U | X | SC1 | 0029 |
| R07 | The sidecar's environment is applied to the launched child only, never to the parent, siblings or the user's shell. | U | X | SC2 (SEC) | 0029 |
| R08 | The proxy's certificate authority is trusted only by the launched process, per agent runtime (native binary, Rust, Bun, Node, Python) and per OS; the system trust store is never changed. | U | X | SC3 (SEC), HM12, DS12 | 0029 |
| R09 | The launch manifest holds argv and launch overrides only, never credentials. | U | X | SC4 (SEC) | 0029, 0031 |
| R10 | The compression policy is as shipped (adaptive pressure, CCR, Absorb, profiles); `--no-context` is an explicit per-session opt-out. | U | X | SC7 | 0029 |
| R11 | `CODEX_HOME` handling agrees with the sidecar's private overlay: native chats, history and auth stay where Codex expects. | S | X | CX12 | 0029 |

### 2.3 Traffic that passes through

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R12 | Each instance's provider traffic goes through the proxy; the sidecar's session id is separate from `SHINATAMA_INSTANCE`, and the bili MCP shim and `shinatama mcp` coexist as two MCP servers in one launch. The shim must follow `/resume` session ids under Shinatama. | S | X | SC8 | 0029 |
| R13 | Title calls to a provider and the roundtable's bare-model calls go through the same proxy, through its existing auxiliary-call path (the prompt rewriter is the precedent), so cost and security properties hold and Shinatama handles no credentials. | S | V1 | SC10 | 0029, 0028 |
| R14 | A hop's seed and catch-up are ordinary user turns for the proxy and count against the destination's window; the seed budget stays well below the adaptive threshold (`maxTokens: 160000` in the shipped policy) so a hop never triggers compression on the destination's first turn. | S | S2 | SC9 | 0029 |
| R15 | The seed, the bootstrap and the receipt reach the agent exactly as sent: the prompt rewriter (`promptRewrite.mode: auto`) does not alter them and CCR does not externalize them. | A | S2 | SC9 (SEC), Q5 | 0029 |
| R16 | Where the rewriter or CCR would alter or externalize them, a bypass marker is added in the sidecar's single source and inherited, never patched inside Shinatama. | U 2026-11-01 (place) | S2 | SC9 | 0029 |
| R17 | Whether the proxy is one per instance or one shared by a window's instances is undecided and decided by SC5 with its memory, recovery and isolation measurements, together with the launch path (the shipped per-session launcher with its supervision off, or a direct attach). | S (open Q7) | X | SC5 | 0030 |

### 2.4 Supervision: one supervisor, busy is not dead

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R18 | The Shinatama process that owns a proxy is its only supervisor: it alone starts and restarts it; a recovery starts exactly one replacement on the original address and every waiting session attaches to it. Shinatama relies neither on per-session supervision nor on the launcher's message discipline. | U, confirmed 2026-11-01 | X | SC5, SC6 | 0030 |
| R19 | Liveness is two facts: the proxy's process is alive, and its port is held, decided by binding the port (`EADDRINUSE` means held), never by connecting, because on Windows a proxy whose event loop is held refuses new connections once its accept queue is full (232 waiting) while the bind still fails. | U 2026-11-01 | X | SC14 | 0030 |
| R20 | A proxy that misses health checks while both facts hold is **busy**, not dead: the window shows it, nothing is started, and it is probed at a growing interval of at most 8 s so the supervisor's probes do not take the accept-queue slots the agents need. | U 2026-11-01 | X | SC14 | 0030 |
| R21 | Recovery starts only when the process has exited or the port is free; while an agent keeps running the recovery lands on the original address, and an instance's exit drains in order with no lost final flush. | U | X | SC6 | 0030 |
| R22 | A session's process never probes, restarts or reports on the proxy and never starts a replacement; when the shipped per-session launcher is used, its supervision is switched off and its output kept off the agent's terminal. | U 2026-11-01 | X | SC5, SC12 | 0030 |
| R23 | The proxy's own owner watchdog never takes a stall of its own event loop for the loss of every owner: a late tick's delay is credited back to every recorded heartbeat and to the idle grace, while a recycled owner pid that goes silent with the watchdog on time still ages out. Shinatama inherits this fix (Musashi Terminal commit `ac7d57a`) through the single source. | U 2026-11-01 | X | SC12 | 0030, 0029 |
| R24 | Shinatama inherits the 2026-11-02 storage correction through the single sidecar source: six content-reference indexes (schema v9) and transactional CCR synchronization by net reference-count changes. SC16 must prove data preservation and bounded local transport latency during persistence, including on upgrades; increasing timeouts, tolerating a busy state or bypassing the proxy does not satisfy this requirement. Other event-loop holds and the proxy topology remain subject to SC16 and SC5. | U 2026-11-02; S (source regression) | X | SC16 | 0029, 0030 |

### 2.5 Nothing on the agent's terminal

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R25 | No sidecar diagnostic is ever written to an agent's terminal: diagnostics go to Shinatama's log and the proxy status (busy, recovering) shows in the window. | U 2026-11-01 | X | SC15 | 0030 |
| R26 | The rule covers every line a launcher or proxy can write (owner renewal, attach refusals, stale-proxy rotation included): the bytes on an agent's PTY come only from the agent. It is verified by capturing the terminal from launch to exit while faults are injected one at a time (a held event loop, a timed-out and a refused owner renewal, a killed proxy, a stale proxy on the port, an unarmed daemon proxy), each fault appearing once in the log and never in the capture, on every launch path under SC5 and again on every sidecar upgrade; never by auditing messages one path at a time. | U 2026-11-01 | X | SC15 | 0030 |
| R31 | The proxy writes nothing into an agent's conversation either: no text of its own in the model's reply (the `acp_status` report, the `📦 [ACP] Compressed …`, decompress, search and absorb confirmations, the `❌ [ACP] … FAILED` lines), so no harness shows or stores it. Shinatama writes `compress.visibilityMarkers: false` into the baseline, and a running configuration without it shows as a difference (R29). | U 2026-11-17 | X | — | 0030 |

### 2.6 Baseline configuration Shinatama sets

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R27 | CCR is visibly on (`compress.ccr.enabled = true`; `minToolTokens` 4000 in the live configuration); input compression on (`absorb`, `tiers`, `adaptivePressure`, `promptRewrite.mode = "auto"`); output steering on with the verbosity lever (`compress.outputSteering.enabled = true`, `verbosityLevel` default 2). | U 2026-11-01 | X | T6 | 0038 |
| R28 | The effort lever is locked off on every harness, provider and model: Shinatama writes `effortRouting: false` into the `compress.outputSteering` block it manages and offers no switch. The lock lifts only by a later decision backed by a passed T5 row for that provider and model and a measured saving that beats the cache-miss risk. | U 2026-11-01 | X | T5, T7 | 0038 |
| R29 | Shinatama shows one status line per instance (CCR on or off, input on or off, output level) read from the effective configuration the running sidecar resolved, not the intended one; a baseline item that is off shows as a warning, never as a silent state. | A | V1 | T9 | 0038 |
| R30 | Shinatama's own updates never touch the sidecar's no-auto-update rule (FRD-0017). | S | X | X6 | 0029 |

## 3. Open questions

1. Proxy topology, one per instance or shared (register Q7, SC5; worklist #53).
2. Bounds for event-loop holds from work other than the reproduced SQLite/CCR path (SC16).
   The persistence regression already has a local CONNECT-plus-echo limit of 750 ms.
   This is a controlled fixture limit, not a latency promise for remote services.

## 4. Acceptance

SC1 to SC16 pass under Shinatama; HM12 and DS12 for the two new agents. Observed so far
(2026-10-31, 2026-11-01): 170 duplicate replacements, four self-shutdowns with every owner
alive, and `ECONNREFUSED` by two further paths, all with BillionContext 0.1.207's shipped
launcher; the sidecar has not been qualified under Shinatama. The storage fix was
verified and activated in Musashi Terminal on 2026-11-02; its results are evidence for
inheritance, not a passed Shinatama SC16. See the [amendment and acceptance
procedure](../notes/2026-11-02-sidecar-storage-latency.md).
