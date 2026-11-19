# C13 — riesame pre-chiusura del checkpoint padre (2026-11-04)

Contesto: le unità C13.1–C13.5 risultano committed a `c0f535e` (2026-11-03) con
ricevuta [2026-11-03-C13-sidecar-supervision.result.json](2026-11-03-C13-sidecar-supervision.result.json).
Il passo C13 resta `pending` in attesa della chiusura del padre. Prima di
chiudere, le prove registrate sono state rieseguite sull'HEAD corrente
(`b9465e4`, albero prodotto identico a c0f535e per il codice desktop: le
modifiche successive sono solo test/docs più dffb269..f8df5af già committati).

## 1. Prova nativa del supervisore — RIPASSATA

```text
cargo test --offline --locked -p shinatama-engine -j1 --test sidecar_supervisor -- --ignored --test-threads=1 --nocapture
```

**PASS 1/1 in 4,66 s** (2026-11-04). Evidenza macchina riscritta:
`.prototype-evidence/sidecar-supervisor.json` — `passed:true`,
`busyKeptExactOwner:true`, `busyProbeDelayMs:1000`, `automaticRestarts:1`,
`sameAddressAfterRestart:true`, `secondExitPhase:failed/repeated_exit`,
`prepareCalls:2`, `savedMarkerPreserved:true`, `agentCommandsReplayed:false`,
`allNativeJobsClosedAndLedgerReleased:true`. Limiti invariati: una CLI pinned
su Windows, pausa finita del thread principale e due uscite controllate;
nessun soak esteso né traffico provider.

Un primo tentativo è fallito con `AdmissionDeferred`: il gate di headroom
C09R del test (≥1 GiB+1 GiB commit libero) ha rifiutato l'ammissione con
~1,8 GiB di commit libero di sistema. Non è un difetto del supervisore: è
l'ammissione fail-closed che funziona; il rerun con headroom sufficiente è
passato.

## 2. desktop-smoke — prova registrata STALE, corretto l'allineamento

Il check registrato nelle unità C13
(`verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/desktop-smoke.mjs`)
**fallisce sull'HEAD corrente per obsolescenza del test, non del prodotto**:
`dffb269` (2026-11-03, "open project sessions with one action") ha spostato
la persistenza del layout allo scope **globale** (`crates/state/src/global_workspace.rs`;
AVVIO.md: "Il workspace può mostrare viste di progetti diversi"), ma
`desktop-smoke.mjs` (ultimo tocco `c0f535e`) interrogava ancora
`workspace_load` con scope per-progetto, che ora restituisce legittimamente
`null`. Corretto il test a interrogare `global_workspace_load`
(`{workspaceId:'main'}`), stesso pattern di `new-session-smoke.mjs`.

Esito dopo la correzione: la sonda di ownership **passa** (`.workspace-name`
owned/not-dirty/not-pending e record globale `ownership:'ours'`); lo smoke
prosegue e si ferma oltre, al gate `#resource-status` "Resources: available",
per **pressione di commit reale del sistema**: headroom osservato 917 MiB con
stato governor "Resources: reduced activity / New sessions: waiting for
resources" — comportamento C09R corretto sotto pressione, non una regressione.

## 3. Blocco ambientale aperto

Al 2026-11-04 18:0x la macchina ha commit libero ~0,9–2,0 GiB su ~44 GiB di
commit limit (sessioni agente concorrenti, script utente, WebView2). Il
governor desktop rientra in "available" solo sopra la soglia di recovery
calibrata in C09R.3; lo smoke completo (terminali reali, check Claude, sidecar
ready) richiede headroom adeguato. **La chiusura del padre C13 resta aperta
finché un run verde completo di desktop-smoke non è registrato**; nessuna
prova è dichiarata passata senza esecuzione.

## 4. Decisione process-wrap (gate C13.1) — registrata

Valutazione su fonti ufficiali (lib.rs/crates/process-wrap e
docs.rs/process-wrap, consultate 2026-11-04): v10.0.1 (2026-09-23),
Apache-2.0 OR MIT, manutenzione attiva (watchexec org, CI), `JobObject` con
CREATE_SUSPENDED temporaneo e resume post-assegnazione. **Rifiuto motivato**
registrato in `[workspace.dependencies]` (Cargo.toml): capacità già presente
e nativamente provata in-tree (spawn sospeso in job sigillato + ledger
ammissioni C09R), nessuna composizione con ledger/censimento/credit senza
wrapper custom comunque necessario, costo dipendenze contro ADR-0047
(`windows` 0.62 vs windows-sys 0.61, indexmap, tracing), policy MSRV
("solo latest stable", bump senza major) contro toolchain pinned 1.99.1.
watchexec-supervisor ed expectrl non usati, come da scheda.

## Riepilogo stato

| Voce | Stato |
| --- | --- |
| Prova nativa supervisore su HEAD corrente | PASS (rieseguita) |
| desktop-smoke allineato allo scope globale | corretto; sonda ownership verde |
| desktop-smoke run verde completo | **bloccato da headroom di commit del sistema** |
| Decisione process-wrap | registrata (rifiuto motivato) |
| Chiusura padre C13 | **aperta**, in attesa del run verde |
