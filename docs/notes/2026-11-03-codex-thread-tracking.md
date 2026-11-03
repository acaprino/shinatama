# Codex: associazione della sessione nativa al thread

Incremento C11/C25 del 3 novembre 2026. Codice implementato; `cargo check`
della libreria engine, i 15 test mirati e la build desktop passano. La prova
nativa resta da completare. Il precedente smoke di
apertura Codex non dimostra la consegna degli hook o la ripresa nativa.

## Contratto implementato

`codex_launch.rs` aggiunge `prepare_tracked_trial` e `prepare_tracked_start`,
conservando gli ingressi senza tracking. Il profilo
`codex-system-box-tracked-no-daemon-workspace-tui-v1` richiede una propria
ricevuta di startup: quella del profilo precedente non basta. Argomenti e
permessi di base restano `--no-daemon`, `workspace-write`, `on-request` e
sandbox Windows `unelevated`. I limiti restano 1 GiB/job, 768 MiB/processo,
8 processi, CPU 10%, code 4 MiB/512 elementi.

Il servizio Codex prepara `PendingThread::Start` senza un UUID nativo.
Il primo `SessionStart` autenticato con `source=startup` fornisce l'identità
scelta da Codex al lifecycle condiviso. Gli eventi successivi restano vincolati
allo stesso proprietario; `source` non autorizza a cambiare processo o radice.
Non si selezionano sessioni per data, prefisso o nome del file. Il ricevitore
e il bridge sono condivisi con Claude, con route e dialetto Codex distinti.
L'acknowledgement HTTP significa accodamento, non commit SQLite.

Anche un Codex che termina prima di comunicare il proprio UUID mantiene il
collegamento al proprietario nativo fino alla ricevuta di cleanup. L'assenza
di una ricevuta non autorizza la chiusura: è ammessa solo nel retry di un
acknowledgement già tentato, quando anche il collegamento esatto è assente.
Un observer condiviso notifica il frontend dopo le mutazioni confermate;
la sidebar rilegge i metadati del progetto selezionato senza polling o
richieste aggiuntive all'utente. La notifica non trasporta dati nativi né
autorità di avvio/chiusura.

`harness_hooks.rs` estrae la verifica già usata da Claude: byte, identità del
file e antenati rimangono bloccati durante l'invocazione. Gli hardlink del
bridge sono ammessi mantenendo il blocco di scrittura/eliminazione.
Il comando usa WindowsPowerShell ottenuto da `GetSystemDirectoryW`, anch'esso
pinnato, senza ricerca in PATH. `-NoProfile -NonInteractive -EncodedCommand`
trasporta lo script UTF-16LE; il percorso dell'helper è quotato come stringa
PowerShell e il suo codice di uscita viene propagato. Il token resta solo
nell'ambiente del figlio. Nessuna modifica a ExecutionPolicy.

## Verifica statica dello schema 0.166.0

La configurazione tracked è limitata alla versione **0.166.0**. Le altre
versioni richiedono una nuova verifica dello schema. Il layer `SessionFlags`
aggiunge un solo gruppo `hooks.SessionStart`, timeout 5 secondi, `async=false`
e matcher `^(startup|resume|clear|compact|fork)$`. L'enum ufficiale contiene
esattamente queste cinque varianti e il dispatch confronta il matcher con la
stringa `source`; il matcher di SessionStart viene conservato senza riscrittura.
Fonti: [SessionStartSource e dispatch](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/events/session_start.rs#L21-L73),
[normalizzazione del matcher](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/events/common.rs#L120-L139)
e [parser del matcher](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/engine/matcher.rs#L9-L34).

La fiducia riguarda solo la chiave
`C:\<session-flags>\config.toml:session_start:0:0`. Il digest è quello della
definizione normalizzata: `event_name=session_start`, matcher e singolo
handler con `type`, `command`, `timeout=5`, `async=false`. I campi opzionali
assenti non entrano nel valore TOML; gli oggetti JSON vengono ordinati prima
di SHA-256 e il valore usa il prefisso `sha256:`. La porta e il percorso
codificato fanno parte del comando e quindi del digest.
Fonti: [origine SessionFlags e normalizzazione](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/engine/discovery.rs#L428-L459),
[identità normalizzata](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/engine/discovery.rs#L935-L959),
[serializzazione del gruppo/handler](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/protocol/src/hooks.rs#L134-L164)
e [fingerprint](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/config/src/fingerprint.rs#L52-L63).

`hooks.state` nel layer SessionFlags può impostare il solo `trusted_hash`
generato per questa definizione. Non vengono scritti file dell'utente né
concessa fiducia globale agli altri hook. `features.hooks=true` abilita la
funzione richiesta; gli altri hook mantengono le proprie regole di fiducia.
Fonti: [precedenza dello stato per hook](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/config_rules.rs#L7-L63)
e [shell e feature del runtime](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/core/src/session/mod.rs#L5494-L5526).

## Prove necessarie prima di dichiararlo funzionante

Risultati intermedi: `cargo check --offline --locked -p shinatama-engine
--lib -j1` **PASS**, 15,97 secondi. Il successivo test del trasporto condiviso
si è fermato durante la compilazione di `shinatama-state`, prima di eseguire
test, con `only metadata stub found` per `core` e `can't find crate for std`.
La misura immediatamente successiva di `GlobalMemoryStatusEx` rilevava
**0,17 GiB di commit disponibile**; i file `core.rlib` e `core.rmeta` della
toolchain fissata 1.99.1 sono presenti. La pressione memoria è un possibile
fattore, non una diagnosi definitiva dell'errore del compilatore. Con **6,9 GiB
disponibili** la ripetizione passa senza modifiche alla toolchain:

- trasporto condiviso: 1/1, 1,30 secondi;
- ricette Codex: 6/6;
- pin hardlink Claude: 1/1;
- bridge eseguibile: 4/4, 0,21 secondi.
- lifecycle desktop con DB privato: 3/3, 0,82 secondi; callback dopo commit,
  errore del callback isolato, UUID Codex osservato, UUID Claude conservato e
  ricevuta obbligatoria anche quando Codex non ha ancora comunicato un UUID.

`npm run build` passa. Build offline/locked/seriale di app, terminal-helper,
session-helper e hook **PASS**, 1 minuto e 45 secondi.

1. Eseguire i test mirati Codex e la regressione del pin hardlink Claude;
   compilare il collegamento desktop al lifecycle condiviso. Rustfmt e il
   controllo degli spazi del diff sono già passati.
2. Con la build reale 0.166.0, verificare riconoscimento della fiducia, consegna
   dell'UUID, commit del segmento e aggiornamento del thread. Misurare anche
   il processo PowerShell aggiuntivo entro gli stessi limiti e timeout.
3. Chiudere e ripetere la sessione verificando ricevuta di cleanup e assenza di
   associazioni incrociate. La ricetta installa solo SessionStart: la chiusura
   resta fondata sulla prova nativa di cleanup, non su SessionEnd.

`native_resume` resta **false**. La selezione Resume tipizzata accetta un UUID
esatto ma non può usare la qualificazione StartNew; questo incremento non
espone ancora una preparazione Resume lanciabile. Una prova di startup non
dimostra la consegna del primo hook, che Codex esegue nel percorso del turno
([hook runtime](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/core/src/hook_runtime.rs)),
né la continuità della conversazione dopo una ripresa.

## Esiti del 3 novembre sulla stessa build

La prima esecuzione dello stadio `notification-refresh` di
`thread-recovery-smoke.mjs` è fallita con `unexpected-entry-count:0`. La causa è
della fixture, non del prodotto: `listen_js_script` di Tauri 2.12.5 crea le voci
di `__internal_unstable_listeners_object_id__` con `Object.defineProperty` senza
`enumerable`, perciò `Object.entries` non le vede. La fixture usa ora
`Object.getOwnPropertyNames` e codici di motivo limitati (stringhe fisse e
conteggi, nessun dato nativo). Con la correzione la prova passa per intero,
inclusi il refresh della sidebar alla notifica registrata e la conservazione
della selezione, senza alcun avvio di harness.

La consegna backend reale dell'evento resta **non coperta**: l'observer si attiva
solo dopo mutazioni native confermate del lifecycle, impossibili senza harness in
questa fixture. L'iniezione nel callback frontend registrato è dichiarata nella
ricevuta (`notificationDeliveryMode=synthetic-registered-frontend-callback`).

Nella stessa build debug, la prova protetta completa di Resume UI Claude
(`claude-resume-smoke.mjs`) è **PASS**: vale anche come regressione del percorso
tracked Claude rispetto a transport condiviso, hook PowerShell e observer di
questo incremento. I 15 test mirati elencati sopra sono stati rieseguiti e
passano sullo stesso albero prima del commit. Restano richieste le prove native
Codex dell'elenco precedente prima di qualsiasi promozione.
