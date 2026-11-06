# Recupero dopo crash forzato: stato comprovato e blocco ambientale

Data: 2026-11-05/06. Test: `apps/desktop/tests/forced-crash-recovery-smoke.mjs`
(5 fasi, 3 istanze app, crash reali `TerminateProcess`, recupero via UI reale).

## Comportamenti già provati su run reali

| Fase | Evidenza | Run |
| --- | --- | --- |
| Agent group ucciso con host vivo → finalizzazione esatta con ricevuta consumata, thread inattivo, terminale rimosso | check `agentForcedTerminationClosedExactlyWithReceiptOnLiveHost` | #21 |
| Crash app **prima** della ricevuta → `Recover session` rifiuta ("shutdown was not confirmed"), attivazione aperta e ticket esatto preservati | check `crashBeforeReceiptBlockedAndActivationAndBudgetPreserved` | #12, #16, #18, #20, #21, #23 |
| Retry corretto sul rifiuto di prodotto "Windows is low on resources" e sul canary 7 s in timeout | `evidence.openAttempts` | #12, #20 |
| Dialog con progetto preset (campi path nascosti) gestito senza fill su campo invisibile | `evidence.dialogRetries` | #18 → fix |

Non ancora completate in **un unico run** le fasi: crash dentro la finestra
della ricevuta (Stop + busy-poll + kill immediato), Recover reale su app
riavviata, Resume post-recovery.

## Blocco ambientale (misurato)

L'host è condiviso con altre sessioni agente che compilano: il commit libero è
oscillato per due giorni tra **0,09 e 8,7 GiB** (limite commit 41,34 GiB).
Il governatore del prodotto (rapporto con isteresi: ingresso 90%, uscita 87,5%
→ 4,13/5,17 GiB liberi sul limite) rifiuta correttamente le ammissioni sotto
soglia; il gate della prova attende le finestre (≥6,5 GB app#1, ≥8,1 GB app#2/#3).

## Limite di prodotto emerso (materiale per ADR-0051)

Con un ticket di una sessione bloccata (crash pre-ricevuta, 1 GiB) trattenuto
nel ledger, una nuova apertura richiede: ticket bloccato 1 GiB + sidecar 1 GiB
+ canary trial ~2 GiB = 4 GiB = `MAX_TOTAL` hard
(`terminal_runtime.rs:35`). Il tetto viene calcolato **all'avvio dell'app**
da `min(physical/2, commit/2, MAX_TOTAL)`: se l'avvio avviene sotto gli 8 GiB
di commit il tetto scende (osservato 3,75 GiB) e ogni nuova sessione viene
rifiutata con `Ledger(Capacity)`. In pratica: **un crash pre-ricevuta può
rendere inutilizzabile il ledger finché il thread non è recuperato** — e quel
recupero non esiste prima di ADR-0051. Misurato dal ledger reale della fixture
(slots: ticket bloccato cpu1000 + sidecar cpu2000 + riserva sessione cpu2000).

La smoke ora verifica il tetto persistito dopo il gate e rilancia l'app nella
stessa finestra per riportarlo a `MAX_TOTAL` (fino a 3 volte, evidenza
`evidence.ceilingRetries`); se anche con tetto massimo l'uguaglianza 4,0 GiB
non fosse ammessa, il limite è di prodotto e va registrato come tale.

## Limiti della prova

- L'esito completo richiede ~10–15 minuti continui con commit ≥8,1 GiB: con
  l'host condiviso attuale non è stato possibile in un singolo run.
- Credenziali sintetiche, provider irraggiungibile, nessun turno di modello
  completato; guardia solo-inoltro, non firewall OS.
- Le fasi provate separatamente non vengono rivendicate come un'unica
  esecuzione completa.

## Prossimi passi

1. Con macchina quieta: `verify/P14/target/debug/p14-probe.exe wrap-run node
   apps/desktop/tests/forced-crash-recovery-smoke.mjs` (attende da solo la
   finestra; esito in `.prototype-evidence/forced-crash-recovery.json`).
2. Dopo il PASS: progress C25 chiuso, nota aggiornata, pacchetto rigenerato.
3. In parallelo: ADR-0051 (proposto) copre il recupero del ticket bloccato e
   quindi anche il ripristino della capacità del ledger.
