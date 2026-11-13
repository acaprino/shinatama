# BillionContext: audit del motore di compressione fissato in Shinatama

Data: 2026-11-13. Studio statico, nessuna modifica al prodotto, nessuna chiamata LLM, nessuna lettura di database, log o credenziali delle sessioni. Checkout Shinatama osservato: `68ebb87f154d1b4e276d6ec4987272fded7599c4`, con modifiche preesistenti lasciate intatte. Questo documento distingue **fatto nel pin**, **limite dimostrato dal codice** e **proposta da qualificare**; non misura risparmi reali.

## 1. Identità e forza delle prove

Il riferimento è **il pacchetto preparato locale**, non il master corrente di GitHub:

| Proprietà | Valore registrato |
| --- | --- |
| Package | `billion-context` `0.1.207` |
| Commit upstream registrato | `7e69b49ff4facb00b5ba6541b48d0f7fbe63d63d` |
| Source tree SHA-256 | `68d08d963cc1e081f6605677db824db62d3eddc1c3ec7126e444fdc9cd885579` |
| Artifact tree SHA-256 | `6bac74bc9ce76fdd38635adb7e6b2aa4fa536c55e6c61fd8f4a642787675f1e8` |
| Entry `dist/index.js` SHA-256 | `0c0e71df9811dbc20050fc1e9887d6da8afcefc73ff78348f1a51bcd5e9cca3e` |
| Runtime fissato | Node `24.14.1`, SHA-256 `58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f` |

Fonti: [provenance.lock.json, righe 2–23](../../sidecar/source-build/provenance.lock.json#L2), [runtime.lock.json, righe 2–15](../../sidecar/runtime.lock.json#L2). La preparazione copia output statici validati dal sorgente Musashi e non è un rebuild pulito: il README distingue esplicitamente versione, fingerprint e qualificazione runtime [README, righe 3–21](../../sidecar/source-build/README.md#L3). Il commit upstream non identifica da solo tutte le aggiunte Musashi.

Verifica svolta oggi: SHA-256 dei **27 moduli principali citati** confrontati con `sourceFiles` del lock, zero differenze; SHA-256 dell'entry preparata uguale a `entrySha256`. Sono quindi prove sul contenuto fissato. Non sono una prova di funzionamento del CLI su ogni protocollo. Il precedente test del 3 novembre prova controlli source-runtime con una completion sintetica e zero chiamate pagate, mantenendo `runningCliEffectiveConfigVerified=false` [nota C12.4, righe 1–35 e 57–75](2026-11-03-sidecar-effective-config.md#L1). I flag statici del lock non sostituiscono tali prove separate.

La baseline scritta da Shinatama abilita tiers, Absorb/CCR a 4.000 token, adaptive pressure a 160.000, prompt rewrite `auto`, output L2 e **effortRouting=false**. Non abilita la riduzione del ragionamento [sidecar_config.rs, righe 153–175](../../crates/engine/src/sidecar_config.rs#L153).

## 2. Cosa riduce davvero ciascun livello

| Meccanismo | Comportamento nel pin | Limite da non confondere con una garanzia |
| --- | --- | --- |
| CCR | Sostituisce risultati tool grandi con riferimento e anteprima; conserva il testo originale | Il modello deve decidere di recuperare; dipende dal canale tool e dalla persistenza |
| Absorb | Fa produrre al modello una sintesi e nasconde la coppia chiamata/risultato | Sintesi semantica con perdita; nel resolver Bili è `mediaOnly=true` per default |
| Tiers | T1 riassume contenuto, T2 distilla blocchi T1, T3 condensa blocchi T2 | `tiers=true` abilita un percorso, non dimostra risparmio né buona sintesi |
| Adaptive pressure/epoche | Decide quando riscrivere il passato e conserva la stessa vista dentro un'epoca | Soglie e quantità, non classificazione semantica dell'intento né ottimizzazione economica completa |
| Output steering | Aggiunge istruzioni di concisione; effort è una leva separata | Può impoverire spiegazioni o prosa richiesta se il livello è inadatto |
| Prompt rewrite | Aggiunge una nota di richiesta riformulata senza sostituire l'originale | Può aumentare input, costo e latenza; non è compressione del prompt |

Fonti dei comportamenti: [CCR, righe 24–43 e 478–558](../../sidecar/source-build/prepared/kernel/src/ccr.ts#L24), [Absorb resolver, righe 272–286](../../sidecar/source-build/prepared/src/compress-settings.ts#L272), [Absorb, righe 119–134 e 374–399](../../sidecar/source-build/prepared/kernel/src/absorb.ts#L119), [tiers, righe 246–258](../../sidecar/source-build/prepared/kernel/src/compress.ts#L246), [epoche, righe 1–5](../../sidecar/source-build/prepared/src/musashi-terminal/epoch.ts#L1), [steering, righe 124–129 e 307–319](../../sidecar/source-build/prepared/kernel/src/output-steering.ts#L124), [rewrite wire, righe 68–100](../../sidecar/source-build/prepared/src/musashi-terminal/rewrite-wire.ts#L68).

La pipeline kernel ordinaria è: riconciliazione/ref → sync/prune → snapshot → **CCR** → hide Absorb → crush → prompt Absorb → filter → nascondi compress → view latch → recommend/nudge → emergency truncate [compress.ts, righe 947–963](../../sidecar/source-build/prepared/kernel/src/compress.ts#L947). CCR precede Absorb; un placeholder già memorizzato non è candidato Absorb [absorb.ts, righe 98–115](../../sidecar/source-build/prepared/kernel/src/absorb.ts#L98). Non sommare percentuali ipotetiche dei due livelli: spesso agiscono sullo stesso materiale in modo mutuamente esclusivo.

## 3. CCR: vantaggi reali e limite decisivo per Codex

Il default kernel è spento, soglia 4.000 token e anteprima di 96 caratteri. Il nodo tratta **solo `tool-result`** con testo, call ID e riferimento valido, esclude tool ACP/retrieve, tool configurati esclusi e messaggi protetti. I messaggi user ordinari non vengono esternalizzati da questo passaggio [ccr.ts, righe 55–65 e 478–527](../../sidecar/source-build/prepared/kernel/src/ccr.ts#L55).

La proiezione è deterministica: una ritrasmissione del testo grezzo con lo stesso ref riproduce il placeholder già memorizzato; non riespande silenziosamente il contesto [ccr.ts, righe 513–558](../../sidecar/source-build/prepared/kernel/src/ccr.ts#L513). La proprietà è favorevole alla cache del prefisso, ma 96 caratteri iniziali possono non rivelare una conclusione importante in fondo: **rischio di visibilità**, non perdita del testo archiviato. Va misurato con risultati in cui errore/esito essenziale è alla fine.

`acp_retrieve` restituisce dati esplicitamente marcati non attendibili. Da 4.000 token in su, quando il chiamante fornisce la directory export, restituisce un file da leggere a pagine. La versione host usa tale directory; se la scrittura fallisce ritorna un errore e una preview, non un percorso fittiziamente riuscito [ccr.ts, righe 282–329 e 403–430](../../sidecar/source-build/prepared/kernel/src/ccr.ts#L282), [store.ts, righe 178–211](../../sidecar/source-build/prepared/src/store.ts#L178). Il risparmio netto dipende anche da quanto spesso l'agente recupera e rilegge gli originali.

Il codice **disarma CCR dove il round trip non è disponibile**, anche con `enabled=true` nella configurazione:

| Corsia | Gate effettivo nel pin |
| --- | --- |
| Proxy Anthropic/OpenAI/Google | Può armarsi con `compress.injectTool` e CCR risolto attivo |
| Proxy Responses, host diverso da Codex | Richiede tool injection, nessun `ACP_NO_INJECT_TOOL`, nessun `FORCE_TEXT_PROTOCOL`, protocollo compress diverso da marker |
| Responses riconosciuto come **Codex host** | **CCR disarmato**: il router Codex rifiuta tool Bili iniettati; i risultati grandi restano verbatim fino al fold ordinario |
| Plugin Anthropic/OpenAI | Ammesso, ma segue **l'intero blocco CCR globale**: gli override provider/modello divergenti non governano questa corsia |
| Plugin Google/Responses | Disarmato dalla whitelist plugin |

Fonte dei gate: [handle.ts, righe 1759–1802](../../sidecar/source-build/prepared/src/server/handle.ts#L1759); whitelist e allineamento config/stamp: [store.ts, righe 88–111](../../sidecar/source-build/prepared/src/store.ts#L88). Non generalizzare «CCR on per ogni harness» dal JSON base. Occorre osservare **l'effectiveCcr della sessione dopo il gate**, con motivo di disarmo. Per i tool proxy Google/Responses esistono superfici specifiche [prepare-google.ts, riga 296](../../sidecar/source-build/prepared/src/server/prepare-google.ts#L296), [prepare-responses.ts, riga 414](../../sidecar/source-build/prepared/src/server/prepare-responses.ts#L414).

Persistenza: `contentStoreOf` preferisce il mirror unified, poi il file legacy, infine uno store nuovo [store.ts, righe 114–129](../../sidecar/source-build/prepared/src/store.ts#L114). La sincronizzazione CCR usa differenze dei refcount nella transazione, con sei indici schema v9 fissati dal lock [storage/store.ts, righe 589–635 e 1023](../../sidecar/source-build/prepared/src/storage/store.ts#L589), [lock, righe 26–36](../../sidecar/source-build/provenance.lock.json#L26). Per i fold, `storeCoveredOriginals` conserva gli originali coperti ma esclude reasoning e placeholder senza originale: evita falsi recuperi del placeholder stesso [ccr.ts, righe 603–657](../../sidecar/source-build/prepared/kernel/src/ccr.ts#L603). Il TTL `ccrRetrievalTtlMs` appartiene alla vecchia coda di consegna; non equivale al TTL dello store CCR: il percorso attuale consegna nel tool result senza accodamento [store.ts, righe 206–209 e 332–338](../../sidecar/source-build/prepared/src/store.ts#L206).

## 4. Absorb e tiers: attenzione alla semantica

Nel **kernel** `mediaOnly` non è fissato dal default; è il resolver **Bili** a impostarlo a `true`. È una distinzione importante quando si leggono documentazione upstream e sorgente kernel separatamente [absorb.ts, righe 35–46](../../sidecar/source-build/prepared/kernel/src/absorb.ts#L35), [compress-settings.ts, righe 281–285](../../sidecar/source-build/prepared/src/compress-settings.ts#L281).

Con tale default solo risultati tool con payload media sono candidati e bypassano `minToolTokens`: il conteggio del solo testo non contiene i byte immagine. L'invito impone all'agente di sintetizzare subito; anche la funzione esecutiva rifiuta un risultato privo di media. Una sintesi non più corta produce un warning, non una prova di economia [absorb.ts, righe 61–75, 188–197, 326–332 e 397–399](../../sidecar/source-build/prepared/kernel/src/absorb.ts#L61). **Proposta:** evitare `mediaOnly=false` come scorciatoia generale per risparmiare; servono prove separate su qualità e costo dei turni supplementari.

I tiers sono sintesi di sintesi. Le soglie di default per conteggio sono 1.000 blocchi T1 e 2.000 blocchi T2: il codice le descrive come percorso per conteggio sostanzialmente disattivato nel caso ordinario; resta il percorso per massa di token/crescita. In pressione si sceglie il tier con maggiore materiale recuperabile, subordinato al beneficio minimo [kernel/config.ts, riga 8](../../sidecar/source-build/prepared/kernel/src/config.ts#L8), [compress.ts, righe 1766–1783 e 1815–1883](../../sidecar/source-build/prepared/kernel/src/compress.ts#L1766). `tiers=true` non significa «comprimere continuamente ogni N messaggi».

## 5. Intento, profili ed epoche

Il pin conosce soltanto **`analysis` e `build`**, oltre alla selezione `auto`. Non esiste un profilo `writing`. I cinque campi ammessi sono `maxContextLimit`, `emergencyThresholdPercent`, `nudgeGrowthTokens`, `preserveRecentMessages`, `preserveRecentTokens`; profili non ammessi/campi sconosciuti vengono scartati. Profili non controllano direttamente CCR, rewrite, immagini o output steering [context-profiles.ts, righe 7–17 e 63–89](../../sidecar/source-build/prepared/src/musashi-terminal/context-profiles.ts#L7).

| Profilo predefinito | Soglia nominale | Recenti | Token recenti | Crescita nudge |
| --- | --- | --- | --- | --- |
| analysis | 65% | 12 | 16.000 | 30.000 |
| build | 45% | 6 | 6.000 | 15.000 |

Fonte: [context-profiles.ts, righe 36–43](../../sidecar/source-build/prepared/src/musashi-terminal/context-profiles.ts#L36). Selezione da modalità forzata, header plugin, blocco **nativo** Codex Plan/Default, coppie Claude EnterPlanMode/ExitPlanMode; conflitto e assenza di evidenza ripiegano su analysis. Il testo dell'utente, reasoning e documenti non sono segnali. L'overlay è globale → profilo → provider → modello, quindi forzare il profilo non vince sugli override successivi [context-profiles.ts, righe 1–6, 115–141 e 184–217](../../sidecar/source-build/prepared/src/musashi-terminal/context-profiles.ts#L1).

Conseguenza: un lavoro di scrittura in Codex Default può essere classificato `build` nonostante l'intento umano. **Proposta:** tenere distinto l'intento della sessione dall'harness mode e aggiungere in futuro uno schema esplicito versionato; non inventare oggi `profiles.writing` nel JSON credendo che sia consumato.

Adaptive pressure richiede `enabled` e una capacità del modello riconosciuta; con capacità fallback/unknown/default torna al kernel ordinario. La curva usa `config.nudge.maxContextLimitPct` e `maxTokens`, dopo riserva output e sicurezza: `maxTokens=160000` è il limite asintotico della curva, **non** una finestra del modello e **non** un trigger rigido sempre a 160.000 [adaptive-turn.ts, righe 64–85](../../sidecar/source-build/prepared/src/musashi-terminal/adaptive-turn.ts#L64), [adaptive-pressure.ts, righe 23–33](../../sidecar/source-build/prepared/src/musashi-terminal/adaptive-pressure.ts#L23).

`targetRatio` e `growthWindow` sono parsati e validati ma non consumati dal percorso analizzato; Shinatama li ha già rimossi dai controlli espliciti della baseline. `epochTokens` e `cacheTtlSeconds` sono invece letti. Il passo dell'epoca è quello configurato, oppure metà soglia con minimo 32.000 token; cache TTL predefinito 3.600 s più margine di 60 s; emergenza a 85% della finestra [adaptive-pressure.ts, righe 3–20](../../sidecar/source-build/prepared/src/musashi-terminal/adaptive-pressure.ts#L3), [adaptive-turn.ts, righe 85–100](../../sidecar/source-build/prepared/src/musashi-terminal/adaptive-turn.ts#L85), [epoch.ts, righe 26–57](../../sidecar/source-build/prepared/src/musashi-terminal/epoch.ts#L26), [baseline, righe 171–174](../../crates/engine/src/sidecar_config.rs#L171).

Dentro l'epoca il passato mantiene la stessa vista, i fold accettati possono attendere in `pending`, e la coda riceve solo ref, CCR e inviti Absorb all'arrivo. Cambiare policy a ogni turno o moltiplicare compattatori rischia di vanificare tale disciplina [epoch.ts, righe 1–5 e 43–49](../../sidecar/source-build/prepared/src/musashi-terminal/epoch.ts#L1), [adaptive-turn.ts, righe 123–154 e 173–178](../../sidecar/source-build/prepared/src/musashi-terminal/adaptive-turn.ts#L123). Il messaggio umano corrente, il suo gruppo wire e i system sono protetti nel percorso adattivo [request-protection.ts, righe 4–14](../../sidecar/source-build/prepared/src/musashi-terminal/request-protection.ts#L4).

## 6. Output steering: numeri più alti significano più concisione

| Livello | Direttiva effettiva |
| --- | --- |
| 0 | Nessuna direttiva di verbosità |
| 1 | Omettere preambolo e ricapitolazione |
| 2 | Anche evitare ripetizione di codice/file/diff/output, citare path e linee, non narrare ogni tool riuscito |
| 3 | Conclusioni senza motivazione salvo richiesta; prosa minima, edit piccoli; mantenere negazioni e dettagli necessari |
| 4 | Token minimi, frammenti ammessi, niente motivazione; eccezioni per azioni distruttive, sicurezza e ambiguità |

Fonte: [kernel/output-steering.ts, righe 120–129](../../sidecar/source-build/prepared/kernel/src/output-steering.ts#L120). L2 è la baseline Shinatama. L3/L4 **non** sono livelli di risposta più approfondita e sono inadatti come automatismo per una richiesta di analisi estesa o prosa articolata. La direttiva è inserita/reimpiazzata in modo idempotente alla coda del prompt system/instructions [kernel/output-steering.ts, righe 159–179](../../sidecar/source-build/prepared/kernel/src/output-steering.ts#L159), [src/output-steering.ts, righe 232–241](../../sidecar/source-build/prepared/src/output-steering.ts#L232).

La classificazione del turno è strutturale, basata sull'ultimo messaggio: nuovo testo/immagine/documento user → nuova domanda; soli tool result → continuazione meccanica, salvo errore. Non decide se il lavoro sia analisi/scrittura/coding. La verbosità configurata si applica indipendentemente da tale categoria; la categoria governa il solo abbassamento effort quando questa leva è abilitata [kernel/output-steering.ts, righe 55–88 e 307–319](../../sidecar/source-build/prepared/kernel/src/output-steering.ts#L55). Il default kernel di `effortRouting` è `true`, ma Shinatama lo imposta esplicitamente `false`: preservare questa impostazione durante merge/refactor [kernel/output-steering.ts, righe 223–235](../../sidecar/source-build/prepared/kernel/src/output-steering.ts#L223), [baseline, riga 175](../../crates/engine/src/sidecar_config.rs#L175).

## 7. Prompt rewrite: qualità potenziale, costo reale

Non riduce il prompt originale: aggiunge una nota user adiacente, ripetuta con gli stessi byte nei turni successivi; il messaggio umano, immagini e snapshot nativi restano. Il contratto dice espressamente che il risultato può essere più lungo e che accorciare non è lo scopo [rewrite-wire.ts, righe 68–100](../../sidecar/source-build/prepared/src/musashi-terminal/rewrite-wire.ts#L68), [prompt-rewrite-contract.ts, righe 5–11](../../sidecar/source-build/prepared/src/musashi-terminal/prompt-rewrite-contract.ts#L5).

L'ammissione locale richiede almeno 120 caratteri, 20 parole, massimo 4.096 byte e assenza di sintassi eseguibile/fence/tag. `auto` è limitato ai main request di Claude/Codex/OpenCode/Pi/Gemini, esclude child. Il cap di costo stimato predefinito è $0,01; la deadline complessiva è al massimo 3 s. Modello selezionato dal catalogo autorizzato, ammissione persistita **prima** del dispatch, risultati e deduplica persistenti evitano ricomprare la stessa decisione dopo retry [validation.ts, righe 22–31](../../sidecar/source-build/prepared/src/musashi-terminal/prompt-rewrite-validation.ts#L22), [rewrite-hook.ts, righe 20–33](../../sidecar/source-build/prepared/src/musashi-terminal/rewrite-hook.ts#L20), [rewrite-settings.ts, righe 4–16](../../sidecar/source-build/prepared/src/musashi-terminal/rewrite-settings.ts#L4), [prompt-rewrite.ts, righe 91, 149–176 e 178–213](../../sidecar/source-build/prepared/src/musashi-terminal/prompt-rewrite.ts#L149).

La validazione rifiuta tool call, incompletezza, wrapper non valido, eccesso di dimensione, risultato invariato/cosmetico, sintassi eseguibile e differenze dei literal riconosciuti. **Non prova equivalenza semantica completa** di negazioni, autorizzazioni o sfumature; queste sono richieste al modello nel contratto [validation.ts, righe 68–84](../../sidecar/source-build/prepared/src/musashi-terminal/prompt-rewrite-validation.ts#L68), [contract, riga 7](../../sidecar/source-build/prepared/src/musashi-terminal/prompt-rewrite-contract.ts#L7). Questo è un limite della validazione, non un caso osservato di alterazione.

**Proposta:** valutare `auto` come funzione di qualità separata dal risparmio, con contabilizzazione propria e corpus di equivalenza per italiano/negazioni/intento. I percorsi letti non espongono un bypass dedicato chiamato seed/bootstrap/receipt; l'esclusione generica dei tag XML non sostituisce una prova end-to-end dei byte del trasferimento. Il requisito esiste già in [FRD-0009, righe 49–51](../frd/FRD-0009-billioncontext-sidecar.md#L49).

## 8. Immagini: eliminare lavoro inutile senza promettere recupero eterno

Il vecchio `compress.stripImages` è inerte e `compress.imageCompression` è rimosso; il parser avverte. Non sono manopole utilizzabili [config.ts, righe 2433–2474](../../sidecar/source-build/prepared/src/config.ts#L2433). Esiste invece `imageRescale`, on per default, dimensione massima predefinita 1.600 pixel. Il passaggio egress si attiva sopra 100 KiB o sopra la dimensione, adotta la ricodifica solo se risparmia almeno 20%, usa JPEG qualità 80, tratta PNG/JPEG/BMP e lascia passare gli altri formati. È deterministico e non riscrive gli originali [knobs.ts, righe 548–571](../../sidecar/source-build/prepared/src/knobs.ts#L548), [image-rescale.ts, righe 1–48 e 146–168](../../sidecar/source-build/prepared/src/image-rescale.ts#L1).

**Limite del bundle locale:** il modulo importa dinamicamente `jimp` esterno; se manca, lo disabilita per il processo e inoltra gli originali. Il manifest preparato dichiara soltanto `zod` come dipendenza runtime e il `node_modules` preparato osservato contiene soltanto `zod`; anche l'entry compilata conserva `import("jimp")`. Quindi la configurazione `imageRescale=true` **non prova il ridimensionamento effettivo** in un'installazione isolata. Non è stato eseguito un test di risoluzione runtime e non si esclude una dipendenza ambientale esterna, che sarebbe comunque da evitare per un bundle riproducibile [image-rescale.ts, righe 20–25 e 91–99](../../sidecar/source-build/prepared/src/image-rescale.ts#L20), [package.json, righe 45–47](../../sidecar/source-build/prepared/package.json#L45), [lock, righe 21–24](../../sidecar/source-build/provenance.lock.json#L21).

`acp_read_original_image` è registrato solo dove il gate consente il tool; Codex riceve annotazioni dimensionali senza quell'invocazione. Il rescale può invece operare anche su tale corsia, se l'encoder è disponibile [handle.ts, righe 1803–1813](../../sidecar/source-build/prepared/src/server/handle.ts#L1803), [store.ts, righe 61–79](../../sidecar/source-build/prepared/src/store.ts#L61). **Proposta:** rendere visibili encoder e canale di recupero effettivi; non attivare una riduzione più aggressiva solo perché il toggle risulta on.

Il recupero dei fold passa da `decompress({ imageRef })`, con file leggibile dal tool della macchina. Si indicizzano i pixel presenti nella richiesta inbound per ref, con spill su disco e metadati in memoria; URL senza bytes e immagini già eliminate dallo store locale del plugin non sono recuperabili tramite questo indice [image-restore.ts, righe 1–18 e 249–303](../../sidecar/source-build/prepared/src/image-restore.ts#L1), [decompress-shared.ts, righe 291–312](../../sidecar/source-build/prepared/src/decompress-shared.ts#L291).

Tre limiti concreti di durabilità:

1. L'indice è **latest-wins per richiesta**, non un catalogo storico permanente; se il nuovo body non contiene immagini diventa vuoto [handle.ts, righe 2334–2355](../../sidecar/source-build/prepared/src/server/handle.ts#L2334).
2. I file restore scadono per `mtime` dopo una settimana; scrittura skip-if-exists non aggiorna quel timestamp. L'indicizzazione precede il prune periodico, ma **il successivo refresh post-prepare reindicizza il body e può ricreare il file**. Non è quindi corretto dedurre da quel primo ordine un errore del percorso normale. Vanno provati TTL e corsie senza refresh, oltre al caso in cui i pixel non sono più nel body [image-restore.ts, righe 306–333, 336–355 e 434–444](../../sidecar/source-build/prepared/src/image-restore.ts#L306), [handle.ts, righe 2347–2352 e 2487–2495](../../sidecar/source-build/prepared/src/server/handle.ts#L2347).
3. Unified storage conserva il testo dei messaggi ritirati, ma rilascia esplicitamente i relativi riferimenti media CAS; il read-original-image consulta l'indice e il file, senza fallback CAS in questa funzione, e legge la prima immagine del ref [storage/store.ts, righe 755–779](../../sidecar/source-build/prepared/src/storage/store.ts#L755), [image-rescale.ts, righe 602–611](../../sidecar/source-build/prepared/src/image-rescale.ts#L602).

**Proposta:** prima ridurre catture inutili alla sorgente, poi usare fold/Absorb per materiale già esaminato; richiedere prove di recupero dopo reload, rewind, TTL e più immagini nello stesso messaggio. La semplice esistenza di storage unified non dimostra recuperabilità perpetua dei pixel. Le prove devono distinguere byte su disco, indice risolvibile, tool disponibile e contenuto che arriva davvero al modello.

## 9. Prezzi, prestazioni e gate di pubblicazione

Il pin ha un report economico che calcola costo iniziale, risparmio per turno, break-even e rimborso del fold, con prezzi configurati o modello registry. La scelta `priceProfile` è esplicitamente **report-only, senza impatto sul trigger** [handle.ts, righe 2316–2330](../../sidecar/source-build/prepared/src/server/handle.ts#L2316), [cache-report.ts, righe 262–301](../../sidecar/source-build/prepared/kernel/src/cache-report.ts#L262). Non attribuire al pin una politica che compressa solo quando conviene in denaro.

Il gate effettivo di beneficio confronta byte delle viste prima/dopo, includendo wrapper e stima del thinking nascosto. Con adaptive on richiede `4 * max(1000, ceil(min(beforeBytes / 4, maxTokens) * 0.05))` byte; con adaptive off almeno un byte. Non include direttamente costo della chiamata LLM, probabilità di riuso, latenza o prezzo cache [compression-benefit.ts, righe 32–61](../../sidecar/source-build/prepared/src/musashi-terminal/compression-benefit.ts#L32). La pubblicazione rifiuta un fold sotto il minimo senza cambiare prefisso [stream.ts, riga 975](../../sidecar/source-build/prepared/src/stream.ts#L975).

**Proposte di qualifica prioritarie**, non implementate da questo studio:

| Priorità | Prova concreta | Criterio di accettazione |
| --- | --- | --- |
| P0 | Matrice harness × wire × plugin/proxy × tool injection | Stato CCR e recovery corrispondente al gate reale, nessun placeholder irrecuperabile |
| P0 | Restart/fork/rewind con store CCR e media | Testo recuperato esatto; limiti pixel espliciti; nessun falso recupero del placeholder |
| P0 | Bundle isolato senza dipendenze ambientali, immagini >1.600px | Encoder deterministico presente oppure status di pass-through; recupero disponibile dichiarato |
| P0 | Invecchiamento file restore oltre 7 giorni e nuova richiesta | Nessun indice che promette un file già eliminato; esito osservabile |
| P1 | Source fixtures per intenti e steering | Writing non trattato implicitamente come coding; L3/L4 non sopprimono dettagli richiesti |
| P1 | Rewrite su negazioni, condizioni, autorizzazioni e trasferimenti | Nessuna alterazione dell'intento; costo/latency ausiliari conteggiati separatamente |
| P1 | Replay sintetico con CCR/fold/Absorb uno alla volta | Misure di input/output/cache/retrieval, latenza e correttezza; nessuna percentuale non misurata |
| P1 | Finestre diverse, immagini pesanti, modalità adattiva on/off | Soglie rispettano la finestra reale; verificare l'interazione perché `epoch.tokensOf` conta il testo, mentre il budget wire tratta anche immagini |

Per l'ultima riga: [epoch.ts, righe 61–64](../../sidecar/source-build/prepared/src/musashi-terminal/epoch.ts#L61) e [handle.ts, righe 1403–1410](../../sidecar/source-build/prepared/src/server/handle.ts#L1403). È una richiesta di prova d'interazione, non una conclusione di overflow esistente.

La direzione più solida è usare il sidecar come unico proprietario di proiezione e recupero, esporre capacità **effettive per sessione**, stabilizzare policy e fingerprint, poi misurare. Aggressività più alta non è una dimostrazione di economia o stabilità. Ogni cambiamento al motore deve entrare nella sorgente canonica e nel lock, non come patch locale sul percorso richiesta [README, righe 33–37](../../sidecar/source-build/README.md#L33), [ADR-0029, righe 67–89](../adr/ADR-0029-billioncontext-sidecar.md#L67).
