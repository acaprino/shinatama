# Servizi interni per harness — 3 novembre 2026

La richiesta architetturale dell'utente introduce un servizio Rust separato per
ciascun harness, con un'interfaccia comune e un registro. I servizi condividono
il flusso di avvio, senza copiare il controller per ogni agente. Sono componenti
interni al processo Shinatama: non introducono daemon, porte o processi aggiuntivi.
I subprocessi propri degli harness restano soggetti alla loro ricetta e a C09R.

Il flusso utente resta **progetto selezionato → New session → harness → Open**.
Preparazione e qualificazione restano interne; nessun nuovo Check obbligatorio.
I thread appartengono al progetto. Tab, pannelli e finestre restano una scelta
separata di disposizione, senza cambiare l'identità della conversazione.

## Confini e responsabilità

| Componente | Responsabilità |
| --- | --- |
| `HarnessDriver` | Ricette tipizzate: discovery, richiesta di storage, probe, preparazione di prova e sessione, classificazione degli errori. Mantiene distinti i tipi e le ricevute dei motori. |
| `Service<D>` | Unico flusso comune per osservazioni, ammissione delle risorse, prova isolata della TUI, creazione/ripresa del thread, avvio e mantenimento dei proprietari fino alla pulizia confermata. |
| `dyn HarnessService` e registro | Selezione per identificativo dell'harness, descrittore e capacità esplicite. Il controller Tauri mantiene la proprietà della richiesta e inoltra l'operazione al servizio scelto. |
| Moduli Claude, Codex e OpenCode | Adattamento dei motori esistenti, senza proprie copie di `check`, `open`, loop della prova TUI o stato `Facts`. Gli harness non implementati restano indisponibili. |
| Lifecycle opzionale | Hook Rust fidato per associare il processo esatto al thread prima dell'esecuzione e finalizzare solo dopo pulizia confermata. Assenza dell'hook non significa che resume sia supportato. |

Le prove e il proprietario della pulizia rimangono tipizzati: la serializzazione
serve solo alla diagnostica già filtrata. Nessun percorso di helper, argomento,
token, budget o prova di qualificazione diventa un input arbitrario della UI.
L'abbandono della richiesta UI non libera il proprietario mentre il lavoro
nativo è in corso. Un fallimento della pulizia conserva proprietà e prenotazioni;
il nuovo servizio non può avviare un sostituto sulla base di un timeout.

La separazione non cambia i profili dei motori. Codex conserva `--no-daemon`,
permessi nativi e limite della home. OpenCode conserva l'eseguibile diretto,
`--standalone`, database-file e quattro percorsi XDG privati, home degli strumenti
per istanza e plugin di ripristino dell'ambiente. I pin dell'eseguibile npm e il
controllo del numero osservato di hardlink restano nel motore. Le ricevute
restano legate a harness, versione, byte del programma e profilo esatto.

## Piano committabile e ripresa

Il nuovo **C11S** è un seguito di C11, non una riscrittura del suo risultato
storico. Dipende da C11, C09 e C09R; precede il completamento dei passi harness
C15–C20. C12 resta l'unico padre `in_progress`. C11S e le sue due unità sono ora `committed`, con i risultati delle prove riportati sotto.

1. **C11S.1 — Servizio, registro e driver.** Integrare insieme il servizio comune,
   il registro e i tre driver affinché il commit compili e conservi le API della
   UI. Eliminare i flussi duplicati solo dopo il collegamento dei chiamanti.
   Controllare i casi essenziali di proprietà, rifiuto e pulizia nel percorso
   comune; preservare le prove precedenti come storico, senza attribuirle al
   nuovo binario. Stima tecnica: 2–4 ore.
2. **C11S.2 — Regressione e checkpoint.** Verificare apertura nel progetto scelto,
   chiusura e rifiuti dal servizio comune sulle capacità effettivamente provate.
   Eseguire le prove native in sequenza, con dati privati e limiti congelati;
   registrare build, harness/versione, risultato e proprietari residui. I casi
   ancora non qualificati restano espliciti e appartengono ai relativi passi
   harness. Stima tecnica: 1–3 ore.

Ogni unità registra SHA reale, verifiche eseguite e relativo documento sotto
`progress.json → steps → C11S → units`. Non basta la presenza del codice per
promuovere un'unità. Alla ripresa leggere il checkpoint, ispezionare il diff del
checkout condiviso e completare la prima unità senza evidenza conclusiva; non
ricominciare discovery, build o prove già superate senza una modifica rilevante.
Finché C12 rimane attivo, i checkpoint C11S possono essere documentati senza
creare un secondo padre `in_progress`.

## Checkpoint delle prove sul nuovo servizio

- Sul codice integrato sono riuscite build frontend e backend con tutti i
  binari. Sono passati i due test mirati `agents::tests` e il test mirato
  `claude_session_events::tests` relativo al drenaggio degli eventi.
- Le nuove prove native di **Codex 0.166.0** e **OpenCode 2.2.5** sono entrambe
  superate. Ciascuna usa dati applicativi privati nuovi e progetti sintetici:
  New session, scelta progetto e harness, singolo Open, preparazione interna
  del servizio, TUI originale, persistenza di progetto/thread/vista, arresto
  della sola sessione posseduta e chiusura del frame. Le ricevute locali
  `.prototype-evidence/new-session-codex.json` e
  `.prototype-evidence/new-session-opencode.json` riportano `status: passed`
  e tutti e quattro i controlli riusciti. Sono file ignorati da Git; questa
  nota ne conserva i risultati. Nessun accesso account, risposta di login
  o prompt al provider è stato eseguito.
- Anche **Claude Code 2.1.317** supera ora la prova nativa di apertura e
  chiusura sul servizio comune. Il rifiuto del componente hook, dovuto ai due
  hardlink della build Cargo, è stato corretto; il relativo test mirato sul
  filesystem è passato. La ricevuta locale ignorata
  `.prototype-evidence/new-session-claude_code.json` aggiunge la conferma di
  un proprietario vivo per la conversazione fissata nel lancio e della sua
  finalizzazione dopo la pulizia nativa. Il profilo tracciato resta distinto
  dalla baseline sicura. Queste prove non dimostrano un evento hook realmente
  emesso da Claude, autenticazione, messaggi al provider o resume nativo.
- Le prove riuscite qualificano l'apertura nel perimetro descritto. Non provano
  autenticazione, strumenti, traffico/TLS provider, archivio OpenCode o resume
  Codex/OpenCode. Il problema dei percorsi OpenCode mancanti è corretto e
  l'apertura è descritta in [opencode-open](2026-11-03-opencode-open.md).
- Daodan resta rinviato per richiesta dell'utente. Nessuna dipendenza di questo
  incremento ne richiede installazione, accesso o integrazione.

C11S e le due unità sono completati nel perimetro architetturale verificato.
Il commit `454f75a4292343b77395fd8e720ede9ffa44869e` contiene servizio, registro, driver, UI e regressioni;
`7bc716f78332b1c0af41bdb89e3181251fe6b0bc` contiene i motori OpenCode e Claude tracciato.
C12 resta l'unico padre attivo. Conservare le prove di apertura, ripetendole
solo se modifiche successive coinvolgono quel percorso.

Anche la regressione PowerShell è passata:
`.prototype-evidence/new-session-powershell.json` conferma riuso del progetto
già selezionato da intestazione e barra laterale, thread di due progetti con
la stessa disposizione e risposta della shell sana. Questo verifica il flusso
e la separazione progetto/disposizione; non attribuisce a PowerShell capacità
di un harness agente.

Il completamento C11S riguarda la separazione dei servizi e le regressioni del
perimetro provato. Non chiude automaticamente C15–C20, i gate provider/TLS,
la resilienza completa, i set non vuoti o la gestione delle finestre flottanti.

Verifica del solo piano: `python -X utf8 scripts/implementation.py validate`
superato con **130 passi, 714/714 requisiti assegnati**, dipendenze acicliche e
checkpoint valido. `python -X utf8 scripts/implementation.py render` ha
rigenerato `PLAN.md`. Nessuna build o prova nativa è stata eseguita per questa
modifica documentale: gli esiti prodotto sopra riportati provengono dalle
esecuzioni del coordinatore sul nuovo servizio. Il piano viene rigenerato con il checkpoint dei commit reali.
