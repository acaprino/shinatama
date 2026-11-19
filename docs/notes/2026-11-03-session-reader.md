# C22 — lettore sessioni: dipendenza, confini e prove pendenti

Stato: **2 test inline PASS in 0,21 s; helper nativo PASS 1/1 in 0,24 s; UI
PENDING**. Questa slice non chiude C22 e non qualifica importazione, modifica,
esportazione o ripresa delle sessioni.

## Provenienza effettiva

La dipendenza opzionale `mukade` usa il repository
`https://github.com/acaprino/mukade` alla revisione completa
`4124b1f880f49a8f958801195324628752e35c4a`. Il fetch Cargo è riuscito e
`Cargo.lock` registra questa esatta sorgente Git/revisione; non esiste un
`path` dependency verso la copia di revisione. La feature è `session-reader`.

`scripts/mukade-pin.json` descrive 117 file sorgente, tree Git
`759c2426a57dfd0491f60678e9788a84768900bb` e fingerprint aggregato
`a34dccf85a0a3ef3a028ad4e252fdf861562c5494f7ed309e29945910c4a160b`.
Lo snapshot `.verification/mukade-pin-4124b1f` serve alla revisione/preparazione:
il prodotto usa codice Rust compilato e **non legge un checkout a runtime**.
`metadataWorkerQualification` è `contained-synthetic-metadata-passed`: il
helper confinato ha superato la prova sintetica metadata, senza qualifica UI.

## Contratto della slice metadata

1. `PreparedSessionList::resolve` accetta `RootSelector {storeRootId,
   configurationRevision}` e risolve il record SQLite esatto, disponibile e con
   revisione corrente. Nessun percorso arbitrario da UI, selettore ambientale o
   fallback alla home. Le radici Box richiedono anche verifica C01 privata.
2. Il solo helper `shinatama-session-helper` chiama il vero
   `mukade::reader::Reader.list/list_all`; nessun parser alternativo o comando CLI.
   Richiesta massima **8 KiB**, pagina massima **50 righe**, risposta/output
   complessivi **256 KiB**, budget temporale del worker **10 secondi**.
3. `RootSelector` e fingerprint dei metadati accompagnano la pagina. Un cursore
   non consente di riutilizzare pagine dopo un cambio di sorgente; il parent
   ricontrolla record e revisione prima di accettare la risposta.
4. Profilo esplicito: job 1 GiB, processo 768 MiB, 8 processi, CPU 10%, code
   1 MiB/4 elementi. La chiamata usa l'ammissione condivisa; solo cleanup nativa
   confermata e crediti restituiti consentono il rilascio della prenotazione.
   Timeout, perdita del helper e cleanup incerta non provocano replay automatico.
5. Campi non disponibili dal reader restano sconosciuti (`None`), compresi date,
   turni o byte non realmente forniti. Nessun titolo/data inventato da scansioni
   aggiuntive. Metadati fuori limite sono omessi/segnalati o rifiutati; transcript,
   export e percorsi dei file sorgente non entrano nella risposta metadata.

## Guardia condivisa del namespace

`pin_reader_namespace(record)` restituisce `SessionNamespaceGuard`, mantenuta
viva attraverso il reader. Gli handle verificano identità e attributi senza
leggere i payload; l'accesso include READ_DATA/LIST_DIRECTORY necessario alla
garanzia di sharing Windows. Consentono condivisione **READ/WRITE**, negano **DELETE** e
quindi impediscono sostituzione/rinomina delle voci già aperte. Reparse point
e file con hardlink multipli sono rifiutati. Sono pinnati root/antenati e solo:

| Harness | Namespace esplicito |
| --- | --- |
| Claude Code | `projects` |
| Codex | `sessions`, `archived_sessions`, `session_index.jsonl` opzionale |
| Pi | `sessions` |
| OpenCode | Shape attuale non supportata; nessun fallback CLI/export |

Walk massimo: **4.096 voci**, **1.024 directory**, profondità **64**. Nessuna
scansione di file credenziali/configurazioni fratelli. `verify_current()`
confronta identità, dimensioni/mtime dei file e namespace prima/dopo la lettura.
`file_size(path)` accetta soltanto file esattamente già pinnati;
`total_input_bytes()` e `file_count()` offrono limiti metadata-only conservativi
per il dettaglio, prima di chiamare il reader.

**Lacuna concorrente esplicita:** negare DELETE agli handle esistenti non vieta
la creazione di nuove voci in una directory. I controlli prima/dopo rilevano
modifiche ordinarie, ma non isolano da un writer ostile che inserisca un nuovo
reparse durante la chiamata al reader. Servono un reader basato su handle o uno
snapshot privato per eliminare questa race. La slice non dichiara questa garanzia.

## Ripresa delle verifiche

I **2 test inline sono PASS in 0,21 s**, su filesystem Windows sintetico: reader
effettivo con radici distinte e sorgenti invariate; guardia che consente append,
nega rename e rifiuta hardlink/reparse e modifiche del namespace. La prova del
reader passa attraverso il seam unit su dati minimi, non attraverso il worker
nativo di prodotto. La verifica ha corretto l'accesso attribute-only, che non
impediva la rename: il test passa con READ_DATA/LIST_DIRECTORY incluso, senza
letture dei payload nel walk.

La prova reale `tests/session_reader.rs` del helper confinato è **PASS 1/1 in
0,24 s** (compilazione 10,27 s). Due radici System ReadOnly sintetiche con lo
stesso native ID restano distinte; campi sconosciuti sono `None`, hash e byte
dei sorgenti restano invariati, revisioni stale/radici mancanti sono rifiutate.
Il ledger contiene **zero prenotazioni dopo ogni helper**. Il pin registra
questo specifico risultato, senza estenderlo ad altri harness o all'intera C22.
La prova UI nativa è **PASS**: archivio esplicito in sola lettura, 53 sessioni
sintetiche nelle pagine 50/3/50, filtro di pagina, selezione e campi Unknown,
deduplicazione della radice, rifiuto dei riferimenti mancanti/superati e tutti
i byte originali invariati anche dopo la chiusura. Non viene avviato un agente.
Log: `.verification/c22-sessions-live.log`; ricevuta e immagine:
`.prototype-evidence/sessions-library.json` e `sessions-library.png`.
La build frontend e quella desktop/helper sono passate. Non è la qualifica
completa dei reader di ogni harness o del requisito padre C22.

Hermes e DeepSeek restano **non supportati** da questa integrazione. OpenCode
resta **unsupported root shape**, senza tentativi CLI o letture export. Nessuno
di questi stati autorizza scoperte ambientali, query di autenticazione o letture
di credenziali. I risultati futuri devono distinguere backend nativo, UI e
completezza del requisito C22.
