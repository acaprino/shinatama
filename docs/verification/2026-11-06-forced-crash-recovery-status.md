# Recupero dopo crash: prova minima (semplificata)

Data: 2026-11-06. Test: `apps/desktop/tests/forced-crash-recovery-smoke.mjs`
(versione minima: **una app crashata, un riavvio, un rifiuto, dati intatti**).

## Cosa prova (4 controlli, niente di più)

| Controllo | Cosa garantisce |
| --- | --- |
| `guardAndOwnerOnlyConfigPrepared` | Config privata owner-only e guardia esterna in piedi prima dell'app |
| `crashLeftActivationOpenAndTicketReserved` | Crash reale (kill) prima di ogni ricevuta → attivazione **aperta** e ticket ancora riservato |
| `restartRefusedRecoverWithoutReceiptAndDataIntact` | Riavvio: `Recover session` rifiuta ("shutdown was not confirmed") e non modifica revision/dati |
| `ownedFrameClosed` | Chiusura pulita dell'app |

## Cosa è stato eliminato e perché

La versione precedente aveva 5 fasi, 3 istanze app, doppi gate di memoria,
retry multipli, race sincrona sulla finestra della ricevuta, agent-kill e
Resume post-recovery: ridondante rispetto ai test già presenti
(`thread-recovery-smoke.mjs` copre il recupero con ricevuta e i rifiuti su
metadati sintetici; i test unitari dello stato coprono chiusura singola e
ricevute; le smoke di resume/race coprono la ripresa nativa).

## Limiti dichiarati

- Non prova il recupero **con** ricevuta né il Resume post-recovery: sono
  coperti altrove come sopra.
- Credenziali sintetiche, provider irraggiungibile, guardia solo-inoltro.
- Richiede ~4 GB di memoria fisica libera (attesa semplice, senza gate).

## Esecuzione

```text
verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/forced-crash-recovery-smoke.mjs
```

Esito in `.prototype-evidence/forced-crash-recovery.json`.
