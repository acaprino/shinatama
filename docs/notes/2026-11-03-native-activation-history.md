# Storico delle attivazioni e cambi di conversazione

Incremento F02/C25: lo schema 5 conserva una sequenza distinta per ciascuna
attivazione. Un processo può passare da una conversazione A a B e tornare ad A
senza inventare una nuova identità di processo né perdere gli intervalli precedenti.
Gli indici impediscono due aperture contemporanee della stessa istanza, dello
stesso thread, segmento o identificatore nativo nella stessa radice.

La migrazione 4→5 conserva gli undici campi delle attivazioni esistenti, con
backup verificato prima della transazione. La validazione dello schema 4 resta
disponibile per controllare ingresso e backup. Non è stato aperto o aggiornato
il database personale dell'utente.

`observe_native_session_start` applica nella stessa transazione la chiusura
dell'intervallo precedente, il segmento corretto e la nuova attivazione. Richiede
istanza, PID e creazione del processo esatti, radice e revisioni correnti.
Un evento ripetuto è idempotente; richieste obsolete o di un altro proprietario
sono rifiutate. Le attivazioni terminate non vengono riaperte implicitamente.

La destinazione Resume e la sidebar seguono l'ultima attivazione osservata,
incluso il ritorno A→B→A. La chiusura esatta usa sequenza e revisione: un comando
vecchio non può chiudere la nuova conversazione dello stesso processo.

Verifiche passate: test di migrazione con conservazione e rollback; tre test
thread, compresi concorrenza con due connessioni, cambio A→B→A, identità di
processo riusata, radici diverse, riapertura del database e cursori obsoleti.

Restano da collegare il controller nativo e le azioni Resume della UI. Il cambio
di schema e i test dello stato non costituiscono una prova di ripresa in Claude.
