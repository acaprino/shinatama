# Recupero del thread dopo pulizia nativa e crash dell'app

Questo incremento copre la finestra fra pulizia nativa completata e
finalizzazione del thread interrotta. Non sblocca un processo di cui non è stata
provata la pulizia e non completa l'intero requisito di recupero dopo crash.

## Comportamento

Prima di eseguire la shell sospesa, Shinatama registra l'associazione esatta fra
istanza, processo agente, host Windows, prenotazione e job. Il callback Rust
`before_run` riceve il ticket dal proprietario nativo; nessuno di questi campi è
accettato dall'interfaccia o dagli eventi dell'harness.

Quando la pulizia nativa è provata, la transazione che libera la prenotazione
salva anche la ricevuta. La chiusura dell'attivazione e il consumo della ricevuta
avvengono insieme in una seconda transazione. Un'interruzione fra le due lascia
quindi una prova durevole, non soltanto un ticket assente.

Per un thread registrato aperto senza terminale attivo in questa finestra,
**Recover session** tenta il recupero esplicito. Il backend richiede la ricevuta,
verifica le identità esatte del vecchio host e dell'agente e la sessione Windows,
quindi confronta nuovamente attivazione, thread e ricevuta nella transazione.
**Resume** rimane un'azione separata. Il recupero non avvia, segnala o termina
processi; un proprietario vivo, un'identità incerta o un dato cambiato lo impedisce.

## Persistenza e limiti

Il journal operativo usa lo schema SQLite esistente, senza migrazione: massimo
64 record e 128 KiB. L'ammissione riserva anche lo spazio necessario alla futura
ricevuta. I record finalizzati sono rimossi atomicamente; quelli ancora necessari
non scadono e non vengono eliminati per fare spazio. Raggiungere il limite
rinvia nuovi avvii. Il controllo di integrità riconosce il limite del journal.

I record precedenti, privi dell'associazione o della ricevuta, restano protetti.
Anche un crash prima della pulizia provata rimane fuori da questo incremento:
servirà un protocollo qualificato per ricostruire la terminazione dell'intero
gruppo. Nessuno sblocco per timeout, PID soltanto o assenza del ticket.

## Verifica

- `cargo test --offline --locked -p shinatama-state -j1 native_session_cleanup::tests`:
  **4/4 PASS**, 0,56 s di esecuzione. Database sintetici, riapertura nel punto di
  crash, recuperi concorrenti, host vivo, PID riutilizzato, sessione Windows
  diversa, ricevuta assente, rollback SQL e limite dei record.
- `cargo test --offline --locked -p shinatama-terminal -j1 lifecycle_`:
  **2/2 PASS**. Il ticket completo arriva al callback prima dell'esecuzione;
  errori e panic non permettono finalizzazione prima della pulizia nativa.
- `npm run build`: **PASS**. Build offline e locked dell'app e dei quattro
  eseguibili desktop/helper/hook: **PASS**, 1 min 48 s.

La prova UI completa di Claude e il pulsante di recupero restano distinti dal
recupero transazionale: non vengono qualificati dai soli test dello stato.
Il riavvio nel test simula la perdita della finalizzazione mediante chiusura e
riapertura del database; non è un arresto forzato del prodotto in esecuzione.
