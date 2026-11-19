# ADR-0051 implementato: certificato di terminazione e recupero pre-ricevuta

Data: 2026-11-06. ADR-0051 **Accepted** (utente, 6 nov 2026).

## Cosa è implementato

- **Sonda job di recupero** (`crates/engine/src/resources/job.rs`,
  `probe_job_recovery`): apre il job nominato `Local\Shinatama.Workload.<hex(job_key)>`
  con `QUERY|TERMINATE`. Nome assente → certificato `JobNameAbsent` (con la
  ricetta kill-on-close pinned e senza breakaway, l'ultimo handle chiuso ha già
  terminato il gruppo). Nome presente → `TerminateJobObject`, rundown fino a
  `ActiveProcesses == 0`, chiusura handle, nuova verifica di assenza del nome →
  `TerminatedByRecovery`. Nome ancora presente dopo la chiusura → blocco esplicito
  (`RundownUnverified`), mai ricevuta fabbricata.
- **Certificato nello stato** (`crates/state/src/native_session_cleanup.rs`,
  `NativeTerminationCertificate` + `certify_native_session_termination`): in
  **una transazione** verifica revisione thread, attivazione aperta e owner
  esatto, associazione senza ricevuta, poi `record_cleanup`, rimozione dello
  slot ledger, `end_activation` e consumo dell'entry. Capacità ripristinata
  insieme alla chiusura. Ricevute già presenti non vengono mai ri-consumate.
- **Ramo desktop** (`apps/desktop/src-tauri/src/threads.rs::thread_recover`):
  con ricevuta → percorso normale invariato; senza ricevuta ma con associazione
  → sonde identità esatte (host+agente vivi = rifiuto esplicito; PID riutilizzato
  accettato), stessa sessione Windows, sonda job, poi certificato e transazione.
  Errori di accesso/rundown → blocco (`cleanup_unproven`), record intatti.

## Prove eseguite

- `cargo test -p shinatama-state native_session_cleanup::tests`: **6/6**
  (2 nuovi: certificato rilascia ticket+chiude+consuma una volta, matrice di
  rifiuti con record preservati).
- `cargo test -p shinatama-engine --lib resources::job`: **12/12**
  (2 nuovi: assenza dopo ultimo handle chiuso = certificato; nome ancora vivo
  con altro holder = blocco; chiave nulla rifiutata).
- `apps/desktop/tests/thread-recovery-smoke.mjs` (app reale, UI e IPC, DB
  privato): **PASS**. Il caso pre-ricevuta recupera con certificato
  `JobNameAbsent` (attivazione chiusa revisione+1, slot ledger rilasciato,
  entry consumata, seconda richiesta rifiutata); proprietario vivo resta
  rifiutato; recupero con ricevuta invariato.
- Suite complete: state 66/66, engine lib 105/105, desktop bins 30/30,
  rustfmt pulito sui file toccati.

## Limiti

- Il percorso felice `TerminatedByRecovery` con job ancora vivo è coperto dal
  blocco (nome che sopravvive) e dalla logica rundown condivisa con
  `WorkloadJob::terminate_and_wait`; il run completo del recupero **reale** su
  crash pre-ricevuta resta da eseguire con la smoke minima a macchina libera.
- Nessun reaper generale, nessuna riconciliazione all'avvio, nessuno sblocco
  per timeout/PID: solo richiesta esplicita per thread.
- Record storici senza associazione restano bloccati e riconoscibili.

## Effetto sul limite `Ledger(Capacity)`

Il ticket bloccato viene rilasciato atomicamente con la chiusura: la capacità
del ledger torna disponibile per nuove sessioni, risolvendo il caso in cui un
crash pre-ricevuta impediva l'apertura (4 GiB di `MAX_TOTAL`).
