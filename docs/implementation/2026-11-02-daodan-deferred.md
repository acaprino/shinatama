# Daodan rinviato su richiesta dell'utente

Richiesta del 2026-11-02: "ignora per adesso l'integrazione con daodan".
Questo e un rinvio di scope, non la rimozione dei requisiti storici o una loro
promozione a implementati.

## Resta rinviato

- D01: installazione dei pacchetti/marketplace Daodan.
- C34/C35: agenda, task, onboarding e trasferimento di task negli hop.
- D02: bridge Daodan moe/peer-review per partecipanti bare-model.
- D03: avvisi task Daodan nel widget.
- D04: task/fase nel companion, thread_task, signals tramite task_router.py,
  misure specifiche della memoria Daodan e altri riusi delle sue componenti.

## Prosegue indipendentemente

Checkup e set generici rimangono in C14; non installano Daodan e la sua assenza
non causa Degraded o rifiuto di avvio. Le prove QSET misurano i componenti
effettivamente abilitati. I worktree generici e la roundtable tramite harness
restano attivi; nessuna reimplementazione sostitutiva dei servizi Daodan e aggiunta.

Gli archi verso C34/C35 sono rimossi da widget, worktree e accettazione del ciclo.
Le clausole miste sono annotate nelle schede interessate. I requisiti restano
assegnati, ma lo scope attivo e il candidato di rilascio dichiarano l'esclusione.

## Ripresa e verifiche

Il checkpoint registra sei passi `deferred`. La selezione automatica non li
propone; anche un `allow_non_v1` non aggira il rinvio. Per riprenderli serve
una nuova istruzione dell'utente, poi si rivalutano versioni e dipendenze.

Verifiche: `python scripts/implementation.py validate`, suite dei checkpoint
e rigenerazione del piano. Nessuna modifica al comportamento dell'eseguibile,
che non integra ancora Daodan. Il prossimo passo attivo resta F32CI.
