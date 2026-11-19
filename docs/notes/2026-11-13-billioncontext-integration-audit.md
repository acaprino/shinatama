# BillionContext: audit dell’integrazione Shinatama

Data: 2026-11-13. Ambito: engine Rust, adapter, token-economy, controller desktop, UI e prove conservate nel repository.

## Perimetro e revisione

- Revisione Shinatama esaminata: `68ebb87f154d1b4e276d6ec4987272fded7599c4` (`S` nei riferimenti seguenti). Tutti i riferimenti `file:line` sono relativi alla radice del repository e a questa revisione, salvo indicazione diversa.
- L’albero aveva modifiche utente preesistenti, incluse `main.rs`, `main.ts`, `session_detail.rs`, test di recovery e `progress.json`. Non sono state modificate né usate come prova di una modifica conclusa. Questa nota è l’unica scrittura dell’audit.
- Nessun processo prodotto, test nativo, server o chiamata provider è stato avviato. Nessun database, log privato o credenziale è stato letto. Gli esiti riportati sono quelli delle prove documentate, non nuove esecuzioni del 13 novembre.
- Il pin del **sorgente BillionContext** è `sidecar/source-build/provenance.lock.json:3-19`: versione `0.1.207`, upstream `7e69b49ff4facb00b5ba6541b48d0f7fbe63d63d`, sorgente canonico `D:/Projects/horness-launcher/packages/billion-context`, entry SHA-256 `0c0e71df9811dbc20050fc1e9887d6da8afcefc73ff78348f1a51bcd5e9cca3e`.
- `sidecar/runtime.lock.json:2-12` fissa invece Node 24.14.1, SHA-256 `58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f`. Non è il pin del sorgente BillionContext.
- Il lock conserva una ricevuta di preparazione statica; i suoi `runtimeQualified:false` non cancellano le successive prove C12/C13. Ugualmente, quelle prove limitate non trasformano il lock in una certificazione di tutte le funzionalità.

## Conclusione

**Fatto:** Shinatama dispone di un’integrazione reale del proxy, con isolamento del runtime, identità verificata, limiti di risorse e recupero finito. Non è più corretto descriverla come sola architettura sulla carta.

**Fatto:** i quattro percorsi nativi Claude, Codex, OpenCode e Pi impostano il proxy e richiedono un proprietario healthy/Ready. Non esiste in questi percorsi una modalità di apertura indipendente dal sidecar.

**Inferenza:** il requisito «non interferire mai negativamente con il workflow» non è dimostrato: il proxy è un punto comune di indisponibilità e le trasformazioni sono attive nel baseline. Riduzione dei token, qualità semantica, cache, latenza, richieste ausiliarie e ripristino delle immagini richiedono misure distinte.

**Raccomandazione:** completare osservabilità e prove di non regressione prima di aumentare l’aggressività; distinguere bypass delle trasformazioni, bypass di una richiesta speciale e collegamento diretto al provider. Sono tre contratti diversi.

## 1. Cosa viene effettivamente preparato

| Area | Implementazione osservata | Evidenza `S` |
| --- | --- | --- |
| Provenienza | Hash dei file e degli alberi; aggiornamento deliberato del lock; nessuna patch locale del request path | `sidecar/source-build/README.md:10-22,35-39`; `crates/engine/src/sidecar_runtime.rs:594` |
| Runtime | Node + `start --host 127.0.0.1 --port … --config … --no-auto-update`; nessun launcher per sessione | `crates/engine/src/sidecar_runtime.rs:434-447` |
| Ambiente sidecar | Ambiente ricostruito: solo SystemRoot/WINDIR ereditati, home/config/data/cache/temp privati, PATH limitato a Node | `crates/engine/src/sidecar_runtime.rs:449-508` |
| Configurazione | File nuovo con `create_new`; file preesistente preservato, non attestato semanticamente | `crates/engine/src/sidecar_config.rs:101-144` |
| Storage | `unified:full`, SQLite, download SQLite disattivato; schema atteso 9 e CCR incrementale | `crates/engine/src/sidecar_config.rs:140-175` |
| Trasformazioni | Prompt rewrite `auto`; tiers; Absorb/CCR da 4.000 token; adaptive pressure con tetto 160.000; steering L2; effort routing spento | `crates/engine/src/sidecar_config.rs:162-176` |
| Parametri inattivi | `targetRatio` e `growthWindow` omessi perché il percorso osservato non li consuma | `crates/engine/src/sidecar_config.rs:171-173,207-216` |
| Runtime desktop | Porta loopback scelta dal controller, singolo supervisor, ammissione risorse, identità health del figlio | `apps/desktop/src-tauri/src/sidecar.rs:239-339` |

Il baseline è un’intenzione iniziale, non la configurazione effettiva di ogni sessione: un file esistente è conservato. Il controller non esporta una configurazione risolta per provider/modello. `RuntimeSummary` riporta flag conservativi statici a false (`crates/engine/src/sidecar_runtime.rs:306-325`).

**Proposta:** separare `prepared_identity`, `runtime_health`, `effective_policy` ed `evidence_scope`. `Ready` deve significare servizio raggiungibile; «CCR attivo per questo turno» richiede una ricevuta del percorso realmente eseguito. Il requisito T9 lo prevede già (`docs/verification-register.md:515`).

## 2. Supervisione, errore, retry e arresto

### Garanzie presenti

- Un solo worker proprietario e claim statico; comandi limitati Start/Stop/Retry/Activity (`crates/engine/src/sidecar_supervisor.rs:32,99-104,168-232`).
- La morte è decisa dal processo esatto posseduto. Dopo un health mancato viene verificata la porta con bind: occupata → Busy, libera → Degraded; un processo ancora vivo non viene rimpiazzato in entrambi i casi (`:407-433,529-563,654-669`).
- Busy riduce i probe a intervalli 1/2/4/8 secondi; le osservazioni del worker continuano durante il backoff (`:502-505,675-692`).
- Prima uscita → un riavvio automatico sulla ricetta/porta originale; seconda uscita → Failed/RepeatedExit, in attesa di azione esplicita (`:411-424`).
- Violazione del budget → stop del solo job posseduto, niente aumento automatico dei limiti e niente replay del lavoro agente (`:450-469`).
- Cleanup incerto → Retired con owner e prenotazione conservati. Retry può riprovare il cleanup; non inventa un rilascio né avvia un duplicato (`:288-318,614-645`).
- Lo shutdown HTTP segue un health con identità verificata sulla **stessa connessione** (`:808-820`).
- stdout/stderr vengono drenati nel worker, conteggiati e scartati; non sono inseriti nel terminale agente (`:754-789`). Il file diagnostico del sidecar è configurato separatamente (`crates/engine/src/sidecar_config.rs:154-164`).

### Limiti concreti

1. **Recuperare il processo non recupera la richiesta interrotta.** Il supervisor non possiede né ripete i messaggi del modello. Il no-replay è una proprietà utile contro duplicazioni, ma non equivale a «ogni turno riesce». La prova C13 non usa richieste provider (`docs/verification/2026-11-03-C13-sidecar-supervision.md:38-45`).
2. **Busy non garantisce disponibilità del trasporto.** La conservazione del PID evita la tempesta di riavvii; il criterio SC16 richiede separatamente che nessuna connessione agente venga rifiutata durante il soak (`docs/verification-register.md:394`).
3. **Porta libera + processo vivo resta Degraded.** È una scelta conservativa nell’implementazione corrente; un processo che non ricrea il listener può restare indisponibile fino all’intervento utente (`crates/engine/src/sidecar_supervisor.rs:561-563,690`). Non va descritto come recovery automatico garantito.
4. **L’arresto non è coordinato con le sessioni.** `Activity::RequestInFlight/SessionSave` esistono, ma la ricerca dei produttori di `Command::Activity` in `crates`/`apps` trova solo il consumer del supervisor. Non c’è evidenza di drain basato sui turni in questo livello (`:93-103,321,585-604`).
5. **La UI consente Stop durante l’uso.** Il controllo è chiamato “Session index”, ma ferma il proxy condiviso; Stop è disabilitato solo in Stopped/Unavailable o durante una richiesta UI (`apps/desktop/src/sidecar-control.ts:12,20,35-37,57`). Il controller invia Stop senza verificare le sessioni attive (`apps/desktop/src-tauri/src/sidecar.rs:161-168`).
6. **Limite output cumulativo.** Il desktop imposta 64 MiB per stream; superarlo ferma il processo. Non è un budget a finestra o una rotazione (`apps/desktop/src-tauri/src/sidecar.rs:320`; `crates/engine/src/sidecar_supervisor.rs:489-499,785-787`). La frequenza reale di raggiungimento non è misurata qui.

**Proposte:** bloccare nuove ammissioni prima dello stop, contare richieste attive/salvataggi, drain finito e stop esplicito con conseguenza comprensibile; stato di richiesta interrotta distinto da errore di preparazione; nessun retry nascosto dopo inizio dello stream o possibile esecuzione di tool. Valutare il limite diagnostico con misure di soak, senza togliere il contenimento.

## 3. Bypass e continuità

| Contratto | Stato osservato | Implicazione |
| --- | --- | --- |
| Escludere una richiesta speciale da rewrite/CCR/compressione | SC9 richiede byte identici per seed/bootstrap/receipt; manca la fixture dedicata eseguita | Non assumere che i messaggi strutturati Shinatama attraversino inalterati il baseline |
| Spegnere le trasformazioni restando nello stesso trasporto | La fixture source-runtime osserva il resolver `ACP_PASSTHROUGH=1`; dichiara esplicitamente `runningCliTrafficVerified:false` | Non equivale a controllo prodotto disponibile e provato sul filo |
| Aprire l’agente senza sidecar | Tutti e quattro i launch richiedono Ready/healthy/process_alive | Un guasto del sidecar impedisce nuove aperture normali |
| Sopravvivere al crash del sidecar | Un riavvio automatico, stessa porta | Le sessioni possono usare il nuovo listener; non c’è prova generale di ripresa del turno in corso |

Riferimenti `S`: `docs/verification-register.md:387`; `docs/verification/2026-11-04-C12.4-effective-config-live.md:77-80`; `crates/engine/tests/fixtures/effective-sidecar.mjs:158,174-176`; `crates/engine/src/claude_launch.rs:826-843`; `codex_launch.rs:975-992`; `opencode_launch.rs:672-689`; `pi_launch.rs:601-616`.

**Proposta:** un contratto canonico esplicito per richieste speciali, riconosciuto prima di ogni mutazione, più prova di confronto dei byte ricevuti. Il marker non deve essere dedotto dal testo del prompt, dal suo volume o dal numero massimo di token in uscita.

**Proposta:** per ridurre l’interferenza, valutare una modalità “solo trasporto” con trasformazioni disattivate e configurazione congelata a inizio sessione. Un collegamento diretto che aggira il proxy cambia invece instradamento, trust e controlli: richiede una scelta di prodotto e una ricetta verificata, non un fallback silenzioso.

## 4. Harness, provider e isolamento

| Harness | Collegamento attuale | Prova reperita e limite |
| --- | --- | --- |
| Claude | HTTP(S)_PROXY loopback; NO_PROXY vuoto; CA solo figlio | C12.4: Claude 2.1.318 headless reale → origine sintetica, steering L2 osservato; niente qualificazione generale di account/provider/TUI tracciata |
| Codex | HTTP(S)_PROXY loopback; NO_PROXY vuoto; CODEX_CA_CERTIFICATE e SSL_CERT_FILE | Apertura TUI documentata; prove successive di tracking/resume sono lifecycle, non certificazione compressione su ChatGPT/API |
| OpenCode | Proxy; loopback escluso con NO_PROXY; NODE_EXTRA_CA_CERTS/SSL_CERT_FILE | TUI 2.2.5 e storage privato documentati; traffico provider e strumenti reali fuori da questa prova |
| Pi | Proxy; loopback escluso; CA Node solo figlio; directory Pi privata | TUI/RPC limitato 1.4.0 documentati; provider/TLS/turni reali non provati dalla prova C18 |
| Hermes / DeepSeek Harness | Nessun launch nativo equivalente esaminato nel prodotto corrente | HM12/DS12 restano gate specifici, non ereditabili dalle prove Node/Claude |

Fonti `S`: `crates/engine/src/claude_launch.rs:683-698`; `codex_launch.rs:721-745`; `opencode_launch.rs:560-575`; `pi_launch.rs:484-499`; `docs/verification/2026-11-04-C12.4-effective-config-live.md:59-82`; `docs/notes/2026-11-03-codex-open.md:30-47`; `docs/notes/2026-11-03-codex-native-resume.md:35-75`; `docs/notes/2026-11-03-opencode-open.md:28-51`; `docs/notes/2026-11-10-pi-adapter.md:101-144`; `docs/verification-register.md:284,303`.

**Distinzioni necessarie:**

- La ricetta proxy non prova che ogni libreria HTTP del processo la rispetti, né che un protocollo intercettato venga compresso correttamente.
- La CA figlia e la home privata isolano il lancio dal processo padre. Non dimostrano da sole l’isolamento tra conversazioni all’interno del proxy condiviso.
- `SHINATAMA_INSTANCE`, `SHINATAMA_THREAD` e il token d’istanza sono marker locali del contratto di launch (`crates/adapter/src/launch/environment.rs:159-175`); questi non sono automaticamente header verificati sul wire BillionContext.
- Il sidecar usa un’unica radice `data/sessions` per il suo owner (`crates/engine/src/sidecar_runtime.rs:474-490`). Occorre provare l’associazione richiesta → sessione del proxy in casi concorrenti, resume, fork e prompt iniziali identici.
- Proxy/CA sono variabili del processo agente e possono raggiungere i tool figli secondo le regole dell’harness. Compatibilità con HTTP locale, MCP, shell e trust personalizzato va verificata per ricetta, non dedotta dalla sola apertura TUI.

**Proposta:** estendere l’evidence model degli adapter con una matrice `(harness build, protocollo, provider route, modello, funzione)`. Le operation correnti qualificano capacità harness, ma non contengono una capacità “compressione provider verificata” (`crates/adapter/src/capability.rs:8-38,78-85,138-163`). Mostrare “non verificato” separatamente da “non disponibile”.

## 5. Token economy: cosa misura e cosa non implementa

### Ledger presente

- Legge usage nativo Claude, Codex `last_token_usage` e Pi camelCase; normalizza input cached/noncached e deduplica i record con identificatore (`crates/token-economy/src/lib.rs:47-92,154-167`).
- Registra richieste, token input/cache read/cache write/output, compaction, tool call e ripetizioni dopo compaction. Conserva le ultime 120 righe di turno ma somma tutti i turni letti (`:14-15,104-119,174-205`).
- Il dettaglio UI mostra quei dati e dichiara che non viene rivendicato un risparmio (`apps/desktop/src/session-detail.ts:156-165`).
- Dieci sessioni per harness sono una soglia documentata e una funzione `may_claim`; la ricerca dei call site trova solo il test. Non c’è una pipeline che raccolga automaticamente un campione, confronti due politiche e autorizzi una percentuale (`crates/token-economy/src/lib.rs:209-211,266`; `docs/notes/2026-11-10-token-ledger.md:13-18`).

### Gap di misura

| Gap | Conseguenza | Evidenza |
| --- | --- | --- |
| Nessun prima/dopo della richiesta trasformata | Token consumati non equivalgono a token risparmiati | Struttura `SessionLedger`, `lib.rs:104-119` |
| Nessun costo monetario o costo delle chiamate ausiliarie | Un rewrite/sommario può consumare più del risparmio apparente | Campi `Usage`, `lib.rs:20-25` |
| Nessun parsing generico di dettagli usage annidati | Formati con cache/reasoning in sottocampi non sono normalizzati da questa funzione | `usage_from_object`, `lib.rs:48-77` |
| Nessun peso immagini, fold o recupero media | Non misura il beneficio CCR/immagini nei lavori render-and-review | `lib.rs:20-25,104-119`; nota ledger `:16-17` |
| Nessuna latenza per trasformazione o TTFT | Non misura la performance percepita del turno | Stesse strutture |
| Ripetizione ≠ errore causato da compaction | Stessa coppia nome/input è un segnale, non una causalità dimostrata | `lib.rs:174-191` |
| Record senza usage solo conteggiati | Il totale può essere parziale; non va presentato come copertura completa | `lib.rs:170-172` |

**Proposta:** aggiungere ricevute per richiesta con id pseudonimo, policy hash, versione, fase, protocollo, token stimati prima/dopo distinti da usage fatturato, costo ausiliario, cache, tempi p50/p95/p99, numero/byte immagini, CCR scritto/recuperato, fallback e motivo. Non registrare prompt o credenziali nella telemetria ordinaria.

### Planner decisionale separato e non collegato

`decision.rs` costruisce un piano deterministico su coppie tool call/result: conserva primo messaggio e recenti, segnala fallback sotto il 25% di riduzione in byte, lascia Keep gli indecisi e può rimuovere o troncare vecchi risultati (`crates/token-economy/src/decision.rs:3-11,69-89,135-179`). `apply` rimuove entrambe le metà della coppia e la troncatura invita a rieseguire il tool (`:182-210`).

**Fatto:** la ricerca di chiamate `decision::plan` / `token_economy::decision` in `crates` e `apps` non trova wiring prodotto; la nota F21 lo conferma (`docs/notes/2026-11-10-token-ledger.md:20-25`). Non è un ulteriore risparmio oggi ottenuto nelle sessioni.

**Rischi se collegato senza revisione:**

- Rimozione per stesso `tool/target` o lettura seguita da edit non dimostra che il vecchio contenuto sia inutile; queste regole precedono `cited later` (`decision.rs:147-162`).
- La protezione dei vincoli è lessicale e vale solo sotto 4.000 byte (`:84-89`): non garantisce la preservazione semantica di vincoli lunghi o in altre formulazioni.
- La riduzione è stimata sui byte del risultato, mentre il messaggio aggiunto dalla troncatura e il costo di eventuali rerun non entrano nel saldo (`:170-178,202`).
- Non è presente una copia recuperabile nel piano: «re-run» può essere caro, non ripetibile o non sicuro. Il requisito non regressivo favorisce prima un riferimento reversibile già supportato dal sistema canonico.

**Proposta:** non collegarlo come seconda compaction. Un solo proprietario della trasformazione, interfaccia del sidecar canonico o seam qualificato per harness, shadow mode, memoria reversibile e test di qualità prima di renderlo attivo; coerente con `docs/adr/ADR-0038-token-savings-per-harness.md:68-87`.

## 6. Prove esistenti: non confondere preparazione, fixture e sessioni reali

| Evidenza | Cosa dimostra | Cosa non dimostra |
| --- | --- | --- |
| C12 runtime/TLS, 2026-11-03 | CLI pinned, SQLite v9, fixture 600 CCR + 200.000 messaggi: save 51–53 ms; health max 68,198 ms; CONNECT max 73,906 ms; trust figlio | Soak lungo, tutte le trasformazioni, tutti i provider |
| C13 nativo, rieseguito 2026-11-04 | Busy conserva owner; un solo restart; stessa porta; secondo exit ferma recovery; cleanup | Cinque agenti mid-turn, final flush di tutte le sessioni, comportamento streaming/provider |
| effective_sidecar source-runtime | Resolver, mutazione steering, effort off preserva high, adaptive on/off su messaggi vuoti, rewrite con completion sintetica | Compressione lunga reale, qualità semantica, intera configurazione del CLI vivo |
| C12.4 Claude reale + origine sintetica | L2 appare sul corpo upstream di una main request | Tutti i knob, seed bypass, account/provider reale, profilo ConPTY |
| T6 storico | 22 sessioni utilizzabili; cache stabile dopo warm-up con piccolo prefisso Anthropic/Claude; costo del miss all’attivazione misurato | Risparmio in output, piena configurazione, altri harness/wire |
| Ledger/F21 | Parser e planner con fixture unit; dettaglio UI documentato | Baseline di dieci sessioni per harness o qualità dopo compaction |

Fonti `S`: `docs/verification/2026-11-03-C12-runtime-tls.md:3-26,37-39`; `docs/verification/2026-11-04-C13-supervisor-recheck.md:16-23`; `crates/engine/tests/fixtures/effective-sidecar.mjs:86-176`; `docs/verification/2026-11-04-C12.4-effective-config-live.md:37-82`; `docs/verification/2026-11-02-T6-output-steering-cache.md:20-31,35-60,73-80`; `docs/notes/2026-11-10-token-ledger.md:7-25`.

I test nativi sono esplicitamente ignorati di default e richiedono esecuzione locale deliberata (`crates/engine/tests/sidecar_runtime.rs:521-523`; `sidecar_supervisor.rs:602-604`; `effective_sidecar.rs:264-266`). Un normale `cargo test` verde non li riesegue automaticamente. La ricevuta C13 del 4 novembre è più recente dell’ADR che diceva “nothing has been run”; la revisione esatta e il perimetro vanno sempre mantenuti.

## 7. Ordine di lavoro raccomandato

| Priorità | Intervento proposto | Criterio di accettazione |
| --- | --- | --- |
| P0 | Bypass semantico esplicito seed/bootstrap/receipt nella sorgente canonica | Corpo identico, nessun rewrite/CCR/compressione/steering, anche con history vicina al limite |
| P0 | Contratto rollback delle trasformazioni | Errore, timeout, cancellazione o storage pieno prima dell’invio non lasciano stato parziale né una richiesta amputata; nessun duplicato dopo invio |
| P0 | Session isolation del proxy condiviso | Due storie con prefissi uguali, parallelismo, resume/fork e immagini non scambiano contenuti, riferimenti CCR o stato adattivo |
| P1 | UI di stato e arresto coordinata | Conseguenza sul traffico visibile; drain finito; zero richieste nuove durante drain; terminali privi di diagnostiche sidecar |
| P1 | Configurazione risolta e matrice provider | Evidenza per funzione/modello/protocollo/build, con Unknown esplicito e receipt di policy effettiva |
| P1 | Ledger completo e benchmark controllato | Baseline + costo netto + qualità + cache + latenza; nessuna percentuale derivata dai soli token consumati |
| P1 | SC16 e crash mid-stream | Cinque istanze, una ora, zero connessioni rifiutate; misure di loop/CONNECT/TTFT; final flush e persistenza verificati |
| P1 | Immagini/CCR end-to-end | Round-trip recuperabile, dettaglio necessario conservato, payload/provider corretti, nessun blob perso dopo crash/migrazione |
| P2 | Adattività più aggressiva o planner aggiuntivo | Shadow mode prima, trigger deterministico, una sola compaction, criteri di rollback e qualità non inferiore misurata |

Le modifiche al percorso di trasformazione vanno nel sorgente canonico BillionContext e poi entrano con re-pin; Shinatama dovrebbe possedere lifecycle, policy di lancio, evidenze e UI. Non introdurre un secondo proxy o una seconda compaction indipendente per “sommarne” i benefici (`sidecar/source-build/README.md:35-39`; `docs/adr/ADR-0038-token-savings-per-harness.md:68-87`).
