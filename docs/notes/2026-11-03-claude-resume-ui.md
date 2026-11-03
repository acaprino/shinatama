# Claude: prova UI della ripresa nello stesso thread

Stato: **parziale, non ancora qualificato**. La separazione dei servizi e la
rinomina sono implementate; questa prova non chiude C25/F02 o l'intero prodotto.

La prova manualmente eseguita `apps/desktop/tests/claude-resume-smoke.mjs` usa la
vera app Windows, ConPTY, un progetto nuovo e un endpoint HTTP sintetico su
127.0.0.1. L'ambiente importa soltanto campi Windows espliciti e una chiave
fittizia; la home Claude viene isolata dal motore del prodotto. Nessun account,
archivio o credenziale personale viene letto. Il progetto fittizio è vuoto.

## Risultati effettivamente osservati

Un'esecuzione sulla copia locale
`.verification/portable/e4fba2a6-64ee-43a3-9db8-206c90425131` ha attraversato:

1. **New session → Claude → Open**, consenso alla sola chiave sintetica e al
   progetto fittizio, primo messaggio e risposta sintetica.
2. **Stop**, finalizzazione dell'attivazione e persistenza del file nativo.
3. Selezione dello stesso thread e **Resume**: nuova istanza/processo, stesso
   thread, segmento, UUID nativo e cartella di lavoro.
4. Secondo messaggio: il provider locale ha ricevuto il precedente messaggio
   assistant nel contesto. Sono osservate esattamente una richiesta per turno,
   un unico thread e lo stesso archivio con entrambe le coppie domanda/risposta.
5. Secondo **Stop**, con entrambi gli intervalli di attivazione chiusi.

L'esecuzione si è poi fermata su un contatore del test: Tauri definisce `invoke`
come proprietà non modificabile, perciò assegnare un wrapper non intercettava
le chiamate. Il contatore è stato eliminato; la prova controlla gli effetti
persistiti e i terminali presenti, senza sostituire il canale IPC del prodotto.

Una ripetizione successiva ha osservato una chiusura inattesa prima del primo
messaggio. Non esistono trascrizione o log di crash da cui ricostruirne la causa.
Sono stati aggiunti al test codice di uscita e campioni limitati di
`terminal_resources`, da acquisire prima del cleanup. Non aumentare i limiti o
considerare questo arresto risolto senza osservarne la causa.

## Correzioni alla fixture

- Il profilo Windows dell'app deve restare coerente per WebView. Dati Shinatama,
  profilo WebView e archivio Claude restano in cartelle nuove della sola prova.
- Il consenso usa frecce e verifica l'opzione Yes visibile prima di Invio.
  Digitare il numero non selezionava la voce e aveva rifiutato la chiave fittizia.
- xterm 6 ignora `insertText` con screen reader attivo. La prova usa eventi
  tastiera e attende il testo sintetico visibile prima dell'invio.
- Le sole letture di metadati tollerano per tre secondi `busy`/`unavailable`:
  il salvataggio del layout usa `unavailable` anche durante una scrittura attiva.
  Apertura, Resume e mutazioni non vengono ripetuti automaticamente.

## Limite di rete e ripresa della verifica

Nei log della sola fixture sono state osservate connessioni del sidecar verso
`https://api.anthropic.com/v1/messages`, interrotte con `ECONNRESET`. Non ci sono
prove di autenticazione riuscita, fatturazione o presenza del prompt sintetico
in quelle richieste. Non è però corretto dichiarare che tutto il traffico fosse
locale. La prova precedente non aveva una guardia per le richieste di sfondo.

La revisione automatica ha quindi rifiutato un nuovo avvio non protetto: l'ambito
autorizzato della prova prevede il provider locale. Prima di rieseguirla occorre
una guardia upstream locale che accetti solo l'autorità esatta del provider
fittizio e rifiuti il resto senza DNS o inoltro remoto, configurata esplicitamente
nel file nuovo e privato del sidecar. Non cambiare i controlli del prodotto,
usare credenziali reali o aggirare il rifiuto. La verifica del percorso proxy
resta distinta da una garanzia di isolamento di rete a livello Windows.

La protezione è stata successivamente implementata e la nuova esecuzione
protetta è stata autorizzata dalla revisione automatica. `--guard-only` ha
superato il controllo delle due forme di richiesta rifiutata e della
configurazione privata, **senza avviare app o harness**. La creazione ACL usa
soltanto sezioni Owner e Access: chiedere anche le sezioni non necessarie
impediva la protezione del file. La fixture non modifica percorsi preesistenti.

La configurazione conserva la baseline e aggiunge `upstreamProxyMode=manual`
con la guardia numerica locale; non contiene override proxy per provider e non
eredita `BILI_UPSTREAM_PROXY`. La revisione dei sorgenti canonici conferma che
questo percorso copre il modello Anthropic osservato e i tunnel ausiliari; un
rifiuto non provoca fallback diretto. Il provider sintetico rifiuta anche
connessioni locali che non provengono dalla guardia.

## Esecuzione protetta precedente: uscita nativa incompleta

La prova ha rifiutato cinque CONNECT (uno è il controllo negativo iniziale) e
una richiesta assoluta negativa; nessuna connessione è stata inoltrata e nessun
messaggio sintetico è stato inviato. Claude è terminato con **codice 1** prima
dell'onboarding; il sidecar è rimasto `ready` e sano. Il supervisore registra
`phase=released`, `reason=process_ended`, non un proprio arresto per superamento
del budget. Ultimo campione: picco commit del job 320.512.000 byte, tre processi
attivi su sette avviati, zero rifiuti delle code; limiti invariati a 1 GiB/job,
768 MiB/processo e otto processi. Questi contatori non spiegano da soli l'errore
interno del CLI e non certificano tutta la sua stabilità.

La ricevuta di questa esecuzione riportava `status=failed`, fase
`first-onboarding`. Non promuovere C25.2/C25.3 o F02 sulla base della sola
esecuzione positiva parziale precedente. Alla ripresa, acquisire la categoria
di errore nativo nella fixture protetta prima della chiusura; non ripetere
aperture prive della guardia e non aumentare limiti senza una causa osservata.

La copia avviabile contiene i commit funzionali fino a `2b3d32b`. Il pacchetto
è verificato: 621 file, 227.392.932 byte. Non equivale a un rilascio approvato né
al completamento del piano. Le prove native headless di hook/Resume e TLS già
superate restano documentate separatamente in
[claude-native-resume](2026-11-03-claude-native-resume.md) e
[claude-proxy-tls](2026-11-03-claude-proxy-tls.md).

## Ripresa del 3 novembre: avvio rinviato per risorse

Due nuove esecuzioni protette si sono fermate prima di avviare il sidecar o
Claude. La seconda conserva la causa esatta, verificata contro una whitelist di
messaggi statici del prodotto:

> Resources are not yet ready for the local service. Wait and retry Start; no process was created.

Il messaggio viene da `Runtime::wait_for_admission()` nel percorso di preparazione
del sidecar. La ricevuta corrente `.prototype-evidence/claude-resume-ui.json`
riporta `status=failed`, fase `new-session`, `open-session-inline-refusal`,
`service.phase=failed` e categoria `resourceAdmission`. Entrambi i run hanno zero
richieste al provider e zero connessioni inoltrate dalla guardia; i soli rifiuti
sono i due controlli negativi iniziali. App della prova e guardia sono chiuse.

Un campione Windows **successivo**, distinto dalla ricevuta del test, rileva
32.983.768 KiB di memoria fisica totale e 10.335.804 KiB disponibili; capacità
commit di 38.569.004 KiB e 3.024.516 KiB disponibili, circa 2,88 GiB. La pressione
commit del campione è circa 92,2%, sopra la soglia d'ingresso del 90% del
governatore. È compatibile con il rinvio osservato, ma non sostituisce lo stato
del governatore durante il run e non identifica la causa dell'uscita Claude
precedente. Nessun limite è stato aumentato e nessuna applicazione personale è
stata arrestata.

La fixture ora raccoglie famiglie d'errore native, codici di rete enumerati,
messaggi statici del prodotto in whitelist e contatori numerici limitati di
`resources_status` prima della chiusura. Non salva testo del terminale, percorsi,
credenziali o corpi delle richieste. Un `exit-banner` prima della disponibilità
del prompt interrompe subito l'attesa. Il controllo sintattico Node passa;
la nuova cattura dei contatori host non è stata ancora esercitata in un run.

La prossima prova richiede risorse sufficienti e deve mantenere la guardia:
acquisire la famiglia dell'errore Claude se ricompare, oppure completare
New session → Stop → Resume nello stesso thread. Il rinvio corrente non dimostra
che l'uscita nativa precedente sia risolta.
