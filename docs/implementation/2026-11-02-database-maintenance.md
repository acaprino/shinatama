# Manutenzione finita dello stato locale

Data: 2026-11-02. Richiesta utente vincolante; valori tecnici proposti, da misurare.

L'utente richiede «job pulizia/manutenzione db per non accumulare sporcizie, dati
malformati, crescita infinita». Il perimetro comprende soltanto `shinatama.db` e
cache, log e artefatti creati da Shinatama. Gli archivi originali degli agenti e
il database BillionContext sono esclusi: il sidecar mantiene la propria singola
fonte e le verifiche C12/C13. Nessuna ricerca o pulizia generica di file SQLite.

## Conservazione e limiti

La storia di lavoro autorevole resta conservata senza scadenza predefinita.
Progetti, thread, messaggi, handoff, cronologia delle azioni e dati del companion
non vengono eliminati, troncati o riscritti da un job automatico. Il vincolo vale
anche per `message_event`, classificato autorevole da FRD-0014 R13: la policy di
R24 non autorizza a cancellarlo. `forget` e un'eventuale archiviazione restano
azioni esplicite e verificabili dell'utente.

Questi sono tetti iniziali proposti, non prestazioni o scelte già qualificate:

| Classe | Conservazione proposta | Tetto proposto | Quando il tetto è raggiunto |
|---|---|---|---|
| Stato autorevole | Senza scadenza | Avviso a 1 GiB, ammissione protetta a 4 GiB | Rapporto per classe e azione esplicita; rinviare operazioni che richiedono nuova persistenza, senza cancellare storia o fingere che una scrittura sia riuscita |
| `event`, `summary_log` operativi | 30 giorni, solo record conclusi e non necessari a consumatori attivi | Rispettivamente 100.000/64 MiB e 10.000/16 MiB | Potatura per batch con cursore e indice verificati; un consumatore in ritardo riceve uno stato esplicito di recupero |
| Cache e log Shinatama sacrificabili | 7 giorni | 256 MiB cache; 32 MiB log | Scarto delle sole voci dichiarate sacrificabili, senza transcript o credenziali nei log |
| Code di retry e quarantena operativa | Retry conclusi 7 giorni; anomalie aggregate 30 giorni | 1.024/4 MiB retry; 128/64 KiB anomalie senza contenuto | Rinviare nuovi tentativi o aggregare contatori; nessuna perdita silenziosa di richieste pendenti e nessuna copia del testo sensibile |
| WAL e backup | Checkpoint opportunistico; backup senza cancellazione automatica in questa fase | WAL: soglia di ammissione 32 MiB; segnale alto 128 MiB. Backup: 8 file, 2 GiB complessivi, almeno 1 GiB libero | Rinviare checkpoint onerosi, segnalare lettori che trattengono WAL, bloccare nuovi backup oltre budget preservando gli esistenti |

Ogni feature che introduce una tabella operativa, una cache o una coda deve
registrare proprietario, schema/versione, indice di selezione, stato cancellabile,
retention, conteggio e budget in byte prima dell'abilitazione. Non si aggiungono
tabelle vuote per feature future e non si eseguono `DELETE` su nomi scoperti per
somiglianza. Con FS03 lo schema corrente comprende `project`, `setting`,
`store_root` e `workspace`, tutti autorevoli: i job non ne cancellano righe.
Le radici rimosse diventano non disponibili; revisioni e riferimenti storici
restano conservati. I limiti operativi futuri diventano verifiche
del passo che introduce il componente, oltre alle prove C03M.

Le nuove scritture FS03 passano dall'ammissione globale esistente prima della
transazione e ricontrollano la versione sotto `BEGIN IMMEDIATE`. Layout: 256 KiB
per documento, versione supportata e revisione/proprietario verificati; letture
radici: pagine di massimo 100 record con campi limitati prima della copia.
Non si riempiono tabelle operative future per provare la classificazione e non
si ricostruisce o elimina una tabella autorevole durante la manutenzione.

Per i backup si propone una futura selezione di candidati all'archiviazione dopo
30 giorni, mantenendo almeno tre copie verificate e la copia prima dell'ultima
migrazione valida. Una copia necessaria per ripristino resta protetta anche se
più vecchia. C03M inizialmente presenta soltanto inventario e candidati; non
elimina backup o file incompleti. Resta applicato il rifiuto preventivo C03 oltre
8 file o 2 GiB. Non è una scadenza della storia di lavoro.

## Esecuzione e proprietà

Il runtime della finestra programma tick solo quando inattivo, fuori dal thread
dell'interfaccia. Un lock di file privato rilasciato dal sistema dopo un crash
impedisce la sovrapposizione anche se un'operazione del filesystem supera la
scadenza della lease. Una lease atomica nel `setting` esistente registra token
casuale del proprietario, identità di processo e scadenza; il PID da solo non
autorizza la sostituzione del proprietario. Un'interruzione non lascia una lease
senza scadenza e non avvia retry infiniti. Nessun servizio residente.

Il profilo iniziale limita un tick a 128 righe e 50.000 istruzioni della macchina
virtuale SQLite, con scadenza cooperativa di 100 ms. Il progress handler interrompe
solo il lavoro SQLite del tick. `SQLITE_INTERRUPT` per scadenza o pressione è
`DeferredBudget`, non corruzione e non un'anomalia da mettere in quarantena.
Non si riavvia lo stesso batch in un ciclo automatico senza un nuovo tick ammesso.
Chiamate OS, attese di storage e flush non hanno una garanzia rigida di 100 ms:
si misurano separatamente e non si eseguono sulla UI né mantenendo transazioni
di scrittura aperte durante l'I/O del filesystem. Nel nucleo implementato i 100 ms
misurano la fase SQL, finalizzazione e rilascio della lease, dopo l'ammissione
filesystem misurata separatamente. Una soglia morbida di 500 ms rinvia il lavoro
SQL se il preflight lento è già rientrato; non può interrompere una chiamata OS
bloccata. Il tempo totale osservato non viene presentato come garantito entro
100 ms. Il contatore VM resta unico e monotono attraverso tutte le fasi SQL.

Il checkpoint automatico usa soltanto `PASSIVE` dopo l'ammissione per dimensione
WAL e pressione. Un checkpoint non offre un numero massimo di pagine copiabili:
si registra durata e risultato reale, senza promettere un limite rigido di I/O.
WAL troppo grande o lettori attivi producono rinvio e diagnostica, non
`TRUNCATE`/`RESTART` forzati. `PRAGMA optimize` ha un limite di analisi; la
compattazione incrementale è ammessa soltanto se `auto_vacuum=INCREMENTAL` è già
attivo e ha un budget di pagine. Mai `VACUUM` completo automatico, mai conversione
silenziosa del formato o aumento dei limiti per far passare una prova.

`quick_check` periodico è distinto dall'`integrity_check` completo richiesto
esplicitamente da doctor. La verifica cooperativa può restare incompleta; quel
risultato non certifica integrità e non viene mostrato come corruzione. Le righe
malformate ricevono soltanto identificatore, categoria, conteggio e riferimento
locale in un rapporto operativo limitato; il contenuto autorevole rimane intatto.
Un'anomalia ripetuta aggiorna il conteggio, non accumula copie o nuove righe senza
limite. Riparazioni, ripristini e rimozioni richiedono un'azione esplicita.

Uno schema più recente apre in sola lettura e non riceve job o lease nuove.
C09R può negare l'avvio o fermare cooperativamente la manutenzione opzionale;
il ritorno a pressione normale richiede un nuovo tick, senza ripetizione
automatica dell'ultimo batch. Il report non espone SQL generico alla UI.

## Passi e prove

1. C03M.1, 6–10 ore: policy, rapporto tipizzato, nucleo finito per lo schema attuale,
   prove di crescita, anomalie e conservazione dei record autorevoli.
2. C03M.2, 6–10 ore: lease e coordinamento fra finestre, scheduling inattivo,
   integrazione con C09R, checkpoint/ottimizzazione/compattazione ammessi.
3. C03M.3, 6–10 ore: crash del proprietario, disco pieno/lock, budget e pressione,
   recuperi ripetuti, terminale sano responsivo e limiti di crescita residua.

Stima complessiva 18–30 ore, da aggiornare dopo la misura locale. Il core isolato
non chiude l'integrazione o le prove di resilienza. C03M dipende da C03 e F32CI;
C09R ne dipende, mentre C04 può procedere indipendentemente dopo C03.

Le prove usano soltanto stato sintetico in cartelle private e sotto limiti sicuri.
Devono verificare byte/righe prima e dopo i job, lease concorrenti, scadenza e crash,
interruzioni che lasciano il database integro, ripetizioni che non fanno crescere
quarantena o metadata, schema futuro invariato e latenza di un vero terminale sano
durante la manutenzione. La qualifica con terminale è un'evidenza separata dai test
SQLite; nessun gate viene dichiarato superato da questa proposta.

## Fonti tecniche

Le capacità dei comandi provengono dalla documentazione ufficiale SQLite di
[checkpoint PASSIVE](https://www.sqlite.org/pragma.html#pragma_wal_checkpoint),
[optimize e limiti di analisi](https://www.sqlite.org/pragma.html#pragma_optimize),
[vacuum incrementale](https://www.sqlite.org/pragma.html#pragma_incremental_vacuum)
e [progress handler cooperativo](https://www.sqlite.org/c3ref/progress_handler.html).
Sono fonti tecniche, non risultati delle prove locali.

## Registro artefatti C05

La creazione progetti possiede un file di coordinamento vuoto
`.shinatama-create.lock` per radice usata e preparazioni
`.shinatama-create-<uuid>.tmp`. Il file di lock non viene eliminato automaticamente:
rimuoverlo mentre altre finestre lo usano potrebbe creare due lock indipendenti.
Le preparazioni sono lavoro recuperabile, separato dalla cache eliminabile e
mai copiato in SQLite/quarantena. Il proprietario è `crates/projects::create`;
il namespace corrente è versione 1. Il commit è il rename atomico senza
sovrascrittura; prima del permit si può annullare, dopo non si promette undo.

L'ammissione rifiuta nuove creazioni con otto preparazioni conservate e interrompe
l'inventario dopo 4.000 entry della radice. Una directory non vuota non scade e
non viene potata per byte: contiene lavoro da conservare e richiede revisione
esplicita dell'utente. Solo la directory vuota della propria operazione può
essere eliminata tramite handle e identità verificata. Il supervisore Git ha
limiti fissi separati: un lancio, due processi, 128 MiB per processo / 256 MiB
per job, cinque secondi CPU user-mode per job / wall cooperativo e 32 KiB di
output scartato dopo il conteggio.
Non esiste una quota rigida in byte sulle scritture native di Git; il profilo
qualificato esegue solo `init`, con template vuoto e senza rete/config esterni.
La manutenzione DB non scandisce né cancella questi file nei progetti. C09R e
le prove storage restano responsabili della qualifica globale delle risorse.
