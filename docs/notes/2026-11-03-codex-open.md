# Codex dal flusso New session

Incremento C16: selezione del progetto esistente, scelta Codex e un solo Open.
La UI non espone Check o preparazione del servizio. Il comando usa il terminale
originale e salva un thread del progetto prima dell'avvio. Il login rimane nella
TUI di Codex; Shinatama non legge né copia credenziali da altre installazioni.

Il catalogo locale seleziona il primo eseguibile Codex nell'ordine PATH. Un
wrapper precedente non viene saltato per avviare silenziosamente una diversa
installazione. File e identità sono fissati e verificati, poi versione e TUI
sono osservate con home temporanee private e proprietari di cleanup conservati.
La ricevuta della prova è legata all'harness, alla versione, ai byte del programma
e al profilo: una ricevuta Claude non abilita Codex o viceversa.

Il profilo usa `--no-daemon`, `--sandbox workspace-write`, approvazione
`on-request` e sandbox Windows `unelevated`. Il daemon privato non è richiesto
da questa ricetta; questo evita di applicare alle sue cartelle il limite più
stretto dei socket Unix. La home ha comunque un limite di 200 unità UTF-16.
Autenticazione su file, temporanei, configurazione e cache restano privati.
La CA è fornita solo al processo figlio, senza modificare Windows.

Limiti fissi: 1 GiB per job, 768 MiB per processo, 8 processi, CPU 10%, coda
4 MiB/512 elementi. La preparazione usa il medesimo controllo risorse C09R;
un rifiuto non aumenta i limiti e non avvia una shell sostitutiva.

La chiusura di un harness mostra New session con progetto e harness già scelti;
non propone più Check agent again. Questa azione crea una nuova sessione e non
è presentata come ripresa della conversazione terminata.

## Verifica e perimetro

Build frontend, desktop e helper Windows riuscite. Tre test mirati engine
(`codex_`) e il test adapter del profilo Codex sono passati. La prova nativa
`new-session-smoke.mjs`, con harness `codex`, è passata sulla versione osservata
**0.166.0**: un Open prepara il servizio, salva thread e vista, avvia la TUI e
chiude correttamente la sola istanza di prova. Ricevuta:
`.prototype-evidence/new-session-codex.json`.

Il primo tentativo aveva identificato le due junction dell'installer standalone
come cartelle non ammesse. Il resolver Codex ora ne segue al massimo otto, solo
su dischi locali fissi, conservando i pin dei genitori; il percorso fisico viene
nuovamente verificato dal probe e dal lancio. Il catalogo degli altri harness
non cambia.

Nessun prompt o login è stato inviato. Provider, strumenti effettivi, TLS del
provider, ripresa nativa e gestione installazioni restano fuori dalla prova.
C16 non è dichiarato completo da questo solo incremento di apertura.
