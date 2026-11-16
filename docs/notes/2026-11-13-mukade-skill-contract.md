# Skill per sessioni comunicanti

La richiesta e': dalla sessione corrente poter dire «apri un'altra sessione e falle implementare il piano», continuando a supervisionarla attraverso Mukade. **Claude e' il predefinito; una richiesta esplicita di Codex seleziona Codex.** Il modello e' una scelta distinta dall'harness.

Questo documento completa lo [studio di UI, codice e logica](2026-11-13-mukade-collaboration-study.md). La skill di progetto e' in `.claude/skills/mukade-collaborate/`. Il suo formato segue la [documentazione ufficiale di Claude Code](https://code.claude.com/docs/en/skills).

## Comportamento richiesto

| Richiesta | Selezione | Modalita' |
|---|---|---|
| «Apri un'altra sessione e falle implementare questo piano» | `claude_code` | Delega interattiva |
| «Apri Codex e fagli implementare il piano» | `codex` | Delega interattiva |
| «Apri Claude per discutere le alternative» | `claude_code` | Dialogo, senza modifiche al codice |
| «Fai rivedere il diff a Codex» | `codex` | Revisione, senza implementazione implicita |
| «Osserva quella sessione» | Sessione esistente identificata esattamente | Osservazione ridotta |
| «Se gli dico apri Codex, lo fa?» | Nessun avvio | Spiegazione del comportamento |

Un harness esplicito non disponibile produce un blocco spiegato; non viene sostituito automaticamente. Un nome di modello o profilo come Qwen o Fable non diventa automaticamente un eseguibile.

## Tre livelli distinti

1. **Skill:** interpreta l'intento, prepara il contesto e segue il protocollo.
2. **Mukade:** conserva stanza, membri, messaggi, ricevute e associazioni alle sessioni native.
3. **Host:** apre e controlla il processo reale con cwd, identita', permessi e terminale corretti.

Il codice esaminato espone i tool MCP `chat_*` per il secondo livello. `chat_join` registra un membro; non apre un processo. La CLI `mukade chat launch` apre un harness e resta bloccante nel suo terminale fino all'uscita. Non crea da sola una scheda Shinatama.

## Flusso della delega

La sessione iniziale seleziona harness e modalita', verifica MCP e launcher, prepara un incarico con obiettivo, piano autorizzato, directory, ambito e criteri di accettazione. Un avvio reale riceve un bootstrap breve con stanza, nome del coordinatore e identificativo univoco dell'incarico.

Il nuovo partecipante usa il ruolo Mukade **peer**: puo' chiedere chiarimenti, comunicare progressi e inviare il resoconto. Il ruolo `worker` e' piu' restrittivo: non puo' mandare messaggi liberi e risponde soltanto con `done` o `blocked` a un'istruzione indirizzata da un `lead`. Non e' quindi il predefinito per la delega interattiva.

Prima dell'assegnazione, il coordinatore richiede un messaggio READY correlato all'avvio e verifica l'identita' del partecipante. La sola presenza in `chat_members` non attesta che un processo sia vivo. Il messaggio di ritorno contiene riferimenti ai file, verifiche eseguite, esiti e blocchi. La verifica del risultato resta distinta dalla ricezione del messaggio.

Gli hook Mukade gestiscono gia' lettura e conferma dei messaggi per le sessioni avviate con il relativo adapter. La skill non deve creare un secondo consumatore con `chat_read`/`chat_wait` sulla stessa membership. In modalita' manuale, invece, legge, elabora e conferma una sola volta il batch. Le attese sono limitate e la scadenza non provoca un secondo avvio automatico.

## Collegamento implementato

Il ponte e' implementato nel codice di Shinatama per le nuove sessioni native Claude e Codex. Non usa il vecchio registro del comando `shinatama mcp`: estende il listener autenticato delle sessioni native, con la stessa identita' e revoca alla fine del processo.

- `shinatama_session_open` accoda una richiesta con UUID stabile. Il valore predefinito dell'harness e' `claude_code`; `codex` resta Codex.
- `shinatama_session_status` restituisce soltanto le richieste del chiamante: `queued`, `starting`, `opened`, `failed` o `cancelled`.
- Il worker desktop riusa preparazione, qualificazione, budget, identita', lifecycle e terminale dei launcher esistenti. La directory e' quella della sessione madre, non un percorso arbitrario fornito dall'agente.
- La UI riceve l'evento e aggiunge una vera scheda. La riconsegna dello stesso evento non duplica il terminale; al reload i processi restano recuperabili attraverso `terminal_list`.
- Il bootstrap e' un unico argomento letterale, dopo `--`, inserito prima del sigillo del launcher. Non e' uno script shell. Il contesto breve arriva alla nuova sessione senza copiarne credenziali o identita'.

Gli argomenti accettati sono `request_id`, `instruction` (max4096 byte UTF-8), `harness`, `mode` (`delegation`, `dialogue`, `review`), e la coppia opzionale `room_id`/`coordinator`. Per una sessione comunicante la skill passa entrambe le informazioni di stanza. Non sono accettati argv, ambiente, credenziali o directory arbitrarie.

Il risultato `opened` prova il lancio del terminale, non READY, consegna o completamento del lavoro. La skill continua la verifica del messaggio READY e poi indirizza l'incarico al nuovo membro. `result.session_id` e' l'istanza Shinatama: non va confusa con l'ID nativo dell'harness usato da `chat_bind`.

La deduplicazione vale per istanza chiamante e UUID. Stesso UUID con argomenti diversi viene rifiutato. Le ricevute restano per tutta l'esecuzione dell'app, con limiti di quattro richieste per chiamante e 256 totali; non vengono eliminate per fare spazio a retry ambiguamente nuovi. Il riavvio dell'app non riproduce gli avvii. Se il padre termina prima della presa in carico, il lavoro viene cancellato. Un lancio gia' iniziato conserva il suo risultato reale anche se il padre termina nel frattempo.

## Sessioni dello stesso thread

Aggiunto il 16 novembre 2026. Nell'interfaccia un thread e' un insieme di sessioni (ADR-0055, record `workset`). Le sessioni di uno stesso thread comunicano attraverso una stanza Mukade del thread:

- `shinatama_thread_peers`, sullo stesso listener autenticato, senza argomenti e in sola lettura, restituisce il thread del chiamante, il nome della stanza, il nome membro del chiamante e le altre sessioni con harness, stato e nome membro. Il chiamante (istanza, sessione salvata, harness) e' fissato dal ricevitore che l'ha lanciato; nessun argomento puo' indicare un'altra sessione.
- Nomi deterministici: stanza `shinatama-thread-<suffisso del workset>`, membro `<harness>-<suffisso della sessione>` (`claude-…`, `codex-…`). `chat_open` di Mukade restituisce la stanza esistente per un nome gia' noto, quindi tutte le sessioni del thread arrivano alla stessa stanza senza un registro in Shinatama. Il nome membro resta lo stesso dopo un resume: se e' gia' usato, la sessione rientra con `chat_rejoin`.
- Una sessione aperta con `shinatama_session_open` viene spostata nel thread della sessione che l'ha chiesta prima che la finestra la mostri; il bootstrap nomina la stanza del thread. Se lo spostamento viene rifiutato, la sessione resta nel proprio thread e funziona comunque; nessun secondo avvio.
- Le istruzioni MCP di inizializzazione dicono al modello che la sessione appartiene a un thread e indicano il tool, cosi' una richiesta dell'utente come «chiedi a Codex nel thread» trova la strada senza la skill.
- Solo Claude Code e Codex ricevono gli strumenti Mukade da Shinatama: OpenCode e Pi compaiono fra i peer con `chat: "none"`.

Senza l'interruttore del thread (sotto) il consumo resta manuale: una sessione inattiva non si risveglia da sola quando riceve un messaggio; lo legge quando chiama `chat_read`/`chat_wait`. Il tool lo dice al modello.

### Thread che parlano: hook Mukade e interfaccia (ADR-0057)

Aggiunto il 16 novembre 2026 su richiesta dell'utente.

- **Interruttore.** Nelle impostazioni del thread (⋯) «Sessions talk to each other». Spento di default; salvato in `threads.chat_enabled.v1`. Accenderlo apre subito la stanza, cosi' un problema si vede subito; non cambia le sessioni gia' aperte.
- **Hook di Mukade, per lancio.** Le sessioni Claude Code e Codex create o riprese in un thread acceso ricevono gli hook dell'adapter Mukade (`mukade chat hook`) accanto all'hook di tracciamento di Shinatama, e la stanza nell'ambiente (`MUKADE_CHAT_ROOM_ID`, ruolo `peer`, ascolto 300 s, massimo 10 risvegli, `MUKADE_DATA_DIR`). L'adapter entra nella stanza al primo turno, consegna al modello cio' che gli altri hanno scritto a ogni turno e, dopo un turno, attende un messaggio indirizzato e riprende la sessione. Claude: tre voci aggiunte al `settings.json` dell'istanza. Codex: prompt e stop, con lo stesso schema di fiducia dell'hook di Shinatama (chiave di stato e hash di identita'), in un'unica tabella `hooks={…}` per restare nel limite di 32 argomenti. Nessuna configurazione globale, nessun trust aggirato.
- **Sessioni nel loro thread.** `agent_open` riceve il thread, quindi il lancio lo conosce; una sessione aperta da un agente nasce nel thread del chiamante.
- **Pannello «Conversation».** Il pulsante «Chat» sulla riga del thread apre un pannello laterale non modale: sessioni con stato di collegamento («in the room», «not in the room yet», «cannot take part») e pulsante per mostrarne la scheda; messaggi con mittente e destinatari, filtro; un campo per scrivere a una sessione o a tutta la stanza come membro `user`. Il pannello legge con `rooms`, `members`, `ledger` attraverso il programma Mukade del pacchetto e non consuma mai i batch dei membri.
- **Segni.** Un punto sul pulsante «Chat» quando la stanza ha eventi non visti; « · in chat» o « · not in chat yet» sulle schede Claude e Codex del thread. Una sola chiamata `rooms` ogni 10 secondi.
- **`shinatama_thread_peers`** con gli hook attivi riporta i nomi dati dall'adapter (`claude-1`, `codex-1`) e dice al modello di non rifare `chat_join` ne' leggere a mano; il bootstrap delle sessioni aperte da un agente fa lo stesso quando la stanza indicata e' quella del thread.

Limiti: dopo ogni turno una sessione in ascolto mostra l'hook di stop in esecuzione per un massimo di 5 minuti (scrivere o Esc lo interrompe); le sessioni gia' aperte entrano solo quando riprese; OpenCode e Pi non partecipano. Lo schema di fiducia degli hook Mukade in Codex (chiavi `user_prompt_submit:0:0` e `stop:0:0`, hash senza matcher) segue quello verificato per l'hook di Shinatama ma non e' stato provato su Codex reale.

Verifiche: test unitari di `mukade_chat`, della proiezione del pannello, di `thread_peers`, della tabella hook di Codex (anche analizzata come TOML) e del modello del pannello in TypeScript; `cargo check`/`clippy` per il target Windows; `tsc`, `npm test`, `npm run build`; aspetto del pannello e della sidebar controllato in Chromium con dati finti. Nessuna prova end-to-end con Mukade e agenti reali su Windows: la procedura e' nella sezione Confirmation di ADR-0057.

Verifiche: test unitari dei nomi, del tool e della proiezione (eseguiti compilando i moduli isolati su Linux), `cargo check` e `cargo clippy` dei crate engine e desktop per il target Windows con un compilatore C fittizio, `tsc --noEmit` e `npm test`. Non e' stata eseguita una prova end-to-end con due sessioni reali e il servizio Mukade su Windows.

## Mukade e primo utilizzo

Shinatama avvia da solo il servizio: all'apertura esegue `mukade\mukade.exe chat start` del proprio pacchetto, con i dati in `mukade` dentro la propria cartella dati (`data\mukade` in un'installazione portatile), e alla chiusura esegue `chat stop`. Non serve alcuna variabile d'ambiente ne' un avvio manuale. Verifica file privati, lock del servizio e health con service/instance ID esatti. Gli endpoint sono solo loopback numerico; i token vengono passati nell'ambiente del figlio, non nella riga di comando o nei file di configurazione MCP. Il pin della vecchia libreria di lettura sessioni non e' stato cambiato.

Se `MUKADE_DATA_DIR` e' impostata, Shinatama usa invece quel servizio esterno: deve essere gia' avviato, e Shinatama non lo avvia ne' lo ferma.

Una sessione ordinaria resta apribile se Mukade non parte. Per un avvio comunicante con `room_id`, il desktop riprova l'avvio e, se fallisce, segnala il blocco con il motivo dato da Mukade (per esempio meno di 1 GiB libero sul disco dei dati). Una perdita della connessione successiva al lancio si manifesta come mancato READY, senza un secondo avvio automatico.

La cartella `mukade` accanto a `Shinatama.exe` la crea l'ultimo passo di `build-release.bat`, dal repository Mukade adiacente; non e' nel manifest verificato di `scripts/package-local.ps1`. Senza quella cartella Shinatama non avvia nulla e le sessioni comunicanti vengono rifiutate.

Le sessioni gia' aperte non acquisiscono retroattivamente l'endpoint MCP di Mukade: serve una nuova sessione. La skill presente nella repository e' caricabile da Claude Code nel progetto; non e' stata installata globalmente in altri progetti o harness.

Questa integrazione usa il consumo MCP **manuale**: il child fa join come peer, manda READY, legge/attende e conferma i batch. Non aggiunge hook Mukade, non sovrascrive quelli nativi di tracking e non modifica il trust di Codex. I kit della CLI Mukade restano un percorso separato. Nessuna promessa di risveglio permanente dopo la fine del turno: le attese sono limitate e il bootstrap dichiara il proprio budget.

File principali: `crates/engine/src/session_control.rs`, `harness_session_events.rs`, `mukade_mcp.rs`, i due launcher nativi, `apps/desktop/src-tauri/src/collaboration.rs`, `thread_peers.rs`, `thread_chat.rs`, `crates/engine/src/mukade_chat.rs`, `apps/desktop/src/thread-chat.ts` e `apps/desktop/src/main.ts`.

## Confini di verifica

La validazione della skill controlla struttura, riferimenti e applicazione del protocollo a scenari. Non equivale a un test end-to-end Claude → Mukade → Codex. La prova completa richiede un incarico piccolo, READY correlato, domanda di ritorno, modifica isolata, test, timeout senza duplicazione e risultato verificato dalla sessione iniziale.

Verifiche eseguite sulla versione iniziale:

- Validatore della struttura skill: superato; corpo principale di 171 righe.
- Riferimenti locali, metadati, assenza di segnaposto TODO e spazi finali: superati.
- `mukade chat tools` sul binario locale release: 21 tool, coerenti con il contratto documentato; nessun tool di avvio sessione.
- Valutazione indipendente delle istruzioni su quattro scenari: predefinito Claude da una sessione Codex, Codex esplicito senza READY, dialogo con adapter attivo, domanda ipotetica senza avvio. Comportamento coerente; precisati `request_id` e `reason` nel riferimento MCP dopo la revisione.
- Nessun avvio reale di Claude/Codex o modifica al trust durante queste verifiche.

I file sono presenti nel progetto. La registrazione in Git non e' riuscita: la creazione di `.git/index.lock` e' stata rifiutata per permessi. Non e' quindi attestata una distribuzione o sincronizzazione della skill, ne' il suo caricamento in una sessione Claude gia' aperta.
