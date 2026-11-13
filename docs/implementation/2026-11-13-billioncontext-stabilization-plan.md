# BillionContext: stato veritiero e arresto sicuro — Piano di implementazione (fasi 0–2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Shinatama mostra per ogni sessione ciò che il sidecar BillionContext fa davvero (non ciò che è configurato), non può fermare il proxy sotto agli agenti per errore, e possiede le prove P0 di integrità (bypass dei messaggi di controllo, isolamento tra sessioni, SC16).

**Architecture:** Il sidecar resta l'unico proprietario della trasformazione delle richieste. Shinatama aggiunge un client admin in sola lettura verso gli endpoint `/__bili/*` già presenti, un arresto in due tempi (rifiuto o drain) nel supervisor esistente, e una vista di stato a tre livelli `configured / applied / verified`. L'unica modifica al motore è un endpoint di lettura delle capacità per sessione, scritta nella sorgente del sidecar che sta in questo repository (`sidecar/source-build/prepared`) e ricostruita lì.

**Tech Stack:** Rust (crates `engine`, `token-economy`, `apps/desktop/src-tauri`), TypeScript senza framework (`apps/desktop/src`), Node 24.14.1 per le fixture, BillionContext 0.1.207 (sorgente in `sidecar/source-build/prepared`).

**Spec:** [studio](../notes/2026-11-13-billioncontext-intent-study.md) §§3, 10, 11, 13, 14 (fasi 0–2); [audit compressione](../notes/2026-11-13-billioncontext-compression-audit.md) §§3, 8, 9; [audit integrazione](../notes/2026-11-13-billioncontext-integration-audit.md) §§1–3, 6, 7.

## Perimetro

Lo studio copre otto fasi su sottosistemi indipendenti. Questo piano realizza le **fasi 0, 1 e 2**, che producono software verificabile da sole e sono il prerequisito di tutto il resto. Le fasi 3–7 sono elencate in fondo come piani successivi, ciascuno con il suo gate d'ingresso; non vanno iniziate prima che i gate di questo piano siano chiusi.

Il piano ha due parti:

- **Parte A (Task 1–8):** solo repository Shinatama. Nessuna modifica al motore.
- **Parte B (Task 9–12):** una modifica alla sorgente del sidecar in `sidecar/source-build/prepared`, ricostruzione, e ciò che ne dipende.

## Global Constraints

- Tutto il lavoro avviene in questo repository. Il motore si modifica solo nella sua sorgente (`sidecar/source-build/prepared/src`, `sidecar/source-build/prepared/kernel`), mai nel compilato `sidecar/source-build/prepared/dist`: dopo ogni modifica si ricostruisce con `node scripts/build.mjs` da quella cartella e si aggiornano le impronte in `sidecar/source-build/provenance.lock.json` nello stesso commit.
- Nessun secondo proxy e nessuna seconda compaction. `crates/token-economy/src/decision.rs` resta non cablato.
- La baseline di `crates/engine/src/sidecar_config.rs:162-176` non cambia: `effortRouting: false`, `verbosityLevel: 2`, `promptRewrite.mode: "auto"`, soglie CCR/Absorb `4000`, `adaptivePressure.maxTokens: 160000`.
- Un file di configurazione esistente non viene mai sovrascritto (`create_new`).
- Il supervisor non riesegue mai una richiesta del modello e non avvia mai un processo sostitutivo concorrente.
- Solo Windows. I test nativi restano `#[ignore]` e si eseguono con `cargo test -p shinatama-engine --test <nome> -- --ignored --test-threads=1`.
- UI: niente popup. Pannelli laterali e conferme in linea; modale solo per decisioni sull'intero workspace.
- L'app si avvia solo da `Shinatama.exe`. La verifica UI segue la procedura esistente: build `dist` + exe con `custom-protocol`, guida via CDP, `SHINATAMA_HOME` fuori da git.
- Nessuna chiamata a provider a pagamento: tutte le prove usano un'origine sintetica locale.
- Nessuna percentuale di risparmio compare nella UI o nella documentazione.
- La telemetria non registra prompt, credenziali né URL con segreti.
- Commit in inglese, stile del repository (`feat(sidecar): …`, `fix(sidecar): …`, `test(sidecar): …`, `docs: …`).

## Review Focus

1. **Stop premuto mentre uno stream SSE è a metà risposta.** Ci si aspetta che la risposta in corso finisca o che lo stato dica esplicitamente «interrotta»; mai un riavvio che ripete la richiesta. → Task 3, test `drain_waits_for_in_flight_then_stops`.
2. **Il sidecar non risponde a `/__bili/status` (event loop bloccato) mentre si chiede lo stop.** Ci si aspetta `in_flight: None` trattato come «sconosciuto», non come zero: il drain attende fino al timeout, non taglia subito. → Task 3, test `unknown_in_flight_is_not_idle`.
3. **Configurazione preesistente diversa dalla baseline.** Ci si aspetta che la UI dica «configurazione personalizzata» e mostri i valori letti dal processo, non quelli della baseline. → Task 2, test `drift_is_reported_when_resolved_differs_from_baseline`.
4. **Sessione Codex con `ccr.enabled=true`.** Ci si aspetta `configured: true, applied: false, reason: codex_host`, non una spunta verde. → Task 9, test `codex_host_reports_ccr_disarmed_with_reason`; Task 10, test `capabilities_match_what_the_origin_received`.
5. **Sessione mai vista dal sidecar (nessuna richiesta ancora inviata).** Ci si aspetta «non ancora osservata», non valori di default presentati come effettivi. → Task 9, test `unknown_session_is_404_not_defaults`; Task 11, test `unobserved_session_renders_unknown`.

---

## File Structure

| File | Responsabilità | Azione |
|---|---|---|
| `crates/engine/src/sidecar_admin.rs` | Client loopback in sola lettura verso `/__bili/*`, con verifica d'identità sulla stessa connessione | Create |
| `crates/engine/src/sidecar_supervisor.rs` | Stato `Draining`, traffico reale in `Snapshot`, comando `Drain` | Modify |
| `crates/engine/src/sidecar_config.rs` | Espone i valori di baseline come struttura confrontabile | Modify |
| `crates/engine/src/lib.rs` | `pub mod sidecar_admin;` | Modify |
| `apps/desktop/src-tauri/src/sidecar.rs` | `Status` esteso, `stop(force)`, lettura configurazione risolta | Modify |
| `apps/desktop/src-tauri/src/agents.rs` | `Controller::live_agent_sessions()` | Modify |
| `apps/desktop/src-tauri/src/main.rs` | Comandi `sidecar_stop(force)`, `sidecar_capabilities(session)` | Modify |
| `apps/desktop/src/sidecar-status.ts` | Funzioni pure: etichette, decisione sui pulsanti, riga di capacità | Create |
| `apps/desktop/src/sidecar-control.ts` | Pannello laterale «Model proxy», conferma di stop in linea | Rewrite |
| `apps/desktop/src/session-detail.ts` | Riga di capacità per sessione | Modify |
| `apps/desktop/tests/sidecar-status.test.ts` | Test delle funzioni pure | Create |
| `crates/engine/tests/sidecar_integrity.rs` | Prove native SC9, isolamento, drain | Create |
| `crates/engine/tests/sidecar_soak.rs` | Prova nativa SC16 | Create |
| `crates/engine/tests/fixtures/origin.mjs` | Origine sintetica: registra i byte ricevuti, risponde in SSE | Create |
| `sidecar/source-build/prepared/src/server/capabilities.ts` | Endpoint `GET /__bili/capabilities` | Create (Parte B) |

---

# Parte A — Repository Shinatama

### Task 1: Client admin del sidecar

**Files:**
- Create: `crates/engine/src/sidecar_admin.rs`
- Modify: `crates/engine/src/sidecar_supervisor.rs:792-920` (rendere `connect`, `request`, `remaining` `pub(crate)`)
- Modify: `crates/engine/src/lib.rs`

**Interfaces:**
- Consumes: `sidecar_supervisor::request(stream: &mut TcpStream, port: NonZeroU16, method: &str, path: &str, keep_alive: bool, until: Instant) -> Option<(u16, Vec<u8>)>` (firma esistente, solo visibilità).
- Produces:

```rust
pub struct AdminTarget { pub port: NonZeroU16, pub pid: u32, pub timeout: Duration }

/// GET /__bili/health then `path` on ONE connection; None unless health is 200,
/// ok==true and pid matches. Body capped at `limit` bytes.
pub fn get_json(target: &AdminTarget, path: &str, limit: usize) -> Option<serde_json::Value>;

/// `inFlight` from /__bili/status. None = unknown, never zero.
pub fn in_flight(target: &AdminTarget) -> Option<u64>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedBaseline {
    pub prompt_rewrite_mode: Option<String>,
    pub tiers: Option<bool>,
    pub absorb_enabled: Option<bool>,
    pub absorb_min_tool_tokens: Option<u64>,
    pub ccr_enabled: Option<bool>,
    pub ccr_min_tool_tokens: Option<u64>,
    pub adaptive_enabled: Option<bool>,
    pub adaptive_max_tokens: Option<u64>,
    pub steering_enabled: Option<bool>,
    pub verbosity_level: Option<u64>,
    pub effort_routing: Option<bool>,
}
/// Parsed from the `compress` and `promptRewrite` objects of GET /__bili/config.
pub fn resolved_baseline(target: &AdminTarget) -> Option<ResolvedBaseline>;
pub fn baseline_from_config_value(config: &serde_json::Value) -> ResolvedBaseline;
```

Limiti fissi: `get_json` usa `limit = 256 * 1024`; una risposta più grande dà `None`.

- [ ] **Step 1: Test che fallisce.** In `sidecar_admin.rs`, modulo `tests`, riusando l'helper `server(...)` di `sidecar_supervisor.rs:1030` (spostarlo in `pub(crate) mod test_support` dello stesso file):

```rust
#[test]
fn in_flight_is_read_only_after_identity_matches() {
    // server answers /__bili/health with pid 4242 and /__bili/status with {"inFlight":3}
    assert_eq!(in_flight(&target(port, 4242)), Some(3));
    assert_eq!(in_flight(&target(port, 9999)), None); // other identity
}
#[test]
fn missing_or_non_numeric_in_flight_is_unknown() {
    // /__bili/status -> {"inFlight":"3"} then {}
    assert_eq!(in_flight(&target(port, 4242)), None);
}
#[test]
fn oversized_body_is_refused() {
    // /__bili/config -> 300 KiB of JSON
    assert_eq!(get_json(&target(port, 4242), "/__bili/config", 256 * 1024), None);
}
#[test]
fn baseline_parser_reads_the_written_baseline_exactly() {
    let value: serde_json::Value = serde_json::from_slice(
        &crate::sidecar_config::baseline_bytes_for_test()).unwrap();
    let parsed = baseline_from_config_value(&value);
    assert_eq!(parsed.ccr_min_tool_tokens, Some(4000));
    assert_eq!(parsed.adaptive_max_tokens, Some(160000));
    assert_eq!(parsed.verbosity_level, Some(2));
    assert_eq!(parsed.effort_routing, Some(false));
    assert_eq!(parsed.prompt_rewrite_mode.as_deref(), Some("auto"));
}
```

- [ ] **Step 2:** `cargo test -p shinatama-engine sidecar_admin` → FAIL (modulo inesistente).
- [ ] **Step 3: Implementare** le quattro funzioni. `GET /__bili/config` può restituire la configurazione dentro un involucro: `baseline_from_config_value` cerca `compress` prima alla radice, poi sotto `config`. Aggiungere in `sidecar_config.rs` `#[cfg(test)] pub(crate) fn baseline_bytes_for_test() -> Vec<u8>` che chiama `baseline` con la radice sintetica già usata dal test esistente.
- [ ] **Step 4:** `cargo test -p shinatama-engine sidecar_admin` → 4 passed. `cargo test -p shinatama-engine sidecar_supervisor` → i test esistenti restano verdi.
- [ ] **Step 5: Commit** `feat(sidecar): read-only admin client with same-connection identity check`

### Task 2: Configurazione risolta e deriva dalla baseline (fase 0)

**Files:**
- Modify: `crates/engine/src/sidecar_config.rs` (dopo `baseline`, riga 184)
- Modify: `apps/desktop/src-tauri/src/sidecar.rs:31-39,63-121`

**Interfaces:**
- Consumes: `sidecar_admin::{AdminTarget, ResolvedBaseline, resolved_baseline, baseline_from_config_value}`.
- Produces:

```rust
// sidecar_config.rs
pub fn intended_baseline() -> ResolvedBaseline; // the values `baseline()` writes

// sidecar.rs — Status gains (camelCase on the wire):
pub resolved: Option<ResolvedBaseline>,   // read from the running process
pub config_drift: Option<bool>,           // None until read; true when resolved != intended
```

Il controller legge la configurazione risolta **una volta** per ogni transizione a `Ready` e la conserva in `Mutex<Option<ResolvedBaseline>>`; `status()` non fa I/O di rete. La lettura avviene nel worker di preparazione esistente (`tauri::async_runtime::spawn_blocking`), mai nel thread della UI. Il pid viene da `Snapshot`; aggiungere a `Snapshot` il campo `pub pid: Option<u32>` valorizzato in `launch`.

- [ ] **Step 1: Test che fallisce** in `sidecar_config.rs`:

```rust
#[test]
fn intended_baseline_matches_the_bytes_written() {
    let value: serde_json::Value = serde_json::from_slice(&baseline_bytes_for_test()).unwrap();
    assert_eq!(crate::sidecar_admin::baseline_from_config_value(&value), intended_baseline());
}
```

e in `apps/desktop/src-tauri/src/sidecar.rs`, nuovo modulo `tests`:

```rust
#[test]
fn drift_is_reported_when_resolved_differs_from_baseline() {
    let mut resolved = intended_baseline();
    assert_eq!(drift(Some(&resolved)), Some(false));
    resolved.verbosity_level = Some(3);
    assert_eq!(drift(Some(&resolved)), Some(true));
    assert_eq!(drift(None), None);
}
```

- [ ] **Step 2:** `cargo test -p shinatama-engine sidecar_config && cargo test -p shinatama-desktop sidecar::tests` → FAIL.
- [ ] **Step 3: Implementare** `intended_baseline()`, `fn drift(resolved: Option<&ResolvedBaseline>) -> Option<bool>`, il campo `pid` e la lettura a `Ready`. Se il nome del pacchetto desktop non è `shinatama-desktop`, leggerlo da `apps/desktop/src-tauri/Cargo.toml` e usarlo in tutti i comandi di questo piano.
- [ ] **Step 4:** stessi comandi → PASS.
- [ ] **Step 5: Commit** `feat(sidecar): report resolved configuration and drift from the baseline`

### Task 3: Traffico reale e drain nel supervisor (fase 1)

**Files:**
- Modify: `crates/engine/src/sidecar_supervisor.rs:46-135,288-341,407-565,566-625`

**Interfaces:**
- Consumes: `sidecar_admin::{AdminTarget, in_flight}`.
- Produces:

```rust
pub enum Phase { /* existing */ Draining }              // serializes as "draining"
pub enum Reason { /* existing */ DrainTimedOut }
pub enum Command { Start, Stop, Drain, Retry, Activity(Activity) }
pub struct Snapshot { /* existing */ pub pid: Option<u32>, pub in_flight: Option<u64>,
                      pub drain_deadline_unix_ms: Option<i64> }
pub struct SupervisorConfig { /* existing */ pub drain_timeout: Duration }
```

Regole, tutte nel worker proprietario:

- A ogni health riuscito il worker legge `in_flight`. `Some(0)` → `Activity::Idle`; `Some(n>0)` → `Activity::RequestInFlight`; `None` → attività invariata. `Activity::SessionSave` non viene più impostata da nessuno: lasciarla nell'enum.
- `Command::Drain` in `Ready`/`Busy` → `Phase::Draining`, `drain_deadline = now + drain_timeout`. In qualunque altra fase equivale a `Stop`.
- In `Draining` il worker interroga `in_flight` a ogni `poll_period`. Due letture consecutive `Some(0)` → `stop(true)`. Scadenza → `stop(true)` con `Reason::DrainTimedOut`. `None` non conta come zero.
- `Command::Start` in `Draining` annulla il drain e torna a `Ready`.
- `Command::Stop` resta immediato: lo usa la chiusura dell'applicazione (`stop_and_wait`).
- `SupervisorConfig::valid` richiede `drain_timeout` tra 1 s e 120 s. Il desktop imposta `30 s` in `sidecar.rs` dove costruisce `SupervisorConfig` (vicino a riga 320).

- [ ] **Step 1: Test che falliscono**, nel modulo `tests` esistente, con il server fittizio che conta le richieste a `/__bili/status`:

```rust
#[test]
fn drain_waits_for_in_flight_then_stops() {
    // status sequence: 2, 1, 0, 0 -> Draining stays through the first three polls,
    // then Stopped with Reason::None; exactly one POST /__bili/shutdown is observed.
}
#[test]
fn unknown_in_flight_is_not_idle() {
    // status never answers -> phase is Draining until drain_timeout, then Stopped
    // with Reason::DrainTimedOut; elapsed >= drain_timeout.
}
#[test]
fn start_during_drain_cancels_it() {
    // Drain, then Start before in_flight reaches 0 -> Ready, no shutdown POST.
}
#[test]
fn stop_is_still_immediate() {
    // Stop with in_flight 5 -> shutdown POST without waiting for 0.
}
```

- [ ] **Step 2:** `cargo test -p shinatama-engine sidecar_supervisor` → i quattro nuovi FAIL.
- [ ] **Step 3: Implementare.** `publish` deve azzerare `drain_deadline_unix_ms` uscendo da `Draining`.
- [ ] **Step 4:** stesso comando → tutti PASS, compresi `live_owner_with_occupied_port_stays_busy_and_probe_backoff_caps_and_recovers` e `graceful_shutdown_uses_same_identity_verified_connection`.
- [ ] **Step 5:** `cargo test -p shinatama-engine --test sidecar_supervisor -- --ignored --test-threads=1` → PASS (riesecuzione C13 con il nuovo stato).
- [ ] **Step 6: Commit** `feat(sidecar): drain on real in-flight traffic before a requested stop`

### Task 4: Stop coordinato con le sessioni agente

**Files:**
- Modify: `apps/desktop/src-tauri/src/agents.rs:262` (impl `Controller`)
- Modify: `apps/desktop/src-tauri/src/sidecar.rs:31-39,63-121,161-169,179-217`
- Modify: `apps/desktop/src-tauri/src/main.rs:537-548`

**Interfaces:**
- Consumes: `Command::Drain`, `Phase::Draining`, `Snapshot::in_flight` (Task 3).
- Produces:

```rust
// agents.rs
impl Controller { pub fn live_agent_sessions(&self) -> usize; } // terminals whose agent process is alive

// sidecar.rs
pub enum StopOutcome { Draining(Status), Refused { agent_sessions: usize, status: Status } }
impl Controller {
    pub fn stop_requested_by_user(&self, agent_sessions: usize, force: bool)
        -> Result<StopOutcome, &'static str>;
}
// Status gains: pub in_flight: Option<u64>, pub agent_sessions: usize,
//               pub drain_deadline_unix_ms: Option<i64>; phase may be "draining"
```

Comando Tauri: `sidecar_stop(window, force: bool)`. Risposta JSON: lo `Status`, più `"refused": true` quando rifiutato.

Regole:

- `agent_sessions > 0 && !force` → `Refused`, nessun comando al supervisor.
- Altrimenti → `Command::Drain`.
- `qualified_owner` continua a esigere `phase == "ready"`: durante `draining` nessun nuovo agente ottiene un permesso di lancio. Messaggio d'errore per quel caso: `"The model proxy is stopping. Start it again before opening an agent."`
- `stop()` (immediato) resta invariato e privato all'uso di `stop_and_wait`.

- [ ] **Step 1: Test che falliscono** nel modulo `tests` di `sidecar.rs`, su una funzione pura estratta:

```rust
#[test]
fn stop_is_refused_while_agents_are_live_unless_forced() {
    assert_eq!(decide_stop(2, false), StopDecision::Refuse);
    assert_eq!(decide_stop(2, true), StopDecision::Drain);
    assert_eq!(decide_stop(0, false), StopDecision::Drain);
}
#[test]
fn draining_owner_issues_no_launch_permit() {
    assert!(!launch_allowed("draining", true));
    assert!(launch_allowed("ready", true));
    assert!(!launch_allowed("ready", false));
}
```

- [ ] **Step 2:** `cargo test -p shinatama-desktop sidecar::tests` → FAIL.
- [ ] **Step 3: Implementare** `decide_stop`, `launch_allowed`, `stop_requested_by_user`, `live_agent_sessions` e il parametro `force`. In `status()` mappare `Phase::Draining => "draining"`.
- [ ] **Step 4:** stesso comando → PASS; `cargo test -p shinatama-desktop` → nessuna regressione.
- [ ] **Step 5: Commit** `feat(sidecar): refuse or drain a user stop while agent sessions are live`

### Task 5: Pannello «Model proxy» senza popup (fase 2)

**Files:**
- Create: `apps/desktop/src/sidecar-status.ts`
- Create: `apps/desktop/tests/sidecar-status.test.ts`
- Rewrite: `apps/desktop/src/sidecar-control.ts`
- Modify: `apps/desktop/src/styles.css` (regole del pannello)

**Interfaces:**
- Consumes: comandi `sidecar_status`, `sidecar_start`, `sidecar_stop { force }`, `sidecar_retry`; `Status` di Task 2 e 4.
- Produces:

```ts
export type ProxyStatus = {
  phase: string; healthy: boolean; message: string | null;
  inFlight: number | null; agentSessions: number;
  drainDeadlineUnixMs: number | null; configDrift: boolean | null;
  refused?: boolean;
};
export function parseStatus(value: unknown): ProxyStatus | null;   // null on any shape error
export function phaseLabel(phase: string): string;
export function buttons(status: ProxyStatus | null, outstanding: boolean):
  { start: boolean; stop: boolean; retry: boolean };                // true = enabled
export function stopConsequence(status: ProxyStatus): string | null; // null = no confirmation needed
export function trafficLine(status: ProxyStatus): string;
```

Testi fissi:

- Nome del servizio ovunque: `Model proxy` (sostituisce `Session index`; id DOM `model-proxy`, `model-proxy-panel`).
- Descrizione: `Every agent reaches its model through this local service. Recovery never resends an agent request.`
- `phaseLabel('draining')` → `Stopping after current requests…`
- `stopConsequence` con `agentSessions > 0` → `` `${n} agent session${n === 1 ? '' : 's'} will lose the connection to the model. Requests already running are allowed to finish.` ``
- `trafficLine`: `inFlight === null` → `Traffic not observed`; `0` → `No request running`; `n` → `` `${n} request${n === 1 ? '' : 's'} running` ``.
- `configDrift === true` → riga `Custom configuration: values differ from the Shinatama baseline.`

Comportamento:

- Il pulsante della toolbar apre un **pannello laterale** (elemento `aside`, non `dialog`, nessuna chiamata a `openModal`). Chiusura con Esc e con il pulsante; il focus torna al pulsante della toolbar.
- Stop con `stopConsequence !== null` non invia nulla: mostra nel pannello una conferma in linea con il testo sopra e due pulsanti, `Keep running` (default, focus) e `Stop anyway`. Solo `Stop anyway` invoca `sidecar_stop { force: true }`.
- Stop senza conseguenze invoca `sidecar_stop { force: false }`.
- In `draining`: Stop disabilitato, Start abilitato con etichetta `Cancel stop`.
- Il polling resta a 1500 ms.

- [ ] **Step 1: Test che falliscono** in `apps/desktop/tests/sidecar-status.test.ts` (runner `node:test`, come gli altri file della cartella):

```ts
test('stop needs confirmation only while agents are live', () => {
  assert.equal(stopConsequence(status({ agentSessions: 0 })), null);
  assert.match(stopConsequence(status({ agentSessions: 1 }))!, /^1 agent session will lose/);
  assert.match(stopConsequence(status({ agentSessions: 3 }))!, /^3 agent sessions will lose/);
});
test('unknown traffic is never shown as idle', () => {
  assert.equal(trafficLine(status({ inFlight: null })), 'Traffic not observed');
  assert.equal(trafficLine(status({ inFlight: 0 })), 'No request running');
});
test('draining disables stop and offers cancel through start', () => {
  assert.deepEqual(buttons(status({ phase: 'draining' }), false), { start: true, stop: false, retry: false });
});
test('ready no longer reads as a guarantee: stop stays available, start does not', () => {
  assert.deepEqual(buttons(status({ phase: 'ready' }), false), { start: false, stop: true, retry: false });
});
test('a malformed status is rejected whole', () => {
  assert.equal(parseStatus({ phase: 'ready', healthy: 'yes' }), null);
  assert.equal(parseStatus({ phase: 'nonsense', healthy: true, message: null }), null);
});
```

- [ ] **Step 2:** `npm test` → i cinque nuovi FAIL.
- [ ] **Step 3: Implementare** `sidecar-status.ts`, poi riscrivere `sidecar-control.ts` usando solo quelle funzioni per ogni decisione.
- [ ] **Step 4:** `npm test` → PASS. `npm run build` → nessun errore `tsc`.
- [ ] **Step 5: Verifica nell'app reale** (procedura CDP del progetto): aprire un agente, premere Stop, osservare la conferma in linea con `1 agent session will lose…`; `Keep running` lascia `Ready`; `Stop anyway` porta a `Stopping after current requests…` poi `Stopped`. Nessun `dialog[open]` nel DOM in nessun passaggio.
- [ ] **Step 6: Commit** `feat(desktop): model proxy panel with a coordinated, confirmed stop`

### Task 6: Origine sintetica e prova SC9 (bypass dei messaggi di controllo)

**Files:**
- Create: `crates/engine/tests/fixtures/origin.mjs`
- Create: `crates/engine/tests/sidecar_integrity.rs`

**Interfaces:**
- Consumes: `shinatama_hop::channel::bootstrap(nonce: &str, seed_path: &Path, last_user_request: Option<&str>) -> String` (`crates/hop/src/channel.rs:52`); il supporto di lancio del sidecar già usato da `crates/engine/tests/effective_sidecar.rs` e `crates/engine/tests/support`.
- Produces: `origin.mjs` — server HTTP su porta scelta dal sistema che stampa `{"port":N}` su stdout, salva ogni corpo ricevuto in `<dir>/<n>.body` con `<n>.json` (metodo, path, header senza `authorization`/`x-api-key`), risponde con uno stream SSE Anthropic valido di un solo blocco di testo. Argomenti: `--dir <path>`, `--delay-ms <n>` (ritardo prima dell'ultimo evento, default 0).

Funzione di supporto nel test:

```rust
/// Sends `user_text` as the last user message of an Anthropic Messages request through
/// the running sidecar and returns the bytes of that message as the origin received them.
fn last_user_text_at_origin(sidecar: &Running, origin: &Origin, history: &[Msg], user_text: &str) -> String;
```

- [ ] **Step 1: Scrivere i test** (tutti `#[ignore = "native: starts the pinned sidecar"]`):

```rust
#[test] #[ignore = "native: starts the pinned sidecar"]
fn bootstrap_reaches_origin_byte_identical() {
    let text = bootstrap("n-1", Path::new("C:\\synthetic\\seed.md"), Some("Continua il lavoro."));
    assert_eq!(last_user_text_at_origin(&sidecar, &origin, &[], &text), text);
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn bootstrap_is_untouched_with_history_near_the_limit() {
    // history: 150_000 estimated tokens of tool results, each > 4000 tokens
    assert_eq!(last_user_text_at_origin(&sidecar, &origin, &history, &text), text);
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn first_destination_turn_sends_no_auxiliary_request() {
    // origin directory holds exactly one body after the turn: no rewrite call, no summary call
    assert_eq!(origin.bodies().len(), 1);
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn dictated_prompt_is_kept_and_any_rewrite_note_is_additive() {
    // a 40-word Italian request with a negation and a figure: the original text is present
    // byte-identical at the origin; anything added is a separate adjacent block.
}
```

- [ ] **Step 2: Eseguire** `cargo test -p shinatama-engine --test sidecar_integrity -- --ignored --test-threads=1`.
- [ ] **Step 3: Registrare l'esito.** Questa è una prova, non una funzione: il risultato decide il Task 12.
  - Tutti PASS → SC9 è soddisfatto dal pin corrente; annotarlo nel Task 8 e **saltare il Task 12**.
  - Un FAIL sui primi tre → non correggere in Shinatama. Marcare i test falliti con `#[ignore = "SC9 open: see Task 12"]`, lasciare l'asserzione intatta, e il Task 12 diventa obbligatorio.
- [ ] **Step 4: Commit** `test(sidecar): SC9 byte comparison of control messages through the pinned sidecar`

### Task 7: Prova di isolamento tra sessioni e prova di drain a metà stream

**Files:**
- Modify: `crates/engine/tests/sidecar_integrity.rs`

**Interfaces:**
- Consumes: `origin.mjs --delay-ms`, supporto di Task 6, `sidecar_admin::in_flight`.

- [ ] **Step 1: Scrivere i test** (`#[ignore]` come sopra):

```rust
#[test] #[ignore = "native: starts the pinned sidecar"]
fn two_sessions_with_identical_prefixes_do_not_share_state() {
    // Sessions A and B (distinct x-claude-code-session-id), identical system prompt and first
    // 10 messages, run concurrently for 20 turns each. A carries a 6000-token tool result
    // containing MARKER_A, B one containing MARKER_B.
    // At the origin: no body of A contains MARKER_B and no body of B contains MARKER_A.
    // GET /__bili/stats lists two distinct session ids, each with requests == 20.
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn resumed_session_keeps_its_own_identity() {
    // A sends 5 turns, the client reconnects with the same session id and sends 5 more:
    // /__bili/stats shows one session with requests == 10, not two.
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn drain_lets_a_running_stream_finish() {
    // origin --delay-ms 3000; client starts a streamed turn; after the first SSE byte the
    // supervisor receives Command::Drain. The client receives the complete stream ending in
    // message_stop; the origin holds exactly ONE body; the final phase is Stopped.
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn killed_sidecar_never_replays_the_interrupted_request() {
    // same setup; the sidecar process is terminated after the first SSE byte. The supervisor
    // restarts it once on the same port; the origin still holds exactly ONE body.
}
```

- [ ] **Step 2: Eseguire** `cargo test -p shinatama-engine --test sidecar_integrity -- --ignored --test-threads=1` → i quattro PASS. Un FAIL di `two_sessions…` o `resumed_session…` è un difetto P0 del motore: fermarsi, non procedere con la Parte B, aprire la correzione nella sorgente del sidecar con il test come riproduzione.
- [ ] **Step 3: Commit** `test(sidecar): session isolation and no-replay under drain and crash`

### Task 8: SC16 soak e registro delle verifiche

**Files:**
- Create: `crates/engine/tests/sidecar_soak.rs`
- Create: `docs/verification/2026-11-13-SC16-soak.md` (data del giorno di esecuzione)
- Modify: `docs/verification-register.md:387,394`

**Interfaces:**
- Consumes: `origin.mjs`, supporto di Task 6, `sidecar_admin::{get_json, in_flight}`.
- Produces: variabile d'ambiente `SHINATAMA_SOAK_SECONDS` (default `3600`); file risultato JSON in `target/sc16-soak.json` con i campi `seconds, clients, requests, refusedConnections, connectMsP50, connectMsP95, connectMsMax, healthMsMax, longestHoldMs, stdoutBytes, stderrBytes`.

Carico fisso: 5 client concorrenti, ciascuno su una sessione con storia crescente fino a 150.000 token stimati, risultati tool oltre 4.000 token a ogni terzo turno. Ogni 250 ms un sesto client misura `CONNECT` più eco e `GET /__bili/health`.

- [ ] **Step 1: Scrivere il test** `#[ignore = "native soak: one hour"] fn five_instances_for_one_hour_refuse_no_connection()` con le asserzioni:

```rust
assert_eq!(result.refused_connections, 0);
assert!(result.connect_ms_max < 750, "SC16 local CONNECT bound");
assert!(result.health_ms_max < 750, "SC16 local health bound");
assert!(result.stdout_bytes < 64 * 1024 * 1024 && result.stderr_bytes < 64 * 1024 * 1024);
```

- [ ] **Step 2: Prova breve** `SHINATAMA_SOAK_SECONDS=120 cargo test -p shinatama-engine --test sidecar_soak -- --ignored` → PASS, file JSON scritto.
- [ ] **Step 3: Prova completa** con il default di un'ora → registrare l'esito qualunque sia.
- [ ] **Step 4: Scrivere** `docs/verification/<data>-SC16-soak.md` con revisione Shinatama, fingerprint del pin, i numeri del JSON, e la frase esplicita su ciò che la prova non copre: ritardo dell'event loop interno (`monitorEventLoopDelay` richiede il motore) e traffico verso provider reali.
- [ ] **Step 5: Aggiornare** le righe SC9 e SC16 di `docs/verification-register.md` con esito e collegamento. SC16 resta «aperto» se la prova completa fallisce o se manca la misura dell'event loop: scrivere quale delle due.
- [ ] **Step 6: Commit** `test(sidecar): SC16 one-hour soak and verification records`

---

# Parte B — Sorgente del sidecar e ciò che ne dipende

Le modifiche di questa parte si scrivono in `sidecar/source-build/prepared` e si ricostruiscono lì. Iniziare soltanto dopo che Task 7 è verde.

### Task 9: Endpoint delle capacità per sessione (sorgente del sidecar)

**Files:**
- Create: `sidecar/source-build/prepared/src/server/capabilities.ts`
- Modify: `sidecar/source-build/prepared/src/server/admin.ts` (accanto alla rotta `/__bili/stats`, riga 210 del file preparato)
- Modify: `sidecar/source-build/prepared/src/image-rescale.ts` (esporre lo stato dell'encoder)
- Test: `sidecar/source-build/prepared/src/server/capabilities.test.ts` (runner `node --test`)

**Interfaces:**
- Consumes: `effectiveCcr(session)` (`src/store.ts:49`), lo stamp di `storeEffectiveImageRescale` (`src/store.ts:61-79`), `opts.compress`, `opts.passthrough`, il risolutore d'identità di `src/session-id.ts`, il profilo scelto in `src/server/handle.ts` (da riga 620).
- Produces: `GET /__bili/capabilities?native=<id>` → `200`:

```json
{
  "schemaVersion": 1,
  "sessionId": "<proxy session id>",
  "nativeId": "<id>",
  "protocol": "anthropic|openai|responses|google",
  "requests": 12,
  "profile": "analysis|build|null",
  "passthrough": false,
  "ccr": { "configured": true, "applied": false, "reason": "codex_host" },
  "readOriginalImage": { "applied": false, "reason": "codex_host" },
  "imageRescale": { "configured": true, "applied": false, "reason": "encoder_missing" },
  "absorb": { "configured": true, "mediaOnly": true },
  "tiers": { "configured": true },
  "steering": { "configured": true, "level": 2, "effortRouting": false },
  "promptRewrite": { "mode": "auto" }
}
```

Codici `reason`, insieme chiuso: `none`, `not_configured`, `inject_tool_off`, `codex_host`, `plugin_wire_not_allowed`, `text_protocol`, `passthrough`, `encoder_missing`, `not_observed`.

Regole:

- Sessione sconosciuta o `native` assente → `404 {"error":"unknown session"}`. Mai valori di default.
- `applied` di `ccr` e `readOriginalImage` viene **solo** dallo stamp dell'ultima richiesta servita. Il `reason` si calcola nello stesso punto in cui oggi si decide lo stamp (`handle.ts:1759-1802`) e si salva con esso: l'endpoint non ricalcola il gate.
- `imageRescale.applied` è `true` solo dopo che l'import dinamico dell'encoder è riuscito almeno una volta in questo processo; `encoder_missing` dopo un import fallito; `not_observed` se non è mai stato tentato.
- La rotta passa dagli stessi gate loopback/origine di tutte le rotte `/__bili/`.

- [ ] **Step 0: Versionare la sorgente del sidecar.** Oggi `sidecar/source-build/.gitignore` esclude tutta `prepared/`: una modifica lì non entrerebbe in git. Sostituire la riga `prepared/` con `prepared/node_modules/` e `prepared.incomplete/`, poi `git add sidecar/source-build/prepared` e commit separato, senza altre modifiche: `chore(sidecar): track the sidecar source in this repository`. Verificare prima con `git status --short sidecar/source-build | wc -l` che il numero di file corrisponda alle voci del lock.
- [ ] **Step 1: Test che falliscono** in `capabilities.test.ts`:

```ts
test('codex_host_reports_ccr_disarmed_with_reason', …)        // responses wire + codex host headers
test('anthropic_proxy_lane_reports_ccr_applied', …)           // reason "none"
test('plugin_google_wire_reports_plugin_wire_not_allowed', …)
test('unknown_session_is_404_not_defaults', …)
test('encoder_missing_is_reported_after_a_failed_import', …)
test('reason_is_the_one_stamped_by_the_request_not_recomputed', …) // change config after the request; reason unchanged
```

- [ ] **Step 2:** `node --experimental-strip-types --test sidecar/source-build/prepared/src/server/capabilities.test.ts` → i sei FAIL.
- [ ] **Step 3: Implementare.** Estendere lo stamp con il `reason`; non cambiare la decisione del gate né l'ordine della pipeline.
- [ ] **Step 4:** suite completa → PASS, nessun test esistente modificato.
- [ ] **Step 5: Commit** `feat(sidecar): per-session capabilities endpoint with stamped disarm reasons`

### Task 10: Ricostruzione del sidecar e lettura delle capacità nell'engine

**Files:**
- Modify: `sidecar/source-build/provenance.lock.json` (nuove impronte)
- Modify: `sidecar/prepare.ps1` (verifica solo l'albero di questo repository)
- Rebuild: `sidecar/source-build/prepared/dist/**`
- Modify: `crates/engine/src/sidecar_admin.rs`
- Modify: `apps/desktop/src-tauri/src/sidecar.rs`, `apps/desktop/src-tauri/src/main.rs`
- Modify: `crates/engine/tests/sidecar_integrity.rs`

**Interfaces:**
- Consumes: endpoint di Task 9.
- Produces:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lever { pub configured: Option<bool>, pub applied: Option<bool>, pub reason: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCapabilities {
    pub schema_version: u64, pub protocol: String, pub requests: u64,
    pub profile: Option<String>, pub passthrough: bool,
    pub ccr: Lever, pub read_original_image: Lever, pub image_rescale: Lever,
    pub steering_level: Option<u64>, pub effort_routing: Option<bool>,
    pub prompt_rewrite_mode: Option<String>,
}
pub enum CapabilityRead { Observed(SessionCapabilities), NotObserved, Unavailable }
pub fn session_capabilities(target: &AdminTarget, native_id: &str) -> CapabilityRead;
```

`404` → `NotObserved`. `schemaVersion != 1`, timeout, identità diversa, corpo non valido → `Unavailable`. `native_id` è accettato solo se lungo 1–128 caratteri di `[A-Za-z0-9._:-]`; altrimenti `Unavailable` senza inviare nulla.

Comando Tauri: `sidecar_capabilities(window, native_id: String) -> { state: "observed"|"not_observed"|"unavailable", capabilities?: SessionCapabilities }`, eseguito in `spawn_blocking`.

- [ ] **Step 1: Ricostruire.** Da `sidecar/source-build/prepared`: `node scripts/build.mjs`. Aggiornare nel lock le impronte dei file sorgente cambiati, di `dist/index.js` (`entrySha256`) e degli alberi. Ridurre `sidecar/prepare.ps1` alla sola verifica dell'albero in questo repository contro il lock (via i parametri `SourceRoot`/`PreparedRoot` e i controlli che leggono fuori dal repository). Poi `pwsh -NoProfile -File sidecar/prepare.ps1` → nessuna differenza.
- [ ] **Step 2: Non regressione.** `cargo test -p shinatama-engine --test sidecar_runtime --test sidecar_supervisor --test effective_sidecar --test sidecar_integrity -- --ignored --test-threads=1` → PASS. Poi `SHINATAMA_SOAK_SECONDS=600 cargo test -p shinatama-engine --test sidecar_soak -- --ignored` → PASS.
- [ ] **Step 3: Test unitari che falliscono** in `sidecar_admin.rs`:

```rust
#[test] fn not_found_is_not_observed() { /* 404 -> CapabilityRead::NotObserved */ }
#[test] fn other_schema_version_is_unavailable() { /* schemaVersion 2 -> Unavailable */ }
#[test] fn invalid_native_id_sends_nothing() { /* "a b", "", 129 chars -> Unavailable, 0 requests */ }
#[test] fn codex_payload_parses_with_reason() { /* ccr.applied == Some(false), reason "codex_host" */ }
```

- [ ] **Step 4:** `cargo test -p shinatama-engine sidecar_admin` → FAIL, poi implementare → PASS.
- [ ] **Step 5: Prova nativa** in `sidecar_integrity.rs`:

```rust
#[test] #[ignore = "native: starts the pinned sidecar"]
fn capabilities_match_what_the_origin_received() {
    // Anthropic lane, 6000-token tool result: capabilities.ccr.applied == Some(true) AND the
    // origin body holds a placeholder instead of the result.
    // Responses lane with codex host headers: ccr.applied == Some(false), reason "codex_host",
    // AND the origin body holds the result verbatim.
}
#[test] #[ignore = "native: starts the pinned sidecar"]
fn large_png_reports_the_encoder_state_it_actually_used() {
    // 2400x1600 PNG over 100 KiB: either the origin receives a JPEG no wider than 1600 and
    // imageRescale.applied == Some(true), or it receives the original bytes and
    // reason == "encoder_missing". Any other combination fails. No crash in both cases.
}
```

- [ ] **Step 6:** eseguire le due prove → PASS. Annotare quale ramo della seconda si è verificato: è la risposta alla domanda su `jimp` dell'audit.
- [ ] **Step 7: Commit** `feat(sidecar): rebuild with per-session capabilities and read them in the engine`

### Task 11: Riga di capacità nel dettaglio sessione (T9)

**Files:**
- Modify: `apps/desktop/src/sidecar-status.ts`
- Modify: `apps/desktop/tests/sidecar-status.test.ts`
- Modify: `apps/desktop/src/session-detail.ts:156-165` (accanto al ledger)
- Modify: `apps/desktop/src/session-detail.css`

**Interfaces:**
- Consumes: comando `sidecar_capabilities { nativeId }` (Task 10).
- Produces:

```ts
export type Lever = { configured: boolean | null; applied: boolean | null; reason: string };
export type CapabilityState =
  | { state: 'observed'; capabilities: Capabilities }
  | { state: 'not_observed' } | { state: 'unavailable' };
export function leverText(name: string, lever: Lever): string;
export function capabilityLines(read: CapabilityState): string[];
```

Testi fissi:

- `not_observed` → una sola riga: `Not observed yet: this session has sent no request through the model proxy.`
- `unavailable` → `Model proxy state unavailable.`
- Leva applicata → `` `${name}: active` ``.
- Leva configurata ma non applicata → `` `${name}: configured, not active here — ${REASONS[reason]}` ``.
- Leva non configurata → `` `${name}: off` ``.
- `REASONS`: `codex_host` → `this agent cannot call the retrieval tool`; `plugin_wire_not_allowed` → `not supported on this connection`; `inject_tool_off` → `tool injection is disabled`; `text_protocol` → `this connection has no tool channel`; `passthrough` → `transformations are paused`; `encoder_missing` → `the image encoder is not installed`; `not_observed` → `not attempted yet`; qualunque altro valore → `reason not reported`.
- Nomi delle leve: `Large-result storage` (ccr), `Original image retrieval` (readOriginalImage), `Image downscaling` (imageRescale).
- Ultima riga sempre presente quando `observed`: `Economic saving is not measured.`

La riga si aggiorna all'apertura del dettaglio e ogni 5 s mentre è visibile; nessuna richiesta quando è chiuso.

- [ ] **Step 1: Test che falliscono:**

```ts
test('unobserved_session_renders_unknown', () => {
  assert.deepEqual(capabilityLines({ state: 'not_observed' }),
    ['Not observed yet: this session has sent no request through the model proxy.']);
});
test('a configured but disarmed lever states why', () => {
  assert.equal(leverText('Large-result storage', { configured: true, applied: false, reason: 'codex_host' }),
    'Large-result storage: configured, not active here — this agent cannot call the retrieval tool');
});
test('an unknown reason is never shown raw', () => {
  assert.match(leverText('Image downscaling', { configured: true, applied: false, reason: '<x>' }), /reason not reported$/);
});
test('observed state never claims a saving', () => {
  assert.equal(capabilityLines(observed()).at(-1), 'Economic saving is not measured.');
});
```

- [ ] **Step 2:** `npm test` → FAIL; implementare; `npm test` e `npm run build` → PASS.
- [ ] **Step 3: Verifica nell'app reale** via CDP: sessione Claude dopo un turno con risultato grande → `Large-result storage: active`; sessione Codex → `configured, not active here — this agent cannot call the retrieval tool`; sessione appena aperta → `Not observed yet…`. Confrontare ogni riga con `state/bili.log` della stessa sessione (criterio T9: la riga coincide con il log in tutte le combinazioni).
- [ ] **Step 4: Aggiornare** la riga T9 di `docs/verification-register.md:515` con esito e data.
- [ ] **Step 5: Commit** `feat(desktop): per-session proxy capabilities, configured versus active`

### Task 12: Bypass esplicito dei messaggi di controllo — solo se Task 6 ha fallito

**Files:**
- Modify: `sidecar/source-build/prepared/src/musashi-terminal/request-protection.ts`
- Modify: `sidecar/source-build/prepared/src/musashi-terminal/rewrite-hook.ts`, `sidecar/source-build/prepared/kernel/src/ccr.ts` (punti di ammissione)
- Modify: `crates/hop/src/channel.rs:52-67` (emissione del marcatore)
- Modify: `crates/engine/tests/sidecar_integrity.rs` (rimuovere gli `#[ignore = "SC9 open…"]`)

**Interfaces:**
- Produces: un marcatore strutturale di messaggio di controllo, riconosciuto **prima** di ogni mutazione. Forma: header HTTP della richiesta `x-shinatama-control: <kind>;<sha256 esadecimale del testo dell'ultimo messaggio user>` con `kind ∈ {seed, bootstrap, receipt}`. Il sidecar applica il bypass solo se l'hash coincide con il testo ricevuto; hash diverso → trattamento ordinario e una riga di log `control marker mismatch`.

Vincolo: il marcatore non si deduce mai dal testo del prompt, dalla sua lunghezza o da `max_tokens`. Se l'harness non permette a Shinatama di aggiungere un header alla richiesta del modello, questo task si ferma e lo si documenta come limite di quell'harness nel registro: non si ripiega su un marcatore nel testo.

- [ ] **Step 1: Verificare la fattibilità per harness.** Per Claude, Codex, OpenCode e Pi stabilire se la ricetta di lancio (`crates/engine/src/*_launch.rs`) può aggiungere un header alle richieste del modello. Scrivere l'esito in `docs/notes/<data>-control-marker-feasibility.md`. Procedere solo per gli harness in cui è possibile.
- [ ] **Step 2: Test che falliscono** nella sorgente del sidecar: `marked_message_skips_rewrite_ccr_and_fold`, `mismatched_hash_gets_ordinary_treatment`, `unmarked_message_is_unchanged_behaviour`.
- [ ] **Step 3: Implementare** nel motore, poi l'emissione in Shinatama.
- [ ] **Step 4: Ricostruire** come Task 10, Step 1–2.
- [ ] **Step 5:** `cargo test -p shinatama-engine --test sidecar_integrity -- --ignored --test-threads=1` → i tre test SC9 PASS senza `ignore` di ripiego.
- [ ] **Step 6: Aggiornare** SC9 nel registro e fare **Commit** `feat(sidecar): structural bypass for seed, bootstrap and receipt`

---

## Gate di uscita di questo piano

Tutti devono essere veri prima di aprire una fase successiva:

- `cargo test --workspace` e `npm test` verdi; `npm run build` senza errori.
- `sidecar_integrity` (tutti i test) e `sidecar_supervisor` nativi verdi sul pin corrente.
- SC16: prova di un'ora con zero connessioni rifiutate e limiti sotto 750 ms, registrata.
- SC9: verde, oppure limite per harness scritto nel registro.
- T9: la riga di capacità coincide con il log del sidecar per Claude e Codex.
- Nessun `dialog` usato dal pannello del proxy; Stop con agenti vivi richiede conferma in linea.

## Piani successivi (non in questo documento)

| Piano | Contenuto dello studio | Gate d'ingresso |
|---|---|---|
| P3 — Ricevute e ledger | §12.4: eventi del proxy correlati alle richieste native, prima/dopo, costo ausiliario, latenze | Gate di questo piano |
| P4 — Policy in osservazione | §§5–6: contratto d'intento, manifest protetti, candidati calcolati senza modificare le richieste | P3 produce dati per almeno 10 sessioni per harness |
| P5 — Riduzioni conservative | §6.3 punti 1–3, §7: deduplicazione esatta, CCR verificata per lane | P4 confrontato con valutazione umana sul corpus §13.3 |
| P6 — Intento e immagini | §5.3, §9: profili oltre `analysis`/`build`, manifest immagini, recupero durevole | P5 senza regressioni di qualità |
| P7 — Output | §8: L1/L2 per compito, prompt versionati, `effortRouting` resta `false` | P6 |
| P8 — Estensione | §14 fase 7: una lane o un modello alla volta | Capacità qualificata nella matrice del Task 10 |

P4–P7 richiedono modifiche al motore e un binding d'identità sessione→policy (§11.3 dello studio): quel binding va progettato in un documento proprio prima di scrivere P4.
