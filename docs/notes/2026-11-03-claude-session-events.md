# Trasporto locale degli eventi di sessione Claude

Preparazione F02: il ricevitore autentica gli eventi con l'identità e il token
già generati per il processo. Il token resta nell'ambiente del figlio e
nell'header della richiesta locale; non viene scritto in argomenti, file o log.

Claude SessionStart richiede un hook di comando. Il nuovo `shinatama-hook`
riduce lo stdin a evento, UUID nativo e cartella, poi invia direttamente al
ricevitore su `127.0.0.1`. Non usa proxy, DNS o account. Stdout resta vuoto.
Riferimento: [documentazione ufficiale dei hook](https://code.claude.com/docs/en/hooks#sessionstart).

Il ricevitore conserva solo metadati autenticati per l'istanza e il progetto
previsti: header 4 KiB, corpo 16 KiB, lettura 2 secondi, coda 32 eventi, massimo
16 ricevitori. Una saturazione resta segnalata come perdita di osservazione.
La risposta 204 significa accodato, non ancora salvato nel database. SessionEnd
non costituisce una prova di arresto o cleanup del processo.

Verifiche eseguite: quattro test del bridge, incluso il collegamento con il
ricevitore reale, e un test mirato del ricevitore su autenticazione, richieste
malformate, saturazione e arresto. Tutti passati, senza lanciare Claude.

Il trasporto non è ancora installato nella ricetta Claude: il profilo corrente
continua a disabilitare i hook. Restano l'integrazione dello stato durevole,
la ricetta interattiva con hook e la prova del cambio/ripresa della conversazione.
