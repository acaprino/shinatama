# Adattatore Pi — 10 novembre 2026

Passo C18. Pi (`@earendil-works/pi-coding-agent` 1.4.0, Node 24.14.1) entra nel
registro dei servizi harness accanto a Claude Code, Codex e OpenCode, con lo
stesso flusso **New session → progetto → Pi → Open** e la stessa prova isolata
della TUI. Nessun accesso account, risposta di login o prompt al provider è
stato eseguito; nessuna chiamata a un modello.

## C18.1 — Riconoscere Pi e definire argv senza shim

- `crates/adapter/src/pi/discovery.rs`: il wrapper npm `pi.cmd` viene riconosciuto
  solo per testo esatto (365 byte, CRLF normalizzato) e **mai eseguito**. I due
  bersagli fissi diventano `node.exe` + `…\node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js`.
  Node segue l'ordine del wrapper: `node.exe` accanto al wrapper, altrimenti il
  primo `node.exe` diretto nel PATH (mai `node.cmd`, mai una shell). Pavimento
  Node specifico di Pi: 22.19.0; una prerelease del pavimento non basta.
- `crates/engine/src/pi_probe.rs`: ricerca PATH limitata (128 cartelle, 10 s),
  ordine `pi.exe`, `pi.cmd`, `pi.ps1`; un wrapper diverso dal shim revisionato
  (anche se precede un'installazione valida) viene rifiutato, senza ripiego.
  Il wrapper viene riletto a ogni risoluzione. `node.exe` (PE, un solo link),
  `cli.js` e `package.json` vengono tenuti aperti senza condivisione in
  scrittura/cancellazione, con antenati bloccati contro la rinomina, e
  sottoposti a hash. Due esecuzioni contenute, ognuna ammessa separatamente dal
  servizio e rilasciata prima della successiva: `node --version`, poi
  `node cli.js --version` con `PI_CODING_AGENT_DIR` privato e vuoto,
  `PI_OFFLINE=1`, `PI_SKIP_VERSION_CHECK=1`, `PI_TELEMETRY=0`. Lo script viene
  passato nella forma ordinaria `C:\…`: con un percorso `\\?\` Node lo
  interpreta come modulo relativo e termina con codice 1 (osservato e corretto).
- `crates/adapter/src/pi/auth.rs`: contratto di stato dell'accesso (PI9). L'unico
  comando è `auth check --json --no-refresh --provider <id>`; `--credentials` e
  `print-*` non vengono mai prodotti. Il parser conserva solo stato e motivo
  chiuso; ogni altro campo (anche un valore a forma di token) viene scartato;
  esito e codice di uscita discordanti diventano `unknown`. Fixture catturate
  da 1.4.0 in una cartella vuota: `credentials_not_configured` e
  `provider_not_found` (exit 1). Senza `--provider` 1.4.0 esce con codice 2.
- Driver `apps/desktop/src-tauri/src/agents/pi.rs` registrato nel catalogo; la
  home box è `PI_CODING_AGENT_DIR` (radice `pi`, ruolo extra `NodeCompileCache`)
  e viene registrata come store root dallo stesso `prepare_box` degli altri
  harness, quindi Mukade legge `sessions/` senza codice aggiuntivo.

## C18.2 — Avviare Pi con skill ed estensioni esplicite

- `crates/adapter/src/pi/run.rs`: ricetta TUI `pi-system-box-explicit-set-tui-v1`.
  Gli argomenti iniziano sempre con `--no-skills --no-extensions
  --no-prompt-templates` (1.4.0: i percorsi espliciti `--skill`/`-e` restano
  caricati anche con la scoperta disattivata). Nessun `--approve`, `--mode` o
  modello. Variabili fisse `PI_SKIP_VERSION_CHECK=1`, `PI_TELEMETRY=0`; la prova
  isolata aggiunge `PI_OFFLINE=1`. Le variabili ereditate `PI_*`, `NODE_OPTIONS`,
  `NODE_PATH`, `NODE_REPL_EXTERNAL_MODULE` e `AI_AGENT` vengono rimosse prima
  della ricetta; le chiavi provider dell'utente passano invariate.
- `crates/adapter/src/pi/sets.rs`: un elemento del set raggiunge Pi solo come
  percorso esplicito dentro la cartella della propria istanza (`--skill` per
  skill, `-e` per plugin/estensioni, ordine canonico del manifest). Percorsi
  fuori istanza (cartelle skill dell'utente, altra istanza, `..`, UNC, relativi)
  e MCP (PI7, nessun override per lancio) vengono rifiutati. Due istanze con set
  diversi non condividono token. Il lancio attuale qualifica solo il set vuoto:
  non esiste ancora uno scrittore che materializzi skill/estensioni per istanza.
- `crates/engine/src/pi_launch.rs`: System+Box con `HomeMapping::Pi`,
  `RuntimeKind::Node` e cache di compilazione Node privata; `CommandForm::NodeScript`
  con pavimento 22.19.0. `PI_CODING_AGENT_DIR` è la home box (le sessioni
  finiscono in `sessions/`). Il wrapper viene risolto di nuovo e `node.exe` e
  `cli.js` vengono ripinnati: hash e dimensioni devono coincidere con
  l'osservazione; il receipt della prova lega agente, identità di `node.exe`,
  versione Pi e profilo. Il file `cli.js` resta aperto (senza condivisione in
  scrittura) per tutta la vita del lancio. Profilo fisso: 1 GiB aggregato,
  768 MiB per processo, 8 processi, CPU 10%, coda 4 MiB/512 elementi.
  `native_launch` riconosce l'ambito `explicit_set_no_discovery` e la prima
  schermata di Pi (riga dei tasti `escape interrupt … clear/exit` più il
  rimando all'aiuto di avvio). Su questa macchina compare in 440–540 ms dalla
  creazione del processo, ben sotto la finestra di 7 s della prova isolata.
  `HOME` figlio non viene usato per isolare le skill: su Windows Node risolve
  la home da `USERPROFILE`; l'isolamento è dato dagli interruttori di scoperta.

### Prova nativa

Prova `native-new-session-user-path` **superata con Pi 1.4.0 e Node 24.14.1**
su binari costruiti da questo ramo e copiati in un pacchetto privato
(`SHINATAMA_SMOKE_APP_EXE`), dati applicativi nuovi e progetti sintetici:
nessun controllo preparatorio né avvio automatico, un solo Open salva
progetto, thread e vista, il servizio locale viene preparato automaticamente,
la prova TUI isolata produce il receipt, la TUI originale si apre nel
terminale contenuto, la sessione posseduta viene fermata e la finestra
chiusa. Ricevuta locale ignorata da Git: `.prototype-evidence/new-session-pi.json`
(`status: passed`, quattro controlli). La home box contiene la cartella del
progetto sotto `sessions/`; senza alcun messaggio Pi non scrive ancora il file
di sessione, quindi la lettura Mukade di una sessione Pi reale non è provata.

La prova `new-session-smoke.mjs` usava ancora il pulsante "Stop terminal" della
barra del riquadro, nascosta dal workspace semplificato su master: ora chiude
la scheda con il suo `×`, che chiede conferma prima di fermare la sessione.
Il ramo PowerShell della stessa prova resta fermo sul vecchio dialogo progetto
(`#folder`), problema preesistente fuori da C18.

## Verifica sul master (10 novembre 2026)

Commit integrati sul master come `5f10452` (C18.1) e `81ca416` (C18.2). Ripetuti sul master:
`cargo test -p shinatama-adapter pi` (13), `cargo test -p shinatama-engine --features session-reader --lib pi_` (5),
`SHINATAMA_SMOKE_SESSION_HARNESS=pi node apps/desktop/tests/new-session-smoke.mjs` PASS (4 controlli).
C18.3 (RPC e capacità) è nella sezione seguente.

## C18.3 — Collegare RPC e provare le capacità Pi

- `crates/adapter/src/pi/rpc.rs`: canale di controllo RPC (`--mode rpc
  --no-session` più gli interruttori di scoperta) limitato alla sola lettura.
  Sono codificabili solo `get_state`, `get_commands` e un comando fisso non
  supportato (`shinatama_unsupported_probe`): prompt, steer, follow-up, bash,
  modello o sessione non sono rappresentabili, quindi nessun run del modello
  può partire da qui. Framing JSONL limitato (LF, CR opzionale, 16 KiB per
  record, 64 record); U+2028/U+2029 restano dentro il record. La proiezione
  conserva solo comando, esito, rifiuto chiuso, `isStreaming`, `messageCount`
  e numero di comandi: modello, provider, costi, sessionId e testo d'errore
  vengono scartati. Nessuna operazione ADR-0011 passa da questo canale:
  `AttachBackend` rifiutata come non implementata (PI5), messaggi, headless ed
  eventi rifiutati come non qualificati.
- `crates/adapter/src/pi/capabilities.rs`: capacità dichiarate per la build
  esatta osservata. `Detect` (fixture `pi-npm-shim-node-script-v1`) dopo la
  sonda contenuta; `StartNew`, `Placement`, `InheritedMarkers` (fixture
  `pi-system-box-explicit-set-tui-v1`) solo dopo il receipt della prova TUI.
  Tutte le altre operazioni hanno un rifiuto esplicito; l'evidenza non vale
  per un'altra versione. La sonda della app proietta ora `Detect` nelle sue
  diagnostiche.
- `crates/engine/src/pi_probe.rs`: `run_rpc_control_with` riusa pin, scratch
  privato e ammissione della sonda; la seconda esecuzione contenuta è Pi in
  RPC con lo script fisso su stdin, poi stdin chiuso. Non è usata dai controlli
  dell'app. Test installato (ignorato, esplicito) superato su Pi 1.4.0:
  `get_state` riuscito con `isStreaming=false` e `messageCount=0`,
  `get_commands` riuscito con 0 comandi, comando non supportato rifiutato come
  `unknown_command`, nessun evento, uscita 0, rilascio confermato (~2,4 s).
- `verify/PI/run.ps1` riesegue le fixture installate (versione/Node e RPC).

## Limiti

- Nessun accesso, risposta di login o prompt è stato inviato: autenticazione
  reale, traffico provider/TLS, strumenti, eventi di run (`agent_start`,
  `agent_settled`), `ui_prompt`, coda messaggi e headless restano non provati.
- Il contratto `auth check` (PI9) è testato su fixture ma l'app non lo esegue:
  lo stato di accesso resta `unknown` nella UI.
- Solo il set vuoto viene avviato; manca lo scrittore che materializza skill
  ed estensioni per istanza. MCP per lancio (PI7) rifiutato.
- Nessun resume nativo, binding di thread o hook Pi (`native_resume=false`).
- La lettura Mukade di una sessione Pi reale non è provata: senza messaggi Pi
  non scrive il file di sessione.
- RPC non è collegato a un flusso utente: esiste solo come prova contenuta.
- Supporto Windows soltanto; Daodan (FRD-0001/R08) rinviato.
