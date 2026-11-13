# Ricerca esplicita nella sessione

La ricerca è una proiezione del reader Mukade già fissato, eseguita nel medesimo
worker contenuto. La richiesta include il selettore dell'archivio, l'ID nativo
esatto e l'impronta della sessione già visualizzata. Non cerca altre cartelle e
non costruisce un indice persistente. Rimane il limite preventivo di 16 MiB
sul file della sessione letta (e 4.096 file nell'archivio); la dimensione totale
dell'archivio non conta. La paginazione riguarda i risultati, non la lettura
dell'input.

La query letterale è obbligatoria, senza caratteri di controllo e limitata a
128 byte UTF-8. Il confronto usa il lowercase Unicode semplice e occorrenze
non sovrapposte, senza espressioni regolari o normalizzazione degli accenti.
Sono ammessi testo canonico di utente e assistente, risultati tool testuali e
riassunti di compattazione. RAW, reasoning, argomenti tool, risultati strutturati,
metadati interni e contenuto degli allegati restano esclusi.

Ogni pagina contiene al massimo 50 risultati con estratti di 512 byte UTF-8;
la risposta resta entro 256 KiB. Si conta l'intera sessione per restituire il
totale, conservando soltanto la pagina richiesta. L'impronta invalida una ricerca
quando il contenuto sorgente cambia. Ogni risultato conserva sia l'indice della
entry canonica sia l'ordinale dell'evento visibile, derivato dal contatore condiviso
con la proiezione del dettaglio. Il salto apre un dettaglio in sola lettura;
non avvia o riprende un agente.

La finestra richiede un comando Search esplicito. Aprirla o digitare non legge
l'archivio. Mantiene una sola richiesta in corso, scarta risposte obsolete,
limita a 64 i cursori precedenti e cancella query ed estratti alla chiusura.
Gli estratti sono assegnati come testo, senza HTML, collegamenti o file allegati.

Questa consegna comprende sorgente e una prova Rust sintetica con il reader reale:
paginazione 50/3, salto coerente nel dettaglio, esclusione dei payload privati,
query oltre limite e rifiuto dopo modifica del sorgente. Esecuzione, collegamento
al controller/helper e verifica nell'app spettano all'integrazione. Non costituisce
completamento della ricerca globale FTS o dei gate del prodotto.

## Navigatore della pagina corrente

La libreria aggiunge il comando Projects / sessions accanto alla lista predefinita.
Raggruppa esclusivamente le righe della pagina già caricata, al massimo 50, usando
la stringa esatta della working directory restituita dal reader. Le righe senza
cartella registrata appartengono a Unknown project. Non risolve percorsi, non
attribuisce progetti canonici e non cerca altre cartelle. Il testo della vista
dichiara esplicitamente questo perimetro.

Ogni gruppo usa details/summary nativi. Espansione, chiusura, cambio vista e filtro
sono operazioni locali senza IPC. Solo la vista attiva contiene i pulsanti di
selezione delle sessioni; i metadati selezionati e Open detail restano gli stessi
della lista. Nuova pagina o nuovo archivio azzerano i gruppi aperti. L'azione di
selezione controlla l'identità immutabile della pagina e dell'archivio prima di
modificare la selezione. Nessun gruppo avvia o riprende un agente.

Per la prova mirata: pulsanti accessibili List e Projects / sessions,
contenitore #session-library-projects, gruppi details.session-project-group,
e pulsanti .session-project-session con attributo data-native-id. La prova
nell'app deve verificare espansione senza nuove letture e selezione dello stesso
ID nativo; questa modifica da sola non certifica un navigatore dell'intero archivio.

## Integrazione verificata il 3 novembre

La build frontend/Windows e i sette test Rust mirati `session_` sono passati.
La prova nativa `sessions-smoke.mjs` è passata su 53 archivi sintetici: pagine
catalogo e dettaglio 50/3/50, ricerca completa della conversazione 50/2 risultati,
salto all'evento 53, dati grezzi solo dopo richiesta esplicita, copia con clipboard
simulata e cancellazione del contenuto alla chiusura. Tutti i file sorgente sono
rimasti identici e nessun agente o terminale è stato avviato. Ricevuta:
`.prototype-evidence/sessions-library.json`.

Le finestre sovrapposte usano l'ordine reale dello stack modale, non l'ordine di
creazione degli elementi DOM: RAW e Find si aprono dal dettaglio e ritornano ad
esso. La prova conta le richieste effettive sul trasporto IPC; non tenta di
sostituire `invoke`, che Tauri rende immutabile.

Il catalogo completo non prova che il reader sappia leggere i legami fra sessioni.
I tre reader fissati non espongono parent nei cataloghi: la UI indica quindi
relazioni e conteggio dei figli sconosciuti. Non interpreta l'assenza del campo
come zero figli. La funzione pura per cataloghi con legami completi conserva le
prove di cicli, ambiguità, ID mancanti e limiti. Restano aperti indice globale,
esportazione, ripresa e parent metadata nativi non forniti dai reader.
