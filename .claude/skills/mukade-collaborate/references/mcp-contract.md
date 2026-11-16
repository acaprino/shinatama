# Contratto operativo Mukade

Verificato sui sorgenti locali Mukade il 13 novembre 2026. Lo schema esposto
dal server installato prevale su questa fotografia. I nomi seguenti sono i
nomi logici: scopri il prefisso effettivo nel client.

## Identita' e idempotenza

- `room_id` e `member_id` sono identita'; `member_handle` e' una credenziale.
- `name` e' il destinatario di `chat_send.recipients`, non il member ID.
- Per le operazioni che espongono `request_id`, genera un UUID per operazione
  logica e riusa **stesso UUID e stessi argomenti** dopo un esito incerto.
  Non cambiare UUID per aggirare un errore o il controllo dei duplicati.
  Read, wait, observe e ack non espongono `request_id`: non aggiungerlo.
- Gli event ID sono stringhe decimali: non convertirli in numeri JavaScript.
- `chat_rejoin` ruota l'handle e invalida il processo precedente. Non e' una
  verifica di disponibilita' e non va usato per risolvere un semplice timeout.
- Un membro attivo non e' un processo necessariamente vivo. `chat_bind`
  associa una sessione gia' esistente; non la avvia.

## Operazioni utilizzate

La colonna input elenca i campi obbligatori, salvo indicazione diversa.

| Tool | Input e note |
|---|---|
| `chat_rooms` | Nessuno; opzionali `cursor`, `limit`. Scorri le pagine quando cerchi una stanza. |
| `chat_open` | `request_id`, `name` (slug minuscolo, massimo 64 caratteri). Conserva l'ID restituito. |
| `chat_join` | `request_id`, `room_id`, `harness`, `role`; opzionali `name`, `session_id`. Restituisce `member_id`, `member_handle`, `name`, `role`, `instructions`. |
| `chat_members` | `room_id`; opzionali `cursor`, `limit`. Include membri storici e attivi. |
| `chat_candidates` | `harness`; opzionali `cwd`, `cursor`, `limit`. Restituisce ID nativi esatti degli store configurati. |
| `chat_bind` | `request_id`, `room_id`, `member_handle`, `harness`, `session_id`; `source_ref` per disambiguare una sorgente. |
| `chat_send` | `request_id`, `room_id`, `member_handle`, `text`; `recipients` opzionale ma da specificare per incarichi, domande e risultati. Solo peer/lead. |
| `chat_signal` | `request_id`, `room_id`, `member_handle`, `instruction_event_id`, `signal`; `reason` obbligatorio solo per `blocked`. Solo worker, su istruzione indirizzata da un lead. |
| `chat_read` | `room_id`, `member_handle`; opzionali `budget_chars`, `ack_batch_id`. Prepara un batch da confermare. |
| `chat_wait` | Come read, con `addressed_only` e `timeout_seconds` (0..300). La skill usa attese brevi e limitate. |
| `chat_ack` | `room_id`, `member_handle`, `batch_id`. Conferma la consegna del batch, non il completamento del lavoro. |
| `chat_peek` | `room_id`, `member_handle`; opzionale `budget_chars`. Non consuma lo stream. |
| `chat_ledger` | `room_id`; filtri opzionali `actor`, `kind`, `since`, `until`, `mention`, `text`, `cursor`, `limit`, `member_handle`. Non consuma lo stream. |
| `chat_observe` | `room_id`, `member_handle`, `target_member_id`; opzionali `budget_chars`, `ack_batch_id`, `latest`, `last_turns`. Solo observer, bersaglio associato esattamente. |

`chat_send` restituisce `event_id`: conservarlo permette di collegare le
risposte all'incarico. Un worker usa `signal: done` senza `reason`; per
`blocked` occorre `reason` su una riga, massimo 200 caratteri. Il segnale
`done` e' terminale per quell'istruzione. Un peer usa invece messaggi testuali
e non tenta `chat_signal`.

Gli elenchi sono paginati. Uno snapshot del ledger non e' una sottoscrizione:
per vedere nuovi eventi serve una nuova interrogazione. Per read/wait/observe
conferma solo un `batch_id` effettivamente restituito e dopo averne gestito il
contenuto; se e' null non inventare un ACK.

L'osservazione usa uno stream separato. Se gli hook gestiscono la chat, lascia
a loro i batch della stanza; un observer puo' confermare i propri batch di
`chat_observe`. La vista ridotta non permette di affermare che un test sia
passato quando l'output completo non e' disponibile.

## Avvio tramite un host

Il server Mukade esaminato espone 21 tool `chat_*`; nessuno avvia processi o schede.
Il server **Shinatama**, distinto da Mukade, espone ora il ponte nativo:

| Tool host | Input |
|---|---|
| `shinatama_session_open` | `request_id` UUID, `instruction` (max4096 byte); opzionali `harness` (`claude_code` predefinito o `codex`), `mode` (`delegation` predefinito, `dialogue`, `review`), `room_id` e `coordinator` insieme. |
| `shinatama_session_status` | `request_id` dello stesso chiamante. |
| `shinatama_thread_peers` | Nessun argomento. Sola lettura: thread (workset) della sessione chiamante, `mukade.room_name`, `you.member_name`, `peers[]` con `session_id`, `harness`, `title`, `state` (`running`, `stopped`, `not_started`, `closed`), `member_name` e `chat` (`mukade` o `none`), passi in `how_to`. |

**Sessioni dello stesso thread.** I nomi sono deterministici:
`shinatama-thread-<suffisso del workset>` per la stanza e
`<harness>-<suffisso della sessione>` per il membro (`claude-…`, `codex-…`).
`chat_open` restituisce la stanza esistente per un nome gia' noto, quindi ogni
sessione del thread arriva alla stessa stanza senza un registro di Shinatama.
`state: running` significa apertura registrata, non agente in ascolto. Il
chiamante e' quello autenticato dal ricevitore: nessun argomento puo' indicare
un'altra sessione. Una sessione aperta con `shinatama_session_open` entra nel
thread del chiamante; il suo bootstrap nomina la stanza del thread.

La risposta distingue `queued`, `starting`, `opened`, `failed`, `cancelled`.
`opened.result.session_id` e' l'istanza posseduta da Shinatama, mentre
`thread_id` identifica il thread salvato. Non sono il `session_id` da usare
in `chat_bind`: per quello occorre l'ID esatto dell'harness. La deduplicazione
vale per chiamante e UUID: riusare un UUID con parametri diversi e' un errore.
Le ricevute restano nel processo host; il riavvio dell'app termina le istanze
e non ripete le richieste. Sono ammessi al massimo quattro incarichi di avvio
per chiamante e 256 ricevute per esecuzione dell'host, senza espulsione che
potrebbe trasformare un retry in un nuovo processo.

Per collegare anche Mukade, Shinatama avvia da solo il programma Mukade del
proprio pacchetto (`mukade\mukade.exe`), con i dati in `mukade` nella propria
cartella dati, e lo ferma alla chiusura. Ne verifica identita' e credenziali e
configura le due connessioni MCP prima del lancio. Non serve alcuna
preparazione. Solo se `MUKADE_DATA_DIR` e' impostata Shinatama usa quel servizio
esterno, che deve essere gia' avviato: non lo avvia e non lo ferma.
Il child usa membership e consumo manuali, senza hook Mukade aggiuntivi.

La CLI puo' costruire il kit per Claude/Codex e lanciare il processo:

```text
mukade chat launch claude_code --room progetto-task --role peer --listen 120 --max-wakes 10 -- <argomenti-harness>
mukade chat launch codex --room progetto-task --role peer --listen 120 --max-wakes 10 -- <argomenti-harness>
```

Questi sono modelli di comando, non istruzioni da copiare con il segnaposto.
Non c'e' un flag `--name` di launch. Non assumere il nome del futuro membro.
La CLI attende l'uscita del figlio nello stesso terminale; un comando eseguito
in foreground blocca il coordinatore. Usarla richiede un terminale indipendente
gestito dall'host, non un semplice `chat_join` o un `Start-Process` dichiarato
come scheda Shinatama. Passare un prompt iniziale richiede gli argomenti
documentati dalla versione dell'harness; non interpolare il brief in una
stringa shell non quotata. Preferire argomenti strutturati e riferimenti a file.

Il kit Claude usa un plugin per quel lancio con hook e configurazione MCP;
il kit Codex compone override e hook. Codex richiede il trust dei tre hook
Mukade: `mukade chat trust codex --dry-run` e' una verifica; la variante senza
dry-run modifica il trust. Non eseguirla tacitamente e non usare opzioni che
disabilitino i controlli. Fai rispettare la procedura di trust dell'harness.

`--listen` dura 0..3600 secondi dopo ciascun turno; `--max-wakes` limita i
risvegli. Un broadcast non e' un risveglio indirizzato. Il sistema non
garantisce di riattivare una sessione dopo che il suo ascolto e' terminato.
La CLI non restituisce una ricevuta strutturata di sessione nativa pronta.
READY e la correlazione con il launcher sono necessari.

Gli store di sessione sono configurati nel servizio. Un child con una home
privata non diventa osservabile solo aggiungendo `source_ref`: quel parametro
non registra una nuova root. Conserva questo limite nel resoconto se il bind
non puo' essere verificato.

## Errori da mantenere visibili

| Evidenza | Stato da riportare |
|---|---|
| MCP non esposto o irraggiungibile | Canale non disponibile; nessuna consegna verificata. |
| Solo `chat_join` riuscito | Membro registrato; nessun processo avviato da questa operazione. |
| Launcher accetta ma niente READY | Avvio richiesto; sessione non ancora pronta. |
| Codex aperto ma hook non trusted | Processo aperto, adesione automatica alla stanza bloccata. |
| Timeout dopo invio | Esito incerto: controllare evento/checkpoint prima di ritentare. |
| Sessione senza binding | Chat utilizzabile se pronta; osservazione nativa non verificata. |
| `chat_close` riuscito | Stanza chiusa; nessuna prova di arresto del processo. |

Riferimenti di implementazione nel repository Mukade: `src/bus/chat_tools.rs`,
`src/bus/chat_mutation.rs`, `src/chat/adapter.rs`, `src/cli/chat.rs`,
`src/harnesses/claude_code.rs`, `src/harnesses/codex.rs`.
