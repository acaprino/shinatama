# Piano completo di implementazione

Generato da `plan.json` e `progress.json` con `python scripts/implementation.py render`.
Le specifiche FRD restano l'autorità sui requisiti; il registro di verifica resta l'autorità sulle prove.

## Ripresa immediata

Eseguire `python scripts/implementation.py status`, poi leggere la scheda del passo indicato.
Aggiornare `progress.json` alla fine di ogni sessione; `verified` significa incremento verificato sulle piattaforme registrate, non tutti i gate FRD superati.

## Regole comuni per ogni incremento

1. Verificare prerequisiti e modifiche locali; delimitare i file del passo. Nessuno `git add .` in un checkout condiviso.
2. Implementare una capacità utilizzabile; mantenere avviabile il programma e mostrare come indisponibili le capacità non provate.
3. Eseguire le prove della scheda, inclusi rifiuti/errori rilevanti. Registrare versione, OS e risultato; i gate attivi si chiudono su Windows, e nessuna prova vale per macOS o Linux, che restano rinviati.
4. Aggiornare evidenza, checkpoint e mappa; selezionare solo i file/hunk del passo e creare il commit indicato dopo le verifiche.
5. Riprendere dal primo passo pronto. Per annullare un passo già committato usare un commit di revert; le migrazioni richiedono backup e compatibilità, mai rollback distruttivo del DB.

I comandi nelle schede future sono verifiche pianificate: i relativi test e comandi devono essere creati nel passo prima di poter dichiarare il risultato.
Le stime sono ore di lavoro tecnico, includono implementazione/verifica/revisione e non costituiscono date di consegna; si aggiornano con le prove delle integrazioni.


## Rinvii richiesti dall’utente

Daodan e temporaneamente fuori dallo scope di implementazione su richiesta dell utente. Installazione, task/onboarding, handover task, avvisi widget e seam dipendenti restano tracciati nei passi deferred e si riprendono solo su nuova richiesta. Checkup, set generici, avvio agenti, ricerca, hop, worktree e roundtable con harness proseguono senza dipendenze Daodan.

macOS e Linux sono fuori dallo scope su decisione dell utente: solo Windows e supportato ufficialmente e non ci sono macchine per provarli. I passi F11 e QA di quei sistemi restano tracciati come deferred; CI, TL1, QA, pacchetti e accettazione V1 valgono su Windows e nessun passo attivo richiede un altro sistema.

## Sequenza e dipendenze

| Passo | Risultato | Prerequisiti | Stato | Ore |
| --- | --- | --- | --- | --- |
| [B00](#b00) | Registrare e rendere riproducibile il prototipo di terminale senza cornice | — | committed | 2–4 |
| [B01](#b01) | Piano completo, copertura FRD e checkpoint riprendibile | B00 | committed | 3–6 |
| [B02](#b02) | Ricordare e riaprire i progetti recenti senza avviare processi | B00, B01 | committed | 8–16 |
| [F32CI](#f32ci) | Verifica continua iniziale, lockfile e avvisi sulle dipendenze | B01 | committed | 3–8 |
| [TL1-proof](#tl1-proof) | Provare la shell Tauri su Windows (gate TL1) | B00, F32CI | committed | 6–16 |
| [C01](#c01) | Validare i percorsi locali privati prima di creare il database | B00, B02, F32CI | committed | 8–16 |
| [C02](#c02) | Aprire SQLite incorporato in sicurezza ed esporre un servizio di stato tipizzato | C01, B02 | committed | 8–16 |
| [C03](#c03) | Aggiungere migrazioni, copie di sicurezza e lettura dei database più recenti | C02 | committed | 6–12 |
| [C03M](#c03m) | Mantenere finiti pulizia, validazione e crescita dello stato locale | C03, F32CI | committed | 18–30 |
| [C04](#c04) | Configurare le radici e individuare i progetti con un selettore nativo | B02, C03 | committed | 10–20 |
| [C05](#c05) | Creare progetti in modo atomico sotto la radice scelta | C04 | in_progress | 8–16 |
| [C10](#c10) | Definire contratti degli adattatori, capacità e verifiche di versione | C02, C03 | pending | 10–18 |
| [C11](#c11) | Costruire piani di avvio sicuri e cartelle private degli agenti | C01, C10 | pending | 16–32 |
| [C12](#c12) | Integrare il sorgente del sidecar e il suo ambiente privato | C11 | pending | 20–40 |
| [C09R](#c09r) | Proteggere l host con budget e recupero adattivi | C03, FS03, C11, C03M | pending | 24–48 |
| [C13](#c13) | Supervisionare i sidecar distinguendo occupazione e recupero | C12, C09R | pending | 20–40 |
| [C14](#c14) | Preparare checkup, manifest e set generici di avvio | C11, C13 | pending | 6–12 |
| [FS03](#fs03) | Separazione degli schemi autorevoli e operativi e record dei progetti | C03, C11 | pending | 4–10 |
| [C06](#c06) | Introdurre un modello validato ad albero per le suddivisioni | B02, FS03, TL1-proof | pending | 10–20 |
| [C07](#c07) | Disegnare l'albero di pannelli e completare i comandi da tastiera | C06 | pending | 15–30 |
| [C08](#c08) | Rendere configurabili i temi e i profili versionati dei terminali | C07 | pending | 12–20 |
| [C09](#c09) | Rendere robusti terminale e contenimento con fixture sintetiche | C07, C08, C09R | pending | 8–16 |
| [C15](#c15) | Avviare il primo adattatore Claude attraverso il checkup | C09, C10, C11, C13, C14 | pending | 12–24 |
| [QA-claude-windows](#qa-claude-windows) | Certificare terminale claude su windows | C09, C15 | pending | 4–8 |
| [C22](#c22) | Risolvere le radici persistenti degli archivi e collegare una revisione verificata di Mukade | C03, C11, C15 | pending | 14–28 |
| [C23](#c23) | Costruire il dettaglio delle sessioni e trascrizioni canoniche limitate | C22, C07 | pending | 18–36 |
| [C24](#c24) | Indicizzare e cercare le sessioni in modo incrementale | C23 | pending | 12–24 |
| [F02](#f02) | Proprietà del runtime, flusso degli eventi e rilevamento della presenza | FS03, C15, C11 | pending | 4–10 |
| [C25](#c25) | Aggiungere thread persistenti e ripresa esclusiva nello stesso agente | C22, C24, FS03, F02 | pending | 12–24 |
| [C26](#c26) | Aggiungere esportazioni e CLI stabile per le sessioni | C23, C24, C25 | pending | 12–24 |
| [F03P](#f03p) | Controlli di riservatezza, doctor e ricostruzione degli indici operativi | FS03, C23, C24 | pending | 4–10 |
| [C16](#c16) | Aggiungere l'adattatore Codex | C15 | pending | 12–24 |
| [C17](#c17) | Aggiungere l'adattatore OpenCode | C15 | pending | 12–24 |
| [C18](#c18) | Aggiungere l'adattatore Pi | C15 | pending | 12–24 |
| [C19](#c19) | Aggiungere Hermes Agent con capacità non supportate esplicite | C15 | pending | 14–28 |
| [C20](#c20) | Aggiungere il pannello ACP DeepSeek | C15 | pending | 20–40 |
| [C27](#c27) | Creare anteprime seed esatte e oscurate | C23, C25, C26, C13 | pending | 16–32 |
| [C28](#c28) | Implementare il coordinatore degli hop close/fork con registro persistente | C27 | pending | 20–40 |
| [C29](#c29) | Aggiungere recupero degli hop, aggiornamento, ritorno e standby misurato | C28 | pending | 20–40 |
| [C30](#c30) | Completare la matrice degli hop ordinati tra i sei agenti | C19, C20, C29, C16, C17, C18 | pending | 16–32 |
| [F03H](#f03h) | Prove delle transazioni hop, dell’unicità e del recupero dopo arresto anomalo | F03P, C29 | pending | 3–8 |
| [C31](#c31) | Costruire l'API di messaggistica limitata e circoscritta tra istanze | C25, C27, F02 | pending | 14–28 |
| [C32](#c32) | Esporre quattro strumenti MCP e hook iniettati all'avvio | C31, C11, C14, C16, C17, C18, C19, C20 | pending | 18–36 |
| [C33](#c33) | Consegnare una tavola rotonda headless con limiti rigidi | C32, C13, C24, C28, C29 | pending | 20–40 |
| [C34](#c34) | Mostrare lo stato Daodan in sola lettura e digitare i comandi | C14, C25, C32, D01 | deferred | 16–32 |
| [C35](#c35) | Trasferire il contesto delle attività Daodan dopo un hop close completato | C29, C34 | deferred | 8–16 |
| [C36](#c36) | Aggiungere preferiti, metadati git e avvio in worktree collegati | C25 | pending | 12–24 |
| [C39](#c39) | Vincolare importazione e trasferimento nativo post-v1 alle verifiche | C30, C26 | pending | 24–48 |
| [C21](#c21) | Aggiungere Gemini per il solo avvio | C13, C14, C15 | pending | 6–12 |
| [C37](#c37) | Aggiungere installazione, aggiornamento e ripristino dei programmi gestiti | C14, C15, C16, C17, C18, C19, C20 | pending | 16–32 |
| [C41](#c41) | Aggiungere navigazione limitata e in sola lettura nei file del progetto | C07, C04 | pending | 10–18 |
| [C42](#c42) | Aggiungere lettura sicura di Markdown e modifica del sorgente | C41 | pending | 16–32 |
| [C43](#c43) | Abilitare la modifica visuale Markdown solo dopo le verifiche di fedeltà | C42 | pending | 16–32 |
| [C44](#c44) | Aggiungere anteprime degli artefatti e lettura DOCX verificata separatamente | C41 | pending | 12–24 |
| [C45](#c45) | Vincolare browser incorporati e mini-app post-v1 a verifiche separate | C07, C44 | pending | 24–48 |
| [F04](#f04) | Hook di stato nativi e prima panoramica delle istanze | F02, C15 | pending | 4–10 |
| [F05](#f05) | Utilizzo, dati git, collisioni e cronologia della collaborazione | F04, C23, C29 | pending | 4–10 |
| [F06](#f06) | Titoli testuali originali con impostazioni, esclusioni e limiti di costo | F03P, F04 | pending | 2–6 |
| [F07](#f07) | Runtime condiviso per modelli locali e download facoltativi | F06, C13 | pending | 6–16 |
| [F08](#f08) | Titoli facoltativi dei provider dai cataloghi degli account autenticati | F06, C13, C14 | pending | 3–8 |
| [F09](#f09) | Protocollo delle schede companion, azioni predefinite e rendering sicuro | C42, C11 | pending | 4–10 |
| [F10](#f10) | Companion R1: integrazione Claude su richiesta | F09, C15, C13, F04 | pending | 6–16 |
| [F11-windows](#f11-windows) | Confinamento del companion a livello di sistema operativo: Windows | F10 | pending | 6–20 |
| [F12](#f12) | Companion R2: flusso automatico dei turni e continuità dopo gli hop | F10, C23, C29, F04 | pending | 5–12 |
| [F13](#f13) | Companion R3: memoria personale, quiz ed esportazione | F12, FS03 | pending | 5–12 |
| [F14-codex](#f14-codex) | Adattatore host del companion: Codex | F10, C15, C16, F11-windows | pending | 4–12 |
| [F14-opencode](#f14-opencode) | Adattatore host del companion: OpenCode | F10, C15, C17, F11-windows | pending | 4–12 |
| [F14-pi](#f14-pi) | Adattatore host del companion: Pi | F10, C15, C18, F11-windows | pending | 4–12 |
| [F14-hermes](#f14-hermes) | Adattatore host del companion: Hermes Agent | F10, C15, C19, F11-windows | pending | 4–12 |
| [F14-deepseek](#f14-deepseek) | Adattatore host del companion: DeepSeek Harness | F10, C15, C20, F11-windows | pending | 4–12 |
| [F15](#f15) | Companion R4: ricerca web senza lettura delle pagine per gli harness idonei | F10, F13, C15, F11-windows | pending | 5–14 |
| [F16](#f16) | Companion R5: lettura delle pagine limitata ai risultati | F15 | pending | 4–10 |
| [F17](#f17) | Contratto delle richieste browser e instradamento immutabile | C11, C13, C32 | pending | 4–10 |
| [F18](#f18) | Moli: estrazione singola con limiti e anteprima statica | F17, C42 | pending | 6–16 |
| [F19](#f19) | Browser Patchright esplicito e ciclo di vita con un solo controllore | F17, C13 | pending | 8–20 |
| [F20](#f20) | Registro dei token per sessione, riferimento iniziale e regole di proprietà | C23, C13 | pending | 4–12 |
| [F21](#f21) | Compattazione decisionale Claude basata soltanto su regole | F20, C15, C13 | pending | 6–16 |
| [F22](#f22) | Scoring incrementale julia1 su stato esteso solo in coda | F07, F21 | pending | 4–12 |
| [F23-codex](#f23-codex) | Adattatore di compattazione decisionale: Codex | F22, C15, C13, C16 | pending | 4–14 |
| [F23-opencode](#f23-opencode) | Adattatore di compattazione decisionale: OpenCode | F22, C15, C13, C17 | pending | 4–14 |
| [F23-pi](#f23-pi) | Adattatore di compattazione decisionale: Pi | F22, C15, C13, C18 | pending | 4–14 |
| [F23-hermes](#f23-hermes) | Adattatore di compattazione decisionale: Hermes Agent | F22, C15, C13, C19 | pending | 4–14 |
| [F23-deepseek](#f23-deepseek) | Adattatore di compattazione decisionale: DeepSeek Harness | F22, C15, C13, C20 | pending | 4–14 |
| [F24](#f24) | Sintesi asincrone degli strumenti e originali recuperabili | F20, F22, C13 | pending | 4–12 |
| [F25](#f25) | Instradamento locale del modello all’avvio della sessione | F07, F20, C15, C14 | pending | 5–14 |
| [F26](#f26) | Consenso alle decisioni ospitate, chiave nel vault e integrazioni dei sottoagenti | F25, C11 | pending | 5–14 |
| [C40](#c40) | Verificare i confini delle credenziali nelle chiamate ausiliarie, nel vault del router e nei browser | C13, C15, F26, F19 | pending | 8–16 |
| [F27](#f27) | Selezione degli strumenti differiti senza invalidare la cache | F25, C15 | pending | 4–12 |
| [F28](#f28) | Installazione facoltativa TokenEconomizer e rimozione degli strumenti solo su proposta | F20, C15, C13 | pending | 6–16 |
| [F29](#f29) | TokenEconomizer: envfacts e protezione dai cicli subordinata alle verifiche | F28, F07 | pending | 4–12 |
| [F30](#f30) | Widget finale della V1: prova per piattaforma e avvisi di consultazione | F02, F05, F06, C29, C15, F16, F19, F24, F27, F29, C30, C33, C36, C37, C43, C44 | pending | 6–16 |
| [F31](#f31) | Widget: risposte rapide legate alla richiesta e pipeline delle animazioni revisionata | F30, C11 | pending | 6–16 |
| [F32PKG](#f32pkg) | Programmi di installazione, fornitura dei runtime e primo hop su sistema pulito | F32CI, C29, C14, C13, C16 | pending | 6–18 |
| [F33](#f33) | Aggiornamenti facoltativi e nuova verifica delle dipendenze vincolate | F32PKG, C15 | pending | 5–14 |
| [FE01](#fe01) | Valutazione post-V1 del gateway code-mode | F20 | pending | 4–16 |
| [C38](#c38) | Valutare le varianti aperte di roundtable e il rinvio del gateway | C32, C33, FE01 | pending | 8–16 |
| [FE02](#fe02) | Valutazione successiva dell’escalation tra modelli | F25, F20 | pending | 4–16 |
| [QSET-claude](#qset-claude) | Provare set e tool reali per claude | C14, C15 | pending | 3–6 |
| [QSET-codex](#qset-codex) | Provare set e tool reali per codex | C14, C16 | pending | 3–6 |
| [QA-codex-windows](#qa-codex-windows) | Certificare terminale codex su windows | C09, C16 | pending | 4–8 |
| [QSET-opencode](#qset-opencode) | Provare set e tool reali per opencode | C14, C17 | pending | 3–6 |
| [QA-opencode-windows](#qa-opencode-windows) | Certificare terminale opencode su windows | C09, C17 | pending | 4–8 |
| [QSET-pi](#qset-pi) | Provare set e tool reali per pi | C14, C18 | pending | 3–6 |
| [QA-pi-windows](#qa-pi-windows) | Certificare terminale pi su windows | C09, C18 | pending | 4–8 |
| [QSET-hermes](#qset-hermes) | Provare set e tool reali per hermes | C14, C19 | pending | 3–6 |
| [QA-hermes-windows](#qa-hermes-windows) | Certificare terminale hermes su windows | C09, C19 | pending | 4–8 |
| [QSET-deepseek](#qset-deepseek) | Provare set e tool reali per deepseek | C14, C20 | pending | 3–6 |
| [QA-deepseek-windows](#qa-deepseek-windows) | Certificare il pannello ACP DeepSeek su windows | C09, C20 | pending | 4–8 |
| [F32SIGN](#f32sign) | Artefatti di rilascio firmati e notarizzazione | F32PKG, QSET-claude, QA-claude-windows, QSET-codex, QA-codex-windows, QSET-opencode, QA-opencode-windows, QSET-pi, QA-pi-windows, QSET-hermes, QA-hermes-windows, QSET-deepseek, QA-deepseek-windows | pending | 4–12 |
| [V1-acceptance](#v1-acceptance) | Verificare e distribuire il candidato dello scope attivo | B00, C01, C02, C03, B02, C04, C05, C06, C07, C08, C09, C10, C11, C12, C13, C14, C15, C16, C17, C18, C19, C20, C21, C22, C23, C24, C25, C26, C27, C28, C29, C30, C31, C32, C33, C36, C37, C40, C41, C42, C43, C44, FS03, F02, F03P, F03H, F04, F05, F06, F07, F08, F09, F10, F11-windows, F12, F13, F14-codex, F14-opencode, F14-pi, F14-hermes, F14-deepseek, F15, F16, F17, F18, F19, F20, F21, F22, F23-codex, F23-opencode, F23-pi, F23-hermes, F23-deepseek, F24, F25, F26, F27, F28, F29, F30, F31, F32CI, F32PKG, F32SIGN, F33, B01, QSET-claude, QA-claude-windows, QSET-codex, QA-codex-windows, QSET-opencode, QA-opencode-windows, QSET-pi, QA-pi-windows, QSET-hermes, QA-hermes-windows, QSET-deepseek, QA-deepseek-windows, C09R | pending | 8–16 |
| [D01](#d01) | Integrare pacchetti e marketplace Daodan | C14, C15 | deferred | 4–8 |
| [D02](#d02) | Collegare i partecipanti bare-model della roundtable a Daodan | C33, D01 | deferred | 4–8 |
| [D03](#d03) | Mostrare gli avvisi dei task Daodan nel widget | C34, F30 | deferred | 4–8 |
| [D04](#d04) | Riprendere le seam Daodan in companion, router e misure token | D01, F10, F25, F29 | deferred | 4–8 |
| [QA-claude-macos](#qa-claude-macos) | Certificare terminale claude su macos | C09, C15 | deferred | 4–8 |
| [QA-claude-linux](#qa-claude-linux) | Certificare terminale claude su linux | C09, C15 | deferred | 4–8 |
| [F11-macos](#f11-macos) | Confinamento del companion a livello di sistema operativo: macOS | F10 | deferred | 6–20 |
| [F11-linux](#f11-linux) | Confinamento del companion a livello di sistema operativo: Linux | F10 | deferred | 6–20 |
| [QA-codex-macos](#qa-codex-macos) | Certificare terminale codex su macos | C09, C16 | deferred | 4–8 |
| [QA-codex-linux](#qa-codex-linux) | Certificare terminale codex su linux | C09, C16 | deferred | 4–8 |
| [QA-opencode-macos](#qa-opencode-macos) | Certificare terminale opencode su macos | C09, C17 | deferred | 4–8 |
| [QA-opencode-linux](#qa-opencode-linux) | Certificare terminale opencode su linux | C09, C17 | deferred | 4–8 |
| [QA-pi-macos](#qa-pi-macos) | Certificare terminale pi su macos | C09, C18 | deferred | 4–8 |
| [QA-pi-linux](#qa-pi-linux) | Certificare terminale pi su linux | C09, C18 | deferred | 4–8 |
| [QA-hermes-macos](#qa-hermes-macos) | Certificare terminale hermes su macos | C09, C19 | deferred | 4–8 |
| [QA-hermes-linux](#qa-hermes-linux) | Certificare terminale hermes su linux | C09, C19 | deferred | 4–8 |
| [QA-deepseek-macos](#qa-deepseek-macos) | Certificare il pannello ACP DeepSeek su macos | C09, C20 | deferred | 4–8 |
| [QA-deepseek-linux](#qa-deepseek-linux) | Certificare il pannello ACP DeepSeek su linux | C09, C20 | deferred | 4–8 |

<a id="b00"></a>
## B00 · Registrare e rendere riproducibile il prototipo di terminale senza cornice

Un punto di ripristino riproducibile del prototipo Tauri/xterm/Rust già realizzato, con i limiti attuali documentati.

**Ambito:** v1. **Dipendenze:** nessuna. **Stima:** 2–4 ore.

**File/componenti:** `apps/desktop/**`; `crates/terminal/**`; `Cargo.toml`; `Cargo.lock`; `package*.json`; `docs/implementation/tauri-prototype.md`.

**Accettazione osservabile:**

- Registrare file sorgenti, lockfile, comandi di compilazione e limiti attuali; preservare il lavoro estraneo a questo incremento.
- La compilazione e la prova desktop devono passare prima del punto di ripristino; il vincolo della sola UI nativa deve riportare l'eccezione Tauri autorizzata.

**Verifiche del passo:** `npm ci && npm run build`; `cargo test -p shinatama-terminal`; `npm run test:desktop`.

**Gate collegati, da registrare separatamente:** partial TL2/TL5/P1/P9/P14 only.

**Requisiti coperti dal piano:** FRD-0001/R12, FRD-0001/R14, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R22, FRD-0001/R26, FRD-0001/R27, FRD-0001/R29, FRD-0001/R33, FRD-0001/R34, FRD-0001/R56.

**Commit proposto:** `feat(b00): Registrare e rendere riproducibile il prototipo di terminale senza cornice`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/**, crates/terminal/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="b01"></a>
## B01 · Piano completo, copertura FRD e checkpoint riprendibile

Piano leggibile e macchina, dipendenze senza cicli, ogni requisito assegnato e stato distinto fra pianificato, verificato e committato. La ripresa identifica una sola azione pronta.

**Ambito:** v1. **Dipendenze:** B00. **Stima:** 3–6 ore.

**File/componenti:** `docs/implementation/PLAN.md`; `docs/implementation/plan.json`; `docs/implementation/progress.json`; `docs/implementation/README.md`; `scripts/implementation.py`; `scripts/test_implementation.py`.

**Accettazione osservabile:**

- Tutti i requisiti dei 17 FRD Markdown sono assegnati ad almeno un passo.
- Validazione rifiuta riferimenti mancanti, cicli, due passi attivi, prove assenti e commit senza SHA reale.
- La scheda di ogni passo specifica risultato utilizzabile, ambito, file, verifiche, gate, stima e commit.

**Verifiche del passo:** `python scripts/implementation.py validate`; `python -m unittest discover -s scripts -p test_implementation.py`; `python scripts/implementation.py status`.

**Gate collegati, da registrare separatamente:** nessun gate intero chiuso da questo passo.

**Requisiti coperti dal piano:** FRD-0017/R17, FRD-0017/R18, FRD-0017/R19, FRD-0017/R20, FRD-0017/R21, FRD-0017/R22.

**Commit proposto:** `docs(b01): Piano completo, copertura FRD e checkpoint riprendibile`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire docs/implementation/PLAN.md, docs/implementation/plan.json. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="b02"></a>
## B02 · Ricordare e riaprire i progetti recenti senza avviare processi

Cronologia di dieci cartelle recenti, filtro e ultima cartella ripristinata nella configurazione utente, senza avvio automatico. Selezione validata e atomica; importazione in SQLite prevista in C02/C03.

**Ambito:** v1. **Dipendenze:** B00, B01. **Stima:** 8–16 ore.

**File/componenti:** `crates/projects/**`; `apps/desktop/src/project-picker.ts`; `apps/desktop/src/main.ts`; `apps/desktop/src/styles.css`; `apps/desktop/index.html`; `apps/desktop/src-tauri/src/main.rs`; `apps/desktop/src-tauri/Cargo.toml`; `Cargo.toml`; `Cargo.lock`; `package.json`; `apps/desktop/tests/projects-smoke.mjs`; `apps/desktop/tests/desktop-smoke.mjs`; `docs/verification/2026-11-02-B02-project-history.md`.

**Accettazione osservabile:**

- Gli alias dello stesso percorso canonico producono una sola voce recente; ordinamento per ultima apertura e progetto selezionato sopravvivono al riavvio tramite il file di configurazione della piattaforma.
- Le cartelle mancanti restano visibili come non disponibili; una cartella non valida non crea un record di progetto.
- La selezione cambia soltanto la cartella dei terminali futuri e non avvia o riprende terminali.
- Mostrare inizialmente un elenco limitato di progetti recenti; il comportamento esatto delle ultime dieci cartelle non standard viene completato con le radici configurate in C04.

**Verifiche del passo:** `cargo test -p shinatama-projects`; `npm run build`; `npm run test:projects`; `npm run test:desktop`.

**Gate collegati, da registrare separatamente:** PM1 partial, PM7, PM8, S9 partial.

**Requisiti coperti dal piano:** FRD-0004/R01, FRD-0004/R03, FRD-0004/R06, FRD-0004/R19, FRD-0004/R20, FRD-0004/R21, FRD-0004/R22, FRD-0006/R47, FRD-0014/R25.

**Commit proposto:** `feat(b02): Ricordare e riaprire i progetti recenti senza avviare processi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/projects/**, apps/desktop/src/project-picker.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f32ci"></a>
## F32CI · Verifica continua iniziale, lockfile e avvisi sulle dipendenze

CI su Windows, dipendenze riproducibili, inventario delle licenze e documentazione coerente fin dai primi incrementi di codice.

**Ambito:** v1. **Dipendenze:** B01. **Stima:** 3–8 ore.

**File/componenti:** `.github/workflows/verify.yml`; `.node-version`; `rust-toolchain.toml`; `scripts/verify.ps1`; `scripts/check-process.ps1`; `scripts/test_verify.ps1`; `scripts/dependencies.py`; `scripts/test_dependencies.py`; `THIRD_PARTY_NOTICES.md`; `docs/implementation/windows-verification.md`; `docs/implementation/dependency-policy.md`; `docs/verification/2026-11-02-F32CI-windows-verification.md`.

**Accettazione osservabile:**

- Registrare i comandi di build e test su Windows; macOS e Linux sono rinviati e non hanno job di CI.
- Centralizzare ogni dipendenza Rust diretta e generare gli avvisi per le risorse incluse.
- Aggiornare prove, registro, README e mappa del codice a ogni passo completato, senza inventare verifiche superate.
- Non inviare telemetria né richieste di aggiornamento o catalogo per impostazione iniziale.

**Verifiche del passo:** `pwsh -NoProfile -File scripts/verify.ps1`; `python scripts/dependencies.py check --offline`.

**Gate collegati, da registrare separatamente:** P17, P18, X3, X4, X5.

**Requisiti coperti dal piano:** FRD-0017/R06, FRD-0017/R07, FRD-0017/R08, FRD-0017/R09, FRD-0017/R13, FRD-0017/R14, FRD-0017/R17, FRD-0017/R18, FRD-0017/R19, FRD-0017/R20, FRD-0017/R21, FRD-0017/R22.

**Commit proposto:** `feat(f32ci): Verifica continua iniziale, lockfile e avvisi sulle dipendenze`.

**Prima azione alla ripresa:** Aprire .github/workflows/verify.yml (da creare) e preparare il job Windows con build e test attuali; aggiungere audit delle dipendenze dirette e registrare i risultati realmente eseguiti.

<a id="tl1-proof"></a>
## TL1-proof · Provare la shell Tauri su Windows (gate TL1)

Prova della shell Tauri su Windows con un vero agente in PTY, pannelli annidati, Markdown sorgente e anteprima e navigazione file, con la versione di WebView2 e i limiti registrati.

**Ambito:** v1. **Dipendenze:** B00, F32CI. **Stima:** 6–16 ore.

**File/componenti:** `apps/desktop/tests/tl1/`; `docs/verification/`; `docs/verification-register.md`; `docs/adr/`.

**Accettazione osservabile:**

- La build avvia un vero agente digitato nel terminale, senza adattatore, e passa annidamento, Markdown e file senza Node, server di sviluppo o browser esterno.
- La versione di WebView2 e i limiti sono registrati; il comportamento su una macchina senza WebView2 è provato o indicato come lacuna.
- Un fallimento produce una nuova versione di ADR-0005 prima di C06; non si cambia toolkit in silenzio.

**Verifiche del passo:** `npm run build`; `npm run test:desktop`; `planned: fixture TL1 su Windows con agente reale e account di prova autorizzato`.

**Gate collegati, da registrare separatamente:** TL1, P17.

**Requisiti coperti dal piano:** FRD-0001/R22, FRD-0001/R51.

**Commit proposto:** `test(tl1): Provare la shell Tauri su Windows`.

**Prima azione alla ripresa:** Aprire apps/desktop/tests/ e preparare la fixture TL1 con pannelli annidati, Markdown e file; eseguirla prima su Windows con un agente reale digitato nel terminale.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### TL1.1 · Costruire la fixture TL1 e provarla su Windows

Fixture non distribuita con pannelli annidati, Markdown, file e agente reale in PTY.

Stima: 4–10 ore. File: `apps/desktop/tests/tl1/`.

Accettazione: La fixture passa su Windows con un agente reale; il codice non entra nell'interfaccia distribuita.

Verifiche pianificate: `planned: fixture TL1 su Windows con agente reale`.

Commit: `test(tl1.1): Costruire la fixture TL1 e provarla su Windows`. Registrare stato e prove sotto `progress.json` → `steps` → `TL1-proof` → `units` → `TL1.1`.

### TL1.2 · Registrare il gate e proporre gli ADR dipendenti

Riga TL1 aggiornata e nuove versioni Proposed di ADR-0002 (Mermaid) e ADR-0040 (EX-001).

Stima: 2–6 ore. File: `docs/verification-register.md`, `docs/adr/`.

Accettazione: Il registro riporta l'esito reale; le revisioni restano Proposed finché l'utente non ne approva il testo.

Verifiche pianificate: `python "adr_tool.py" validate --project .`.

Commit: `docs(tl1.2): Registrare TL1 e proporre gli ADR dipendenti`. Registrare stato e prove sotto `progress.json` → `steps` → `TL1-proof` → `units` → `TL1.2`.

<a id="c01"></a>
## C01 · Validare i percorsi locali privati prima di creare il database

Un modulo che risolve la cartella dati della piattaforma e SHINATAMA_HOME, applica permessi riservati all'utente ed esclude percorsi di progetto o volumi non locali.

**Ambito:** v1. **Dipendenze:** B00, B02, F32CI. **Stima:** 8–16 ore.

**File/componenti:** `crates/state/src/location.rs`; `crates/state/tests/location.rs`.

**Accettazione osservabile:**

- Il database predefinito si trova fuori da qualsiasi progetto e la diagnostica ne mostra il percorso esatto sulla piattaforma.
- I percorsi sincronizzati, di rete o interni al progetto riconosciuti vengono rifiutati con un errore che indica come procedere; nessun ripiego silenzioso e nessun file creato nel percorso rifiutato.
- Permessi e ACL vengono verificati su ogni sistema supportato; le verifiche mancanti restano dichiarate.

**Verifiche del passo:** `path policy fixtures incl UNC/symlinks/OneDrive/DriveFS`; `owner-permission inspection`.

**Gate collegati, da registrare separatamente:** D15, E4, S11, S2, S9.

**Requisiti coperti dal piano:** FRD-0002/R16, FRD-0002/R41, FRD-0014/R05, FRD-0014/R06, FRD-0014/R09, FRD-0014/R25, FRD-0014/R26, FRD-0014/R31.

**Commit proposto:** `feat(c01): Validare i percorsi locali privati prima di creare il database`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/src/location.rs, crates/state/tests/location.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c02"></a>
## C02 · Aprire SQLite incorporato in sicurezza ed esporre un servizio di stato tipizzato

Schema minimo per progetti e impostazioni, FTS5 incorporato, WAL e transazioni tipizzate brevi, eseguite fuori dal thread della UI.

**Ambito:** v1. **Dipendenze:** C01, B02. **Stima:** 8–16 ore.

**File/componenti:** `crates/state/src/lib.rs`; `crates/state/src/schema.rs`; `Cargo.toml`.

**Accettazione osservabile:**

- I valori effettivi di WAL, NORMAL, foreign_keys e busy_timeout vengono riletti e verificati.
- Letture e scritture concorrenti preservano i dati; gli errori SQLite busy/full sono visibili e non causano perdita di record.
- Nessuna IPC SQL generica e nessuno schema vuoto di funzionalità future; testi sensibili e credenziali non finiscono nei log.
- Importare una sola volta la cronologia di configurazione B02 senza perdere percorsi o ordine; conservare il vecchio file fino alla verifica della migrazione completata.

**Verifiche del passo:** `cargo test -p shinatama-state`; `synthetic 4-reader/4-writer stress and integrity_check`.

**Gate collegati, da registrare separatamente:** S1, S4, S4 partial, S5.

**Requisiti coperti dal piano:** FRD-0014/R07, FRD-0014/R08, FRD-0014/R10, FRD-0014/R13, FRD-0014/R14, FRD-0014/R15, FRD-0014/R26.

**Commit proposto:** `feat(c02): Aprire SQLite incorporato in sicurezza ed esporre un servizio di stato tipizzato`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/src/lib.rs, crates/state/src/schema.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c03"></a>
## C03 · Aggiungere migrazioni, copie di sicurezza e lettura dei database più recenti

Migrazioni incorporate e versionate, copia di sicurezza verificata prima di ogni migrazione e apertura in sola lettura degli schemi più recenti non supportati.

**Ambito:** v1. **Dipendenze:** C02. **Stima:** 6–12 ore.

**File/componenti:** `crates/state/src/migrations.rs`; `crates/state/tests/migrations.rs`.

**Accettazione osservabile:**

- Database nuovi, vecchi, già migrati, più recenti e corrotti vengono gestiti come casi distinti.
- Nessun percorso di ripristino o ricreazione elimina dati autorevoli.
- Un arresto durante la migrazione annulla la transazione, conserva la copia di sicurezza e supera integrity_check.

**Verifiche del passo:** `migration fixture tests`; `kill-at-migration fault test`.

**Gate collegati, da registrare separatamente:** S1, S4, S5.

**Requisiti coperti dal piano:** FRD-0014/R07, FRD-0014/R08, FRD-0014/R10, FRD-0014/R11, FRD-0014/R12.

**Commit proposto:** `feat(c03): Aggiungere migrazioni, copie di sicurezza e lettura dei database più recenti`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/src/migrations.rs, crates/state/tests/migrations.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c03m"></a>
## C03M · Mantenere finiti pulizia, validazione e crescita dello stato locale

Manutenzione locale con policy finite, rapporto tipizzato, quarantena senza payload, proprietà atomica fra finestre e integrazione con la pressione C09R; storia autorevole conservata.

**Ambito:** v1. **Dipendenze:** C03, F32CI. **Stima:** 18–30 ore.

**File/componenti:** `crates/state/src/maintenance.rs`; `crates/state/tests/maintenance.rs`; `apps/desktop/src-tauri/src/storage.rs`; `docs/implementation/2026-11-02-database-maintenance.md`; `docs/verification/`.

**Accettazione osservabile:**

- La specifica di manutenzione del 2026-11-02 governa soltanto shinatama.db e artefatti Shinatama; nessun job tocca archivi agenti o database sidecar.
- Batch con quote di righe/byte/VM e scadenza cooperativa; rinvii visibili, nessuna promessa rigida sui tempi delle chiamate OS. SQLITE_INTERRUPT da budget non equivale a corruzione.
- Proprietà atomica, lock rilasciato dopo crash, schema futuro senza scritture e pressione che rinvia il lavoro opzionale. Nessun ciclo infinito di retry.
- Record autorevoli e backup verificati non vengono cancellati automaticamente; anomalie aggregate e report di crescita hanno tetti fissi. Le feature future registrano policy operative prima di abilitare i propri dati.
- Prove di guasto, spazio insufficiente, contesa, ripetizioni e un terminale sano con latenza misurata; nessun gate si chiude con i soli test unitari.

**Verifiche del passo:** `cargo test -p shinatama-state --test maintenance`; `planned: fault/recovery and real healthy-terminal responsiveness under maintenance`; `python scripts/implementation.py validate`.

**Gate collegati, da registrare separatamente:** S4, S5, I8, X5, RES1, TL5.

**Requisiti coperti dal piano:** FRD-0014/R33, FRD-0014/R34, FRD-0014/R35, FRD-0014/R36, FRD-0014/R37, FRD-0014/R24.

**Commit proposto:** `feat(c03m): Limitare manutenzione e crescita dello stato locale`.

**Prima azione alla ripresa:** Aprire la specifica di manutenzione e il core maintenance.rs; verificare l ultimo commit C03, poi eseguire il prossimo batch o la prossima prova senza toccare archivi degli agenti.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C03M.1 · Definire policy e nucleo finito della manutenzione

Policy numeriche proposte, rapporto tipizzato, controllo della crescita e anomalie senza modificare storia autorevole.

Stima: 6–10 ore. File: `crates/state/src/maintenance.rs`, `crates/state/tests/maintenance.rs`, `docs/implementation/2026-11-02-database-maintenance.md`.

Accettazione: Quote per batch e interruzioni restituiscono lavoro rinviato, non una falsa diagnosi di corruzione. Righe autorevoli e payload sensibili restano intatti; anomalie ripetute hanno spazio limitato.

Verifiche pianificate: `cargo test -p shinatama-state --test maintenance`; `planned: measured Windows integration qualification`.

Commit: `feat(c03m.1): Definire policy e nucleo finito della manutenzione`. Registrare stato e prove sotto `progress.json` → `steps` → `C03M` → `units` → `C03M.1`.

### C03M.2 · Coordinare job inattivi e pressione fra finestre

Scheduler fuori UI, proprietà atomica e lease crash-safe, manutenzione ammessa da budget e collegamento C09R.

Stima: 6–10 ore. File: `crates/state/src/maintenance.rs`, `apps/desktop/src-tauri/src/storage.rs`.

Accettazione: Due finestre non sovrappongono job e un crash non lascia il lavoro bloccato senza scadenza. Schema futuro non riceve scritture; pressione rinvia i job; checkpoint e compattazione rispettano le proprie capacità reali.

Verifiche pianificate: `cargo test -p shinatama-state --test maintenance`; `planned: measured Windows integration qualification`.

Commit: `feat(c03m.2): Coordinare job inattivi e pressione fra finestre`. Registrare stato e prove sotto `progress.json` → `steps` → `C03M` → `units` → `C03M.2`.

### C03M.3 · Qualificare guasti e recuperi ripetuti della manutenzione

Fixture isolate di crash, lock, pressione e storage failure con misure reali di un terminale sano.

Stima: 6–10 ore. File: `crates/state/tests/maintenance.rs`, `verify/RES/`, `docs/verification/`.

Accettazione: Prima/dopo: dati autorevoli conservati, integrity_check superato e nessuna crescita residua illimitata di metadata/quarantena. Un terminale reale sano continua a rispondere; scadenze cooperative e limiti OS vengono descritti senza garanzie inventate.

Verifiche pianificate: `cargo test -p shinatama-state --test maintenance`; `planned: measured Windows integration qualification`.

Commit: `feat(c03m.3): Qualificare guasti e recuperi ripetuti della manutenzione`. Registrare stato e prove sotto `progress.json` → `steps` → `C03M` → `units` → `C03M.3`.

<a id="c04"></a>
## C04 · Configurare le radici e individuare i progetti con un selettore nativo

Più radici, radice predefinita modificabile, individuazione delle cartelle di primo livello, filtro / con focus, selettore nativo F3 e cronologia delle ultime dieci cartelle non standard.

**Ambito:** v1. **Dipendenze:** B02, C03. **Stima:** 10–20 ore.

**File/componenti:** `crates/state/src/projects.rs`; `apps/desktop/src/project-picker.ts`; `apps/desktop/src-tauri/src/projects.rs`.

**Accettazione osservabile:**

- Centinaia di cartelle e un disco lento o rimosso non bloccano l'input; annullare lascia invariata la selezione.
- I progetti mai aperti sono ordinati per data di modifica della cartella; quelli già aperti per attività in Shinatama.
- La cronologia contiene soltanto cartelle esterne alle radici e all'undicesimo inserimento elimina esattamente la voce meno recente.

**Verifiche del passo:** `synthetic large-root and slow-scan fixtures`; `native folder chooser on Windows`; `keyboard focus test`.

**Gate collegati, da registrare separatamente:** PM1, PM6, PM7, PM8, U3.

**Requisiti coperti dal piano:** FRD-0004/R02, FRD-0004/R04, FRD-0004/R05, FRD-0004/R06, FRD-0004/R11, FRD-0004/R19, FRD-0004/R20, FRD-0004/R22.

**Commit proposto:** `feat(c04): Configurare le radici e individuare i progetti con un selettore nativo`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/src/projects.rs, apps/desktop/src/project-picker.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C04.1 · Salvare radici e radice predefinita

Gestione persistente delle radici con selezione esplicita della predefinita.

Stima: 3–6 ore. File: `crates/state/src/projects.rs`, `apps/desktop/src/project-picker.ts`.

Accettazione: Aggiunta, modifica, rimozione e radice mancante hanno fixture; nessuna scansione o creazione avviene implicitamente. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test di persistenza radici e radice rimossa`; `npm run build`.

Commit: `feat(c04.1): Salvare radici e radice predefinita`. Registrare stato e prove sotto `progress.json` → `steps` → `C04` → `units` → `C04.1`.

### C04.2 · Elencare e filtrare i progetti delle radici

Scansione asincrona di primo livello con ordinamento per uso o modifica e filtro con focus.

Stima: 4–8 ore. File: `crates/projects/src/discovery.rs`, `apps/desktop/src/project-picker.ts`.

Accettazione: Una scansione lenta si annulla; centinaia di cartelle non bloccano il terminale e i risultati obsoleti vengono scartati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: corpus radici grandi, scansioni lente e filtro`; `npm run build`.

Commit: `feat(c04.2): Elencare e filtrare i progetti delle radici`. Registrare stato e prove sotto `progress.json` → `steps` → `C04` → `units` → `C04.2`.

### C04.3 · Collegare F3 al selettore nativo e limitare la cronologia

Selettore cartelle multipiattaforma con annullamento e ultime dieci cartelle esterne alle radici.

Stima: 3–6 ore. File: `apps/desktop/src-tauri/src/projects.rs`, `apps/desktop/src/project-picker.ts`.

Accettazione: Annullare conserva il progetto; alias non duplicano voci e l’undicesima cartella esterna elimina la meno recente. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: prova selettore e cronologia standard/non-standard`; `npm run build`.

Commit: `feat(c04.3): Collegare F3 al selettore nativo e limitare la cronologia`. Registrare stato e prove sotto `progress.json` → `steps` → `C04` → `units` → `C04.3`.

<a id="c05"></a>
## C05 · Creare progetti in modo atomico sotto la radice scelta

Finestra di creazione del progetto, mkdir validato e git init opzionale, con valori predefiniti espliciti e annullamento limitato ai file temporanei creati dall'operazione.

**Ambito:** v1. **Dipendenze:** C04. **Stima:** 8–16 ore.

**File/componenti:** `crates/projects/src/create.rs`; `apps/desktop/src/new-project.ts`.

**Accettazione osservabile:**

- Nomi riservati dal sistema, punti finali, duplicati senza distinzione tra maiuscole e minuscole e percorsi non validi vengono rifiutati.
- Un errore in mkdir o git init non lascia un progetto incompleto e non modifica cartelle preesistenti.
- Il tasto n apre la creazione fuori dal filtro incrementale; la creazione del progetto richiede un'azione dell'utente.

**Verifiche del passo:** `name-validation corpus on Windows`; `fault injection during mkdir/git init`.

**Gate collegati, da registrare separatamente:** PM4, U3.

**Requisiti coperti dal piano:** FRD-0004/R12, FRD-0004/R13, FRD-0004/R15.

**Commit proposto:** `feat(c05): Creare progetti in modo atomico sotto la radice scelta`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/projects/src/create.rs, apps/desktop/src/new-project.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c10"></a>
## C10 · Definire contratti degli adattatori, capacità e verifiche di versione

Il motore compila senza adattatori o con adattatori finti; programmi individuati e capacità non supportate sono espliciti in doctor e nella revisione dell'avvio.

**Ambito:** v1. **Dipendenze:** C02, C03. **Stima:** 10–18 ore.

**File/componenti:** `crates/adapter/**`; `crates/engine/**`; `crates/cli/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- Individuare eseguibile e argomenti senza avviare .cmd o leggere segreti; riutilizzare il catalogo Mukade dove la revisione verificata lo consente.
- Versioni sconosciute o non conformi diventano Degraded, con motivo e rifiuto delle singole capacità.
- Scegliere l'architettura di collegamento supportata prima di implementare ciascun adattatore.

**Verifiche del passo:** `fake-adapter contract tests`; `synthetic installation/version fixtures`; `--help/--version only discovery probes`.

**Gate collegati, da registrare separatamente:** E1, E5, CX6, HM2, DS1, D18.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R02, FRD-0001/R03, FRD-0001/R08, FRD-0001/R54, FRD-0002/R04, FRD-0002/R05, FRD-0002/R06, FRD-0002/R10, FRD-0002/R11, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R43, FRD-0006/R44, FRD-0006/R45, FRD-0006/R46, FRD-0006/R48.

**Commit proposto:** `feat(c10): Definire contratti degli adattatori, capacità e verifiche di versione`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/**, crates/engine/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C10.1 · Definire il contratto e l’adattatore finto

Interfacce del motore e capacità tipizzate, compilabili con zero adattatori e con un adattatore in memoria.

Stima: 4–8 ore. File: `crates/adapter/**`, `crates/engine/**`.

Accettazione: Le capacità non dichiarate rifiutano esplicitamente; fixture offline non avviano agenti o modelli. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test contratto con zero/fake adapter`; `npm run build`.

Commit: `feat(c10.1): Definire il contratto e l’adattatore finto`. Registrare stato e prove sotto `progress.json` → `steps` → `C10` → `units` → `C10.1`.

### C10.2 · Individuare programmi e applicare i vincoli di versione

Catalogo di installazioni con eseguibile e argomenti, versione verificata e motivi Degraded.

Stima: 6–10 ore. File: `crates/adapter/src/catalog.rs`, `crates/cli/src/doctor.rs`.

Accettazione: Nessun avvio .cmd o lettura di segreti; versioni sconosciute conservano soltanto le capacità effettivamente provate. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture installazioni, versioni e capacità rifiutate`; `npm run build`.

Commit: `feat(c10.2): Individuare programmi e applicare i vincoli di versione`. Registrare stato e prove sotto `progress.json` → `steps` → `C10` → `units` → `C10.2`.

<a id="c11"></a>
## C11 · Costruire piani di avvio sicuri e cartelle private degli agenti

Un solo costruttore del piano di avvio risolve collocazione, ambiente del figlio, rimozione esatta dei marcatori, ACL delle cartelle private, identità con token e argomenti verificati.

**Ambito:** v1. **Dipendenze:** C01, C10. **Stima:** 16–32 ore.

**File/componenti:** `crates/adapter/src/launch/**`; `crates/engine/src/launch.rs`; `crates/state/src/store_roots.rs`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** La clausola di installazione Daodan nella home system di FRD-0002/R19 e rinviata; la scelta della home resta attiva.

- La combinazione programma gestito e cartella utente di sistema è sempre rifiutata; cambiare collocazione non sposta dati di sessione.
- I file di credenziali non vengono aperti o copiati; le variabili di configurazione e home sopravvivono alla rimozione dei soli marcatori esatti.
- Le particolarità di home, file temporanei e cache di Codex, OpenCode, Pi, Hermes e DeepSeek sono verificate per versione; i percorsi di isolamento non supportati vengono rifiutati.
- UI, CLI e ripresa usano tutti lo stesso punto di avvio con controllo preliminare.

**Verifiche del passo:** `launch-plan pure fixture tests`; `marker names-only nested launch probes`; `box leak/ACL tests in synthetic homes`.

**Gate collegati, da registrare separatamente:** D15, D17, D18, P15, P16, P20, M1, CX12, HM9, DS9, SS5.

**Requisiti coperti dal piano:** FRD-0002/R01, FRD-0002/R02, FRD-0002/R03, FRD-0002/R10, FRD-0002/R11, FRD-0002/R12, FRD-0002/R13, FRD-0002/R14, FRD-0002/R15, FRD-0002/R16, FRD-0002/R17, FRD-0002/R18, FRD-0002/R19, FRD-0002/R20, FRD-0002/R30, FRD-0002/R31, FRD-0002/R32, FRD-0002/R33, FRD-0002/R34, FRD-0002/R35, FRD-0002/R36, FRD-0002/R37, FRD-0002/R38, FRD-0002/R39, FRD-0002/R40, FRD-0002/R41, FRD-0003/R03, FRD-0003/R10, FRD-0003/R11, FRD-0003/R14, FRD-0003/R17, FRD-0003/R20, FRD-0004/R18.

**Commit proposto:** `feat(c11): Costruire piani di avvio sicuri e cartelle private degli agenti`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/launch/**, crates/engine/src/launch.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C11.1 · Applicare la rimozione esatta dei marcatori ereditati

Costruttore dell’ambiente del figlio con liste per adattatore e rimozioni condizionali.

Stima: 3–6 ore. File: `crates/adapter/src/launch/environment.rs`.

Accettazione: Le fixture distinguono marcatori da impostazioni dell’utente e provano che padre e istanze sorelle restano invariati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test nomi esatti, GIT_EDITOR e SBX_NONET_ACTIVE`; `npm run build`.

Commit: `feat(c11.1): Applicare la rimozione esatta dei marcatori ereditati`. Registrare stato e prove sotto `progress.json` → `steps` → `C11` → `units` → `C11.1`.

### C11.2 · Risolvere collocazione e cartelle private

Piani program/home con percorsi temporanei e cache privati, ACL e combinazioni vietate.

Stima: 4–8 ore. File: `crates/adapter/src/launch/placement.rs`, `crates/state/src/store_roots.rs`.

Accettazione: own con system home è rifiutato; la creazione tocca soltanto cartelle di Shinatama e non apre credenziali. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test placement, permessi e percorsi box`; `npm run build`.

Commit: `feat(c11.2): Risolvere collocazione e cartelle private`. Registrare stato e prove sotto `progress.json` → `steps` → `C11` → `units` → `C11.2`.

### C11.3 · Costruire argomenti e identità dell’avvio

Piano immutabile con argv, cartella, override del figlio, identificativi e token salvato come hash.

Stima: 5–10 ore. File: `crates/adapter/src/launch/plan.rs`, `crates/engine/src/launch.rs`.

Accettazione: Percorsi con spazi e simboli non passano da una shell; token e credenziali non compaiono nei manifesti o nei log. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test argv, identità e oscuramento token`; `npm run build`.

Commit: `feat(c11.3): Costruire argomenti e identità dell’avvio`. Registrare stato e prove sotto `progress.json` → `steps` → `C11` → `units` → `C11.3`.

### C11.4 · Collegare il punto di avvio unico e il controllo preliminare

UI e CLI inoltrano la richiesta allo stesso servizio con stati disponibili, rifiutati e da completare.

Stima: 4–8 ore. File: `crates/engine/src/launch.rs`, `apps/desktop/src/launch-review.ts`.

Accettazione: Nessun chiamante salta collocazione, capacita o sidecar richiesti; l assenza di Daodan non blocca il supporto dichiarato. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test convergenza UI/CLI e rifiuto prerequisiti`; `npm run build`.

Commit: `feat(c11.4): Collegare il punto di avvio unico e il controllo preliminare`. Registrare stato e prove sotto `progress.json` → `steps` → `C11` → `units` → `C11.4`.

<a id="c12"></a>
## C12 · Integrare il sorgente del sidecar e il suo ambiente privato

Installazione del sidecar dall'unico sorgente con provenienza registrata, cartelle private, CA e ambiente del figlio, più configurazione di base effettiva.

**Ambito:** v1. **Dipendenze:** C11. **Stima:** 20–40 ore.

**File/componenti:** `crates/sidecar/src/source.rs`; `crates/sidecar/src/config.rs`; `crates/sidecar/src/launch.rs`.

**Accettazione osservabile:**

- Il codice sul percorso delle richieste non viene duplicato o modificato in Shinatama; le correzioni arrivano dall'unico sorgente.
- Nessuna modifica a certificati o configurazione di sistema; i manifesti non contengono credenziali; la fiducia nella CA è provata per ciascun runtime degli agenti.
- La configurazione di compressione è esplicita, effortRouting resta bloccato su off e la disattivazione della compressione funziona.
- Nessun avvio dichiarato supportato salta le verifiche della copertura del traffico del provider.
- Ereditare dal singolo sorgente sidecar la correzione storage del 2026-11-02: schema v9 con sei indici e sync CCR transazionale per variazioni nette; identificare l artefatto con revisione/fingerprint e non con il solo numero versione.

**Verifiche del passo:** `SC1-SC4/SC7-SC11 isolation corpus`; `synthetic HTTP/TLS fixtures`; `effective-config/seed-bypass fixtures`.

**Gate collegati, da registrare separatamente:** SC1-SC4, SC7-SC13, CX12, HM12, DS12, T5, T6, T7.

**Requisiti coperti dal piano:** FRD-0003/R17, FRD-0009/R01, FRD-0009/R02, FRD-0009/R03, FRD-0009/R04, FRD-0009/R05, FRD-0009/R06, FRD-0009/R07, FRD-0009/R08, FRD-0009/R09, FRD-0009/R10, FRD-0009/R11, FRD-0009/R12, FRD-0009/R13, FRD-0009/R14, FRD-0009/R15, FRD-0009/R16, FRD-0009/R23, FRD-0009/R27, FRD-0009/R28, FRD-0009/R30.

**Commit proposto:** `feat(c12): Integrare il sorgente del sidecar e il suo ambiente privato`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/sidecar/src/source.rs, crates/sidecar/src/config.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C12.1 · Fissare sorgente e provenienza del sidecar

Dipendenza dell’unico sorgente con revisione, hash e costruzione riproducibile; nessun fork locale.

Stima: 4–8 ore. File: `crates/sidecar/src/source.rs`, `docs/licenses/**`.

Accettazione: Una revisione errata o un componente mancante produce errore esplicito senza download o modifica impliciti. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: verifica provenienza e costruzione sidecar`; `npm run build`.

Commit: `feat(c12.1): Fissare sorgente e provenienza del sidecar`. Registrare stato e prove sotto `progress.json` → `steps` → `C12` → `units` → `C12.1`.

### C12.2 · Creare root privata e manifesti senza credenziali

Directory e configurazione del sidecar sotto dati privati, con manifesti limitati ad argv e override.

Stima: 5–10 ore. File: `crates/sidecar/src/config.rs`, `crates/sidecar/src/manifest.rs`.

Accettazione: Le fixture provano che nessun file di configurazione globale o credenziale è letto, copiato o modificato. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test SC1, SC2 e SC4 su ambiente sintetico`; `npm run build`.

Commit: `feat(c12.2): Creare root privata e manifesti senza credenziali`. Registrare stato e prove sotto `progress.json` → `steps` → `C12` → `units` → `C12.2`.

### C12.3 · Applicare la fiducia TLS soltanto ai processi figli

Override CA specifici per runtime senza cambiare il trust di sistema.

Stima: 5–10 ore. File: `crates/sidecar/src/trust.rs`, `crates/adapter/src/launch/**`.

Accettazione: I test TLS locali provano trust nel figlio e rifiuto nel processo non configurato; i runtime non provati restano scoperti. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture CA e trust per runtime`; `npm run build`.

Commit: `feat(c12.3): Applicare la fiducia TLS soltanto ai processi figli`. Registrare stato e prove sotto `progress.json` → `steps` → `C12` → `units` → `C12.3`.

### C12.4 · Collegare traffico e configurazione effettiva

Piano di lancio attraverso il sidecar, baseline compressione e bypass seed ereditato dal sorgente.

Stima: 6–12 ore. File: `crates/sidecar/src/launch.rs`, `crates/sidecar/src/config.rs`.

Accettazione: Il traffico sintetico passa nel proxy; configurazione effettiva letta e effortRouting off; nessuna dichiarazione di copertura senza prova. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test traffico, effective-config e seed bypass`; `npm run build`.

Commit: `feat(c12.4): Collegare traffico e configurazione effettiva`. Registrare stato e prove sotto `progress.json` → `steps` → `C12` → `units` → `C12.4`.

<a id="c09r"></a>
## C09R · Proteggere l host con budget e recupero adattivi

Controllore locale misurato, budget per albero e ammissione aggregata, limiti preventivi e recupero attribuibile, con priorita a host e sessioni sane.

**Ambito:** v1. **Dipendenze:** C03, FS03, C11, C03M. **Stima:** 24–48 ore.

**File/componenti:** `crates/engine/src/resources/**`; `verify/RES/**`; `docs/implementation/2026-11-02-host-resilience.md`.

**Accettazione osservabile:**

- R59-R63 seguono la specifica di resilienza aggiornata: nessun crash o blocco indefinito dell host nel perimetro qualificato.
- Budget e soglie sono misurati e congelati prima del test; niente limiti restrittivi condivisi con host e renderer.
- Solo emergenza attribuibile autorizza arresto automatico del responsabile; nessun replay di lavoro agente.

**Verifiche del passo:** `pianificato: RES1-RES4 sintetici prima delle integrazioni dipendenti`; `pianificato: prove di carico e fault isolate, con dati sintetici`.

**Gate collegati, da registrare separatamente:** RES1, RES2, RES3, RES4, TL5, P18.

**Requisiti coperti dal piano:** FRD-0001/R59, FRD-0001/R60, FRD-0001/R61, FRD-0001/R62, FRD-0001/R63.

**Commit proposto:** `feat(c09r): proteggere host e sessioni sane con budget adattivi`.

**Prima azione alla ripresa:** Leggere la specifica di resilienza e preparare fixture sintetiche con limiti; iniziare dal campionatore locale e non da soglie arbitrarie.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C09R.1 · Misurare risorse e pressione senza bloccare l host

Campionatore locale limitato per host/renderer, alberi e sistema, con PID+start-time; distinguere commit/residente e gestire campioni mancanti.

Stima: 6–12 ore. File: `crates/engine/src/resources/**`, `verify/RES/**`, `docs/verification/`.

Accettazione: Fixture di processo terminato, accesso negato, crescita e contatori senza doppio conteggio. La build e l avvio delle funzionalita esistenti restano utilizzabili; nessun gate e promosso senza la relativa prova.

Verifiche pianificate: `pianificato: fixture sintetiche RES1-RES4 su Windows con risorse limitate`; `pianificato: misura locale di memoria, CPU, code e latenza UI`.

Commit: `feat(c09r.1): Misurare risorse e pressione senza bloccare l host`. Registrare stato e prove sotto `progress.json` → `steps` → `C09R` → `units` → `C09R.1`.

### C09R.2 · Ammettere avvii e applicare limiti nativi per albero

Prenotazioni aggregate tra finestre e limiti OS prima del carico, separati da host/renderer; fail-closed per nuovi avvii se manca il budget.

Stima: 6–12 ore. File: `crates/engine/src/resources/**`, `verify/RES/**`, `docs/verification/`.

Accettazione: Due finestre concorrenti non superano il budget; scadenza/riconciliazione dopo crash senza affidarsi al solo PID. La build e l avvio delle funzionalita esistenti restano utilizzabili; nessun gate e promosso senza la relativa prova. Burst di allocazioni e creazione figli piu rapidi del campionamento incontrano i limiti nativi; fallimenti di allocazione/creazione restano locali. Il guasto del monitor non rimuove i limiti esistenti.

Verifiche pianificate: `pianificato: fixture sintetiche RES1-RES4 su Windows con risorse limitate`; `pianificato: misura locale di memoria, CPU, code e latenza UI`.

Commit: `feat(c09r.2): Ammettere avvii e applicare limiti nativi per albero`. Registrare stato e prove sotto `progress.json` → `steps` → `C09R` → `units` → `C09R.2`.

### C09R.3 · Calibrare budget e intervento di emergenza attribuibile

Baseline sane con tetti fissi e isteresi; ridurre carico opzionale e rifiutare nuovi avvii prima di arrestare solo il responsabile.

Stima: 6–12 ore. File: `crates/engine/src/resources/**`, `verify/RES/**`, `docs/verification/`.

Accettazione: Nessuna vittima arbitraria per pressione esterna; una perdita continua non alza il tetto; il terminale sano continua a rispondere. La build e l avvio delle funzionalita esistenti restano utilizzabili; nessun gate e promosso senza la relativa prova. Rientro sotto una soglia inferiore con intervallo stabile; policy e limite visibili nella UI, overhead misurato, nessuna sospensione cieca di processi con lock.

Verifiche pianificate: `pianificato: fixture sintetiche RES1-RES4 su Windows con risorse limitate`; `pianificato: misura locale di memoria, CPU, code e latenza UI`.

Commit: `feat(c09r.3): Calibrare budget e intervento di emergenza attribuibile`. Registrare stato e prove sotto `progress.json` → `steps` → `C09R` → `units` → `C09R.3`.

### C09R.4 · Qualificare guasti e recupero con soglie congelate

Policy osservabile e fixture isolate per fault del monitor, OOM limitato, process storm e output eccessivo; nessun replay dei comandi agente.

Stima: 6–12 ore. File: `crates/engine/src/resources/**`, `verify/RES/**`, `docs/verification/`.

Accettazione: Ripetere fault in ambiente con limiti/VM, mai esaurire la macchina; salvati e fratelli sani preservati, recupero con retry finiti. La build e l avvio delle funzionalita esistenti restano utilizzabili; nessun gate e promosso senza la relativa prova.

Verifiche pianificate: `pianificato: fixture sintetiche RES1-RES4 su Windows con risorse limitate`; `pianificato: misura locale di memoria, CPU, code e latenza UI`.

Commit: `feat(c09r.4): Qualificare guasti e recupero con soglie congelate`. Registrare stato e prove sotto `progress.json` → `steps` → `C09R` → `units` → `C09R.4`.

<a id="c13"></a>
## C13 · Supervisionare i sidecar distinguendo occupazione e recupero

Un solo supervisore per finestra, topologia scelta sulla base delle misure, recupero sullo stesso indirizzo e diagnostica separata con stato visibile.

**Ambito:** v1. **Dipendenze:** C12, C09R. **Stima:** 20–40 ore.

**File/componenti:** `crates/sidecar/src/supervisor.rs`; `apps/desktop/src/sidecar-status.ts`.

**Accettazione osservabile:**

- Processo vivo e porta occupata significano busy, non morto; i test con ciclo eventi bloccato non avviano sostituti duplicati.
- Il recupero crea esattamente un sostituto; attese e ultimo scaricamento dell'output sopravvivono all'uscita della sessione.
- Nessun byte di launcher o proxy raggiunge il PTY dell'agente durante i guasti.
- La UI mostra lo stato effettivo CCR, input e output, distinguendo chiaramente traffico non coperto, busy e recovering.
- SC16 eredita la correzione storage del 2026-11-02: sei indici schema v9 e CCR incrementale transazionale; fixture locale CONNECT/health sotto 750 ms, integrità e prova di carico senza connessioni rifiutate, senza bypass o aumento dei timeout.
- FRD-0009/R24 aggiornato 2026-11-02: prova controllata di persistenza e integrita nel build preparato; ciascun CONNECT+echo e health locale sotto 750 ms, senza aumentare timeout o bypassare il proxy. SC16 globale resta aperto fino al carico di un ora sulle piattaforme richieste.
- Prima di avviare il sidecar si valuta process-wrap per i processi senza PTY (job con avvio sospeso, licenza, manutenzione, costo di integrazione): l'esito è una decisione registrata in `[workspace.dependencies]` con motivo e licenza, oppure il rifiuto motivato. watchexec-supervisor ed expectrl non vengono usati.
- Ogni sidecar usa ammissione e budget C09R; recupero con attesa crescente, numero massimo di tentativi e blocco dopo guasti ripetuti. Allocazioni rifiutate, pipe rotte e monitor guasto non bloccano l host ne creano duplicati; log e code restano limitati.

**Verifiche del passo:** `held-event-loop/port-bind/dead-owner/stale-proxy fault suite`; `PTY capture plus diagnostic-log comparison`; `SC16 inherited storage-latency suite, v8 migration integrity, local CONNECT/health fixture and one-hour load per supported OS`; `planned: RES1-RES4 sidecar con crash ripetuti, memoria al limite, monitor guasto e verifica limiti di retry/log/code`.

**Gate collegati, da registrare separatamente:** SC5, SC6, SC12, SC14, SC15, SC16, T9, RES1, RES2, RES3, RES4 (integrazione).

**Requisiti coperti dal piano:** FRD-0009/R17, FRD-0009/R18, FRD-0009/R19, FRD-0009/R20, FRD-0009/R21, FRD-0009/R22, FRD-0009/R23, FRD-0009/R24, FRD-0009/R25, FRD-0009/R26, FRD-0009/R29, FRD-0001/R59, FRD-0001/R60, FRD-0001/R61, FRD-0001/R62, FRD-0001/R63.

**Commit proposto:** `feat(c13): Supervisionare i sidecar distinguendo occupazione e recupero`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/sidecar/src/supervisor.rs, apps/desktop/src/sidecar-status.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C13.1 · Modellare proprietà, stato e topologia del proxy

Stati tipizzati e unico proprietario del proxy con scelta di topologia registrata dalle misure.

Stima: 4–8 ore. File: `crates/sidecar/src/supervisor.rs`.

Accettazione: Due richieste concorrenti non creano due supervisori o due processi per lo stesso indirizzo. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili. La scelta su process-wrap è registrata con versione, licenza e motivo prima del primo avvio del sidecar. Prima del primo spawn applicare ammissione e budget C09R; valutare process-wrap anche per composizione con i limiti nativi richiesti, senza assumerne il supporto.

Verifiche pianificate: `planned: test concorrenza e proprietà del proxy`; `npm run build`; `planned: verifica su fonti ufficiali di manutenzione e licenza di process-wrap; avvio sospeso nel job su Windows`.

Commit: `feat(c13.1): Modellare proprietà, stato e topologia del proxy`. Registrare stato e prove sotto `progress.json` → `steps` → `C13` → `units` → `C13.1`.

### C13.2 · Distinguere proxy occupato da proxy terminato

Controllo separato di processo e porta, intervallo crescente e indicazione busy.

Stima: 4–8 ore. File: `crates/sidecar/src/liveness.rs`, `apps/desktop/src/sidecar-status.ts`.

Accettazione: Un event loop fermo con processo vivo e porta occupata non genera sostituzioni o sonde eccessive. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fault test event loop bloccato e porta occupata`; `npm run build`.

Commit: `feat(c13.2): Distinguere proxy occupato da proxy terminato`. Registrare stato e prove sotto `progress.json` → `steps` → `C13` → `units` → `C13.2`.

### C13.3 · Recuperare una sola volta sullo stesso indirizzo

Recupero serializzato alla morte del processo o liberazione della porta, con drenaggio dell’uscita.

Stima: 4–8 ore. File: `crates/sidecar/src/recovery.rs`.

Accettazione: Un guasto crea un solo sostituto; le sessioni attendono lo stesso endpoint e l’ultimo output non viene perso. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili. Attesa crescente e tetto ai tentativi interrompono i cicli di crash; nessun riavvio dell agente o replay delle sue richieste. La violazione di risorse resta confinata al sidecar responsabile.

Verifiche pianificate: `planned: fault test kill, porta libera e uscita concorrente`; `npm run build`; `planned: crash loop e allocazione rifiutata, recupero limitato senza duplicati o replay`.

Commit: `feat(c13.3): Recuperare una sola volta sullo stesso indirizzo`. Registrare stato e prove sotto `progress.json` → `steps` → `C13` → `units` → `C13.3`.

### C13.4 · Separare diagnostica e terminali degli agenti

Cattura completa della diagnostica nei log privati e stato effettivo nella finestra.

Stima: 4–8 ore. File: `crates/sidecar/src/diagnostics.rs`, `apps/desktop/src/sidecar-status.ts`.

Accettazione: Ogni guasto compare nel log e nessuna sua riga compare nel PTY dell’agente; CCR/input/output sono letti dalla configurazione effettiva. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili. La cattura dei log ha limiti di coda, dimensione e conservazione; saturazione ed errori di scrittura non bloccano host o sessioni.

Verifiche pianificate: `planned: confronto cattura PTY/log con guasti SC15`; `npm run build`.

Commit: `feat(c13.4): Separare diagnostica e terminali degli agenti`. Registrare stato e prove sotto `progress.json` → `steps` → `C13` → `units` → `C13.4`.

### C13.5 · Qualificare il fix storage e la latenza locale del sidecar

Correzione upstream schema v9/CCR ereditata nel build preparato, con migrazione e regressione di persistenza/trasporto nel percorso Shinatama; nessuna patch locale duplicata.

Stima: 4–8 ore. File: `crates/sidecar/**`, `verify/SC16/**`, `docs/notes/2026-11-02-sidecar-storage-latency.md`, `docs/verification/`.

Accettazione: Migrazione ripetibile e integrita: sei indici, nessun blob condiviso perso per CCR riordinati/rinominati/scambiati; scrittura rifiutata fa rollback. Con 200.000 messaggi e 600 sostituzioni CCR, ogni CONNECT+echo e health locale termina sotto 750 ms. Timeout invariati; l indicatore busy non sostituisce la prova.

Verifiche pianificate: `pianificato: suite storage upstream sul fingerprint preparato, fixture migrazione v8/v9 e refcount`; `pianificato: sonde locali durante persistenza nel vero percorso Shinatama; poi carico un ora con cinque istanze dopo integrazione adattatori, ripetuto sugli OS richiesti e upgrade`.

Commit: `test(c13.5): qualificare storage e latenza locale del sidecar`. Registrare stato e prove sotto `progress.json` → `steps` → `C13` → `units` → `C13.5`.

<a id="c14"></a>
## C14 · Preparare checkup, manifest e set generici di avvio

Controllo preliminare unico e selezione dei plugin/skill/MCP scelti per istanza, senza installare o richiedere Daodan.

**Ambito:** v1. **Dipendenze:** C11, C13. **Stima:** 6–12 ore.

**File/componenti:** `crates/engine/src/checkup.rs`; `crates/adapter/src/sets/**`; `apps/desktop/src/launch-review.ts`.

**Accettazione osservabile:**

- Il checkup verifica programma, home, sidecar e set selezionato; Daodan assente non causa Degraded o rifiuto dell agente.
- Le configurazioni system restano immutate e i set per istanza rispettano le limitazioni degli adattatori.
- Le fixture di set generici precedono le prove QSET per adattatore; nessuna installazione marketplace implicita.

**Verifiche del passo:** `fixture manifest e set generici senza Daodan`; `checkup sintetico e rifiuti dei soli prerequisiti attivi`.

**Gate collegati, da registrare separatamente:** D7: checkup generico, D19: set generici per host; pacchetti Daodan rinviati.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(c14): preparare checkup e set generici di avvio`.

**Prima azione alla ripresa:** Aprire il controllo preliminare unico; verificare un avvio con set vuoto e Daodan assente.

<a id="fs03"></a>
## FS03 · Separazione degli schemi autorevoli e operativi e record dei progetti

Record versionati di progetti, spazi di lavoro e radici degli archivi; separazione tra tabelle autorevoli e operative, senza riassegnare la cronologia tra revisioni delle radici.

**Ambito:** v1. **Dipendenze:** C03, C11. **Stima:** 4–10 ore.

**File/componenti:** `crates/state/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** La tabella derivata thread_task di FRD-0014/R13 si aggiungera con C34; gli schemi generici restano attivi.

- Ricostruire soltanto gli indici eliminabili conservando la cronologia autorevole.
- Rimuovere una radice deve marcarla come non disponibile; lo stesso ID nativo in due radici deve restare distinto.
- Il layout deve usare revisione e proprietario e non avviare processi durante il ripristino.
- Il legame del companion deve essere distinto dalla discendenza degli hop; ID e date devono rispettare il contratto canonico.

**Verifiche del passo:** `planned: Rebuild only disposable indexes while authoritative history survives.`; `planned: Root removal marks unavailable; same native ID in two roots remains distinct.`; `planned: Workspace layout uses revision/owner and restores no processes.`; `planned: Companion relation is separate from hop parent lineage; IDs/timestamps match canonical contract.`.

**Gate collegati, da registrare separatamente:** TL3, SS5, S9.

**Requisiti coperti dal piano:** FRD-0014/R13, FRD-0014/R14, FRD-0014/R15, FRD-0014/R16, FRD-0014/R17, FRD-0014/R18.

**Commit proposto:** `feat(fs03): Separazione degli schemi autorevoli e operativi e record dei progetti`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c06"></a>
## C06 · Introdurre un modello validato ad albero per le suddivisioni

Albero di layout versionato in Rust o TypeScript, trasformazioni atomiche per dividere, riordinare, spostare e chiudere viste, con revisioni persistenti degli spazi di lavoro.

**Ambito:** v1. **Dipendenze:** B02, FS03, TL1-proof. **Stima:** 10–20 ore.

**File/componenti:** `crates/workspace/**`; `apps/desktop/src/layout.ts`; `crates/state/src/workspaces.rs`.

**Accettazione osservabile:**

- Cicli, duplicati, schede attive non valide, pesi non finiti e limiti di profondità vengono rifiutati senza modifiche parziali.
- Il ripristino ricrea soltanto descrittori di risorse: nessun PID, output, buffer modificato, credenziale o esecuzione.
- Una seconda finestra non può acquisire lo stesso spazio di lavoro; un layout corrotto recupera l'ultima istantanea valida.

**Verifiche del passo:** `layout property and revision-conflict tests`; `two-window ownership fixtures`.

**Gate collegati, da registrare separatamente:** TL3.

**Requisiti coperti dal piano:** FRD-0001/R23, FRD-0001/R24, FRD-0001/R28, FRD-0001/R30, FRD-0001/R31, FRD-0001/R32, FRD-0014/R18.

**Commit proposto:** `feat(c06): Introdurre un modello validato ad albero per le suddivisioni`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/workspace/**, apps/desktop/src/layout.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C06.1 · Definire e validare l’albero di layout

Modello versionato e trasformazioni pure con limiti di profondità, pannelli e pesi.

Stima: 4–8 ore. File: `crates/workspace/**`, `apps/desktop/src/layout.ts`.

Accettazione: Cicli, duplicati, schede attive invalide e pesi non finiti sono rifiutati senza mutazioni parziali. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test proprietà e invarianti dell’albero`; `npm run build`.

Commit: `feat(c06.1): Definire e validare l’albero di layout`. Registrare stato e prove sotto `progress.json` → `steps` → `C06` → `units` → `C06.1`.

### C06.2 · Persistire revisioni e proprietà dello spazio di lavoro

Record per progetto/workspace con aggiornamento ottimistico, proprietario e ultima copia valida.

Stima: 6–12 ore. File: `crates/state/src/workspaces.rs`, `crates/workspace/src/persistence.rs`.

Accettazione: Due finestre non acquisiscono lo stesso workspace; corruzione o versione sconosciuta recuperano senza esecuzioni. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test conflitto revisioni, proprietario e recupero`; `npm run build`.

Commit: `feat(c06.2): Persistire revisioni e proprietà dello spazio di lavoro`. Registrare stato e prove sotto `progress.json` → `steps` → `C06` → `units` → `C06.2`.

<a id="c07"></a>
## C07 · Disegnare l'albero di pannelli e completare i comandi da tastiera

Pannelli annidati e ridimensionabili, riordino e divisione dai bordi delle schede, anteprima dei rilasci e operazioni equivalenti da tastiera. Cornice con barra superiore, barra dei tasti e overlay di ricerca.

**Ambito:** v1. **Dipendenze:** C06. **Stima:** 15–30 ore.

**File/componenti:** `apps/desktop/src/workspace/**`; `apps/desktop/src/main.ts`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** La palette Daodan di FRD-0001/R12 e rinviata; le altre superfici restano attive.

- Un trascinamento annullato o non valido non cambia nulla; gli spostamenti preservano identità e output del processo attivo.
- Solo il terminale con focus riceve input; una finestra modale impedisce l'incolla nei terminali sullo sfondo.
- Le finestre strette mantengono raggiungibile ogni pannello; una vista nascosta conserva l'ultima dimensione utilizzabile.
- Cornice con bordo e titolo, barra superiore con thread, cartella, ramo, costo e icona dell'agente, barra inferiore dei tasti e overlay di ricerca sono colorati dalla skin; i dati non ancora disponibili appaiono come tali (FRD-0001/R12).

**Verifiche del passo:** `desktop nested-split/move/cancel/keyboard smoke`; `four noisy panes plus resize instrumentation`.

**Gate collegati, da registrare separatamente:** TL2, TL5, U3, U4, U5, P11, P12.

**Requisiti coperti dal piano:** FRD-0001/R14, FRD-0001/R23, FRD-0001/R24, FRD-0001/R25, FRD-0001/R26, FRD-0001/R27, FRD-0001/R29, FRD-0001/R32, FRD-0001/R33, FRD-0001/R34, FRD-0001/R12.

**Commit proposto:** `feat(c07): Disegnare l'albero di pannelli e completare i comandi da tastiera`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/src/workspace/**, apps/desktop/src/main.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C07.1 · Disegnare pannelli annidati e ridimensionamento

Renderer dell’albero con gruppi di schede, dimensioni minime e massimizzazione reversibile.

Stima: 4–8 ore. File: `apps/desktop/src/workspace/render.ts`.

Accettazione: Quattro pannelli annidati restano raggiungibili a finestra stretta; i terminali nascosti mantengono misure utilizzabili. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: prova desktop pannelli annidati e finestra stretta`; `npm run build`.

Commit: `feat(c07.1): Disegnare pannelli annidati e ridimensionamento`. Registrare stato e prove sotto `progress.json` → `steps` → `C07` → `units` → `C07.1`.

### C07.2 · Spostare e dividere schede con trascinamento atomico

Anteprima dei bersagli, riordino e rilascio sul bordo senza ricreare processi.

Stima: 4–8 ore. File: `apps/desktop/src/workspace/drag.ts`.

Accettazione: Annullamento e bersagli invalidi non cambiano il modello; spostamento e divisione conservano identità e output. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: prova desktop drag/cancel/move/edge-split`; `npm run build`.

Commit: `feat(c07.2): Spostare e dividere schede con trascinamento atomico`. Registrare stato e prove sotto `progress.json` → `steps` → `C07` → `units` → `C07.2`.

### C07.3 · Completare focus e operazioni da tastiera

Comandi documentati per focus, spostamento, divisione, ridimensionamento e ripristino.

Stima: 4–8 ore. File: `apps/desktop/src/workspace/keys.ts`, `apps/desktop/src/main.ts`.

Accettazione: La modalità solo tastiera copre tutte le azioni; input e incolla non raggiungono terminali sullo sfondo durante un dialogo. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: matrice focus, shortcut e modal-paste`; `npm run build`.

Commit: `feat(c07.3): Completare focus e operazioni da tastiera`. Registrare stato e prove sotto `progress.json` → `steps` → `C07` → `units` → `C07.3`.

### C07.4 · Disegnare cornice, barre e overlay di ricerca

Cornice, barra superiore, barra dei tasti e overlay di ricerca che lascia intatto lo schermo dell'emulatore.

Stima: 3–6 ore. File: `apps/desktop/src/workspace/chrome.ts`.

Accettazione: Le superfici seguono la skin e chiudendo l'overlay il focus torna al pannello di origine; la compilazione e l'avvio dell'app esistente restano funzionanti.

Verifiche pianificate: `planned: prova desktop di barre, overlay e ritorno del focus`; `npm run build`.

Commit: `feat(c07.4): Disegnare cornice, barre e overlay di ricerca`. Registrare stato e prove sotto `progress.json` → `steps` → `C07` → `units` → `C07.4`.

<a id="c08"></a>
## C08 · Rendere configurabili i temi e i profili versionati dei terminali

Temi Shinatama, Section9, TCTF e Sumi per la cornice, profili per tutti gli agenti, ripiego dei font e opzione esplicita per seguire il tema.

**Ambito:** v1. **Dipendenze:** C07. **Stima:** 12–20 ore.

**File/componenti:** `apps/desktop/src/theme/**`; `apps/desktop/src/profiles/**`; `docs/licenses/**`.

**Accettazione osservabile:**

- L'output truecolour resta invariato; l'opzione di seguire il tema è disattivata inizialmente e modifica soltanto colori predefiniti e sedici colori ANSI.
- Ogni ruolo di testo secondario rispetta il contrasto richiesto o ha un'eccezione esplicita; nessun effetto luminoso.
- Una modifica di font o metriche invia un solo ridimensionamento finale; profili e font sconosciuti usano il ripiego senza riavvio; le licenze delle risorse sono documentate.

**Verifiche del passo:** `contrast script over final tokens`; `ANSI/truecolour/profile/resize fixtures`; `screen-reader and no-Nerd-font checks`.

**Gate collegati, da registrare separatamente:** U1, U2, U8, P10, TL2, X3.

**Requisiti coperti dal piano:** FRD-0001/R42, FRD-0001/R43, FRD-0001/R44, FRD-0001/R45, FRD-0001/R46, FRD-0001/R47, FRD-0001/R48, FRD-0001/R49.

**Commit proposto:** `feat(c08): Rendere configurabili i temi e i profili versionati dei terminali`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/src/theme/**, apps/desktop/src/profiles/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C08.1 · Introdurre temi della cornice selezionabili

Token dei quattro temi e selezione persistente con applicazione immediata.

Stima: 4–7 ore. File: `apps/desktop/src/theme/**`.

Accettazione: Il cambio tema non riavvia terminali e il contrasto di ogni ruolo è misurato senza glow. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: script contrasto e prova cambio tema`; `npm run build`.

Commit: `feat(c08.1): Introdurre temi della cornice selezionabili`. Registrare stato e prove sotto `progress.json` → `steps` → `C08` → `units` → `C08.1`.

### C08.2 · Versionare profili e ripieghi dei terminali

Profili per tutti gli harness con font, metriche, cursore e palette dichiarati.

Stima: 4–7 ore. File: `apps/desktop/src/profiles/**`.

Accettazione: Profili o font mancanti ricadono sul predefinito senza riavvio e una modifica delle metriche consegna il resize finale. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture profili/font e resize`; `npm run build`.

Commit: `feat(c08.2): Versionare profili e ripieghi dei terminali`. Registrare stato e prove sotto `progress.json` → `steps` → `C08` → `units` → `C08.2`.

### C08.3 · Aggiungere follow-skin e avvisi sulle risorse

Opzione off iniziale che modifica solo colori predefiniti e sedici colori ANSI, più licenze degli asset.

Stima: 4–6 ore. File: `apps/desktop/src/profiles/follow-skin.ts`, `docs/licenses/**`.

Accettazione: Le sequenze truecolour conservano i byte; fallback accessibili e licenze sono documentati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: corpus ANSI/truecolour e licenze`; `npm run build`.

Commit: `feat(c08.3): Aggiungere follow-skin e avvisi sulle risorse`. Registrare stato e prove sotto `progress.json` → `steps` → `C08` → `units` → `C08.3`.

<a id="c09"></a>
## C09 · Rendere robusti terminale e contenimento con fixture sintetiche

Meccanismi terminali riusabili e fixture sintetiche per I/O, Unicode, input/focus, resize, code limitate e arresto del processo. Le certificazioni dei veri agenti seguono separatamente. Contenimento con garanzie distinte per crash dell'app e stop del singolo terminale.

**Ambito:** v1. **Dipendenze:** C07, C08, C09R. **Stima:** 8–16 ore.

**File/componenti:** `crates/terminal/**`; `verify/P*/**`; `docs/verification/**`; `crates/terminal/src/process_tree.rs`; `crates/terminal/src/lib.rs`; `apps/desktop/src-tauri/src/main.rs`; `crates/terminal/tests/`.

**Accettazione osservabile:**

- Corpus sintetico riproducibile per schermo alternativo, truecolor, Unicode e sequenze ostili senza account o chiamate modello.
- I/O, modal focus, resize finale, code piene e arresto albero testati sulla piattaforma del commit; limiti per le altre piattaforme espliciti.
- Ogni combinazione agente/piattaforma resta non certificata fino alla relativa scheda QA, senza bloccare il primo adattatore sulle altre combinazioni.
- Il job globale copre l'uscita e il crash dell'app: Shinatama vi entra prima di creare qualsiasi processo da contenere, l'handle resta solo nell'app e non è ereditabile, e dopo il kill dell'app non sopravvive nessun processo entro 10 s.
- Lo stop di un terminale ha una garanzia distinta: con due terminali aperti e un nipote fatto nascere prima dell'assegnazione al job di sessione, arrestare A elimina tutto l'albero di A e lascia B funzionante. La prova forza quest'ordine con un punto di attesa solo di test, senza affidarsi alle ripetizioni.
- Prova di sonda in verify/P14 (docs/verification/2026-11-02-P14-containment-probe.md): con l'ordine attuale nessun codice del figlio gira prima dell'assegnazione al job di sessione perché ConPTY trattiene il figlio finché l'host non risponde alla richiesta di posizione del cursore. Il test permanente fallisce se un figlio gira prima dell'assegnazione; in tal caso, o se una versione di Windows cambia il comportamento, il contenimento passa a PROC_THREAD_ATTRIBUTE_JOB_LIST con la pseudoconsole (verificato nella sonda, preferito a CREATE_SUSPENDED) e la prova viene ripetuta.
- Lo stop di un terminale chiude sia il job sia la pseudoconsole; un discendente staccato dalla console dopo lo stop non sopravvive.
- Integrare C09R nel percorso PTY/IPC/renderer: crash, blocco, allocazione rifiutata, output ostile e code piene restano locali; host e terminale sano rispondono entro soglie dichiarate. La policy di emergenza arresta solo il responsabile, preserva lo stato salvato e non ripete comandi.

**Verifiche del passo:** `fixture sintetiche terminal/emulator e process lifecycle`; `corpus ANSI negativo`; `npm run test:desktop sulla piattaforma disponibile`; `planned: kill dell'app con figlio e nipote su Windows; nessun superstite entro 10 s`; `planned: handle del job globale non ereditabile e assente nei figli`; `planned: guardia permanente: nessun codice del figlio gira prima dell'assegnazione al job di sessione (ordine di produzione)`; `planned: nipote forzato prima dell'assegnazione, stop di A con B vivo, nipote staccato dalla console, su Windows`; `planned: RES1-RES3 nel percorso terminale reale con sibling sano, limiti di memoria/CPU/processi e monitor guasto`.

**Gate collegati, da registrare separatamente:** P1–P20 parziali: soli meccanismi sintetici, TL5 parziale, U6, P14, RES1, RES2, RES3, RES4 (integrazione).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R60, FRD-0001/R61, FRD-0001/R62, FRD-0001/R63.

**Commit proposto:** `feat(c09): Rendere robusti terminale e contenimento con fixture sintetiche`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/terminal/**, verify/P*/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c15"></a>
## C15 · Avviare il primo adattatore Claude attraverso il checkup

Avvio Claude, stato di accesso e login tramite l'unico percorso verificato, con home privata predefinita e capacità dichiarate correttamente.

**Ambito:** v1. **Dipendenze:** C09, C10, C11, C13, C14. **Stima:** 12–24 ore.

**File/componenti:** `crates/adapter/src/claude/**`; `verify/CL/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- La TUI interattiva parte nel progetto scelto con sidecar e set generico selezionato, anche senza Daodan.
- SignedOut, Unknown, Degraded e le domande iniziali di fiducia sono visibili; le credenziali restano nel flusso dell'agente.
- Versione delle fixture e versione installata coincidono prima di dichiarare supporto; i test reali usano un account sintetico autorizzato separatamente.
- Supporto dichiarato solo per versione/capacità/piattaforma effettivamente provate; il resto resta indisponibile fino ai passi QA/QSET.

**Verifiche del passo:** `Claude contract fixtures`; `synthetic signed-out smoke`; `authorized synthetic live TUI canary`.

**Gate collegati, da registrare separatamente:** CL1/CL4/CL8: sottoinsieme avvio/stato, D5, P20 parziale Claude, QA-claude-* per certificazione completa, CL13.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R08, FRD-0001/R09, FRD-0001/R10, FRD-0001/R18, FRD-0001/R19, FRD-0003/R01, FRD-0003/R02, FRD-0003/R03, FRD-0003/R04, FRD-0003/R07, FRD-0003/R10, FRD-0003/R11, FRD-0003/R12, FRD-0003/R14, FRD-0003/R15, FRD-0004/R10, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R45, FRD-0006/R48.

**Commit proposto:** `feat(c15): Avviare il primo adattatore Claude attraverso il checkup`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/claude/**, verify/CL/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C15.1 · Riconoscere Claude e leggere lo stato di autenticazione

Adattatore Claude con eseguibile/versione, capacità e stato SignedIn/SignedOut/Unknown.

Stima: 4–8 ore. File: `crates/adapter/src/claude/discovery.rs`, `crates/adapter/src/claude/auth.rs`.

Accettazione: Comandi status non stampano segreti e una versione fuori fixture è Degraded. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture Claude version/auth/refusal`; `npm run build`.

Commit: `feat(c15.1): Riconoscere Claude e leggere lo stato di autenticazione`. Registrare stato e prove sotto `progress.json` → `steps` → `C15` → `units` → `C15.1`.

### C15.2 · Avviare la TUI Claude dal piano verificato

Avvio interattivo tramite checkup, box e sidecar con arresto legato alla finestra; nessun requisito Daodan.

Stima: 4–8 ore. File: `crates/adapter/src/claude/run.rs`, `crates/engine/src/launch.rs`.

Accettazione: Il terminale mostra la TUI reale e i primi prompt senza pre-risposte; nessun percorso salta il checkup. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: smoke Claude firmato-out e avvio sicuro`; `npm run build`.

Commit: `feat(c15.2): Avviare la TUI Claude dal piano verificato`. Registrare stato e prove sotto `progress.json` → `steps` → `C15` → `units` → `C15.2`.

### C15.3 · Collegare login e capacità Claude provate

Flusso di login dell’agente, segnali iniziali e dichiarazioni di supporto limitate alle fixture passate.

Stima: 4–8 ore. File: `crates/adapter/src/claude/**`, `verify/CL/**`.

Accettazione: Login usa soltanto il flusso dell’agente; canary reali solo con account e budget sintetici autorizzati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: contratto Claude e login in home di test`; `npm run build`.

Commit: `feat(c15.3): Collegare login e capacità Claude provate`. Registrare stato e prove sotto `progress.json` → `steps` → `C15` → `units` → `C15.3`.

<a id="qa-claude-windows"></a>
## QA-claude-windows · Certificare terminale claude su windows

Evidenza riproducibile per un vero claude su windows, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** v1. **Dipendenze:** C09, C15. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/windows/claude/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di claude registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Completare SC16 con cinque istanze e storie lunghe per un ora sulla piattaforma: nessuna connessione rifiutata, profilare i ritardi; usare solo fixture/account e budget autorizzati.
- Ripetere i fault RES applicabili all agente Windows; nessun passaggio di test estranei qualifica la resilienza.

**Verifiche del passo:** `matrice P1–P20 claude/windows applicabile`; `fixture TUI reale claude con account/test sintetici autorizzati`; `planned: RES1-RES4 applicabili per agente/versione Windows con sessioni sintetiche, sibling sano, budget fissati e misure memoria/CPU/latenza`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7, RES1-RES4 (agente reale).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R63.

**Commit proposto:** `feat(qa-claude-windows): Certificare terminale claude su windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/platform/windows/claude/**, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c22"></a>
## C22 · Risolvere le radici persistenti degli archivi e collegare una revisione verificata di Mukade

Catalogo esplicito di cartelle private, di sistema e in sola lettura, con elenchi limitati e in sola lettura ottenuti dall'adattatore tramite Mukade fissato.

**Ambito:** v1. **Dipendenze:** C03, C11, C15. **Stima:** 14–28 ore.

**File/componenti:** `crates/sessions/src/roots.rs`; `crates/sessions/src/readers.rs`; `Cargo.toml`; `Cargo.lock`.

**Accettazione osservabile:**

- Ogni lettura identifica store_root_id; radici mancanti o rifiutate non usano mai valori impliciti dell'ambiente.
- I metadati sconosciuti restano sconosciuti; l'elenco non carica un'intera trascrizione per riga.
- I sottoprocessi di esportazione usano contesto isolato, timeout e pulizia; archivio nativo e file accessori restano invariati.

**Verifiche del passo:** `root-scoping/side-effect fingerprint fixtures`; `Mukade pin conformance corpus`.

**Gate collegati, da registrare separatamente:** SS1, SS5, H27, HM3, DS3, S9.

**Requisiti coperti dal piano:** FRD-0002/R02, FRD-0002/R03, FRD-0002/R20, FRD-0006/R01, FRD-0006/R02, FRD-0006/R03, FRD-0006/R05, FRD-0006/R06, FRD-0006/R07, FRD-0006/R14, FRD-0006/R15, FRD-0006/R17, FRD-0006/R18, FRD-0006/R24, FRD-0006/R44, FRD-0006/R47, FRD-0006/R48, FRD-0014/R16.

**Commit proposto:** `feat(c22): Risolvere le radici persistenti degli archivi e collegare una revisione verificata di Mukade`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/sessions/src/roots.rs, crates/sessions/src/readers.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C22.1 · Fissare e verificare la dipendenza Mukade

Revisione Git, lockfile e adattatore di lettura tramite API effettivamente presenti.

Stima: 4–8 ore. File: `Cargo.toml`, `Cargo.lock`, `crates/sessions/src/readers.rs`.

Accettazione: Una capacità non presente nel pin è rifiutata; nessun convertitore terzo o checkout ambientale. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: conformance API sul pin Mukade`; `npm run build`.

Commit: `feat(c22.1): Fissare e verificare la dipendenza Mukade`. Registrare stato e prove sotto `progress.json` → `steps` → `C22` → `units` → `C22.1`.

### C22.2 · Persistire e risolvere radici esplicite degli archivi

Catalogo box/system/read_only con identità e revisione separate.

Stima: 5–10 ore. File: `crates/sessions/src/roots.rs`, `crates/state/src/store_roots.rs`.

Accettazione: Stesso ID nativo in radici diverse resta distinto; radici mancanti non usano fallback ambientali. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture catalogo e revisioni store_root`; `npm run build`.

Commit: `feat(c22.2): Persistire e risolvere radici esplicite degli archivi`. Registrare stato e prove sotto `progress.json` → `steps` → `C22` → `units` → `C22.2`.

### C22.3 · Elencare sessioni senza caricare le trascrizioni

Metadati limitati e sottoprocessi export isolati, disponibili solo per adattatori implementati.

Stima: 5–10 ore. File: `crates/sessions/src/list.rs`, `crates/adapter/src/**/sessions.rs`.

Accettazione: Campi ignoti restano ignoti e l’elenco non legge un intero transcript per riga; archivi nativi invariati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture listing e fingerprint delle origini`; `npm run build`.

Commit: `feat(c22.3): Elencare sessioni senza caricare le trascrizioni`. Registrare stato e prove sotto `progress.json` → `steps` → `C22` → `units` → `C22.3`.

<a id="c23"></a>
## C23 · Costruire il dettaglio delle sessioni e trascrizioni canoniche limitate

Navigatore Progetti/Sessioni e pannelli in sola lettura Panoramica, Trascrizione e Discendenza, con controlli di riservatezza sui dati grezzi.

**Ambito:** v1. **Dipendenze:** C22, C07. **Stima:** 18–36 ore.

**File/componenti:** `crates/sessions/src/transcript.rs`; `apps/desktop/src/sessions/**`.

**Accettazione osservabile:**

- Un limite precedente alla lettura rifiuta sessioni grandi finché non esiste un'API paginata con limiti sull'intera operazione.
- Risposte obsolete o annullate non aggiornano un altro pannello; il contenuto grezzo non viene registrato o salvato dalla UI.
- La discendenza segnala relazioni mancanti, esterne, ambigue o cicliche senza inventare collegamenti.
- Apertura, scansione, clic sui nodi e copia non riprendono sessioni e non scrivono negli archivi.

**Verifiche del passo:** `30MB/malformed/torn-record/late-response corpus`; `privacy and native-store fingerprint tests`.

**Gate collegati, da registrare separatamente:** SS1, SS2, SS3, SS6, CL6, CX10, OC3, PI3, H15.

**Requisiti coperti dal piano:** FRD-0001/R11, FRD-0006/R04, FRD-0006/R05, FRD-0006/R12, FRD-0006/R13, FRD-0006/R16, FRD-0006/R17, FRD-0006/R18, FRD-0006/R19, FRD-0006/R20, FRD-0006/R21, FRD-0006/R22, FRD-0006/R23.

**Commit proposto:** `feat(c23): Costruire il dettaglio delle sessioni e trascrizioni canoniche limitate`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/sessions/src/transcript.rs, apps/desktop/src/sessions/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C23.1 · Proiettare trascrizioni canoniche con provenienza

Tipi canonici, filtro record e limite prima della lettura.

Stima: 4–8 ore. File: `crates/sessions/src/transcript.rs`.

Accettazione: Input troppo grande viene rifiutato prima di allocarlo; record incompleti e sconosciuti seguono le regole documentate. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: corpus transcript/torn/unknown/size-cap`; `npm run build`.

Commit: `feat(c23.1): Proiettare trascrizioni canoniche con provenienza`. Registrare stato e prove sotto `progress.json` → `steps` → `C23` → `units` → `C23.1`.

### C23.2 · Aggiungere navigatore e panoramica delle sessioni

Albero Projects/Sessions, filtro metadati e Overview in sola lettura.

Stima: 4–8 ore. File: `apps/desktop/src/sessions/navigator.ts`, `apps/desktop/src/sessions/overview.ts`.

Accettazione: Apertura e nuova scansione non lanciano processi o riprendono sessioni; risultati obsoleti vengono scartati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: smoke navigator e request-revision`; `npm run build`.

Commit: `feat(c23.2): Aggiungere navigatore e panoramica delle sessioni`. Registrare stato e prove sotto `progress.json` → `steps` → `C23` → `units` → `C23.2`.

### C23.3 · Mostrare transcript e ricerca nella sessione

Rendering limitato con strumenti collassabili e paginazione solo quando realmente supportata.

Stima: 5–10 ore. File: `apps/desktop/src/sessions/transcript.ts`, `crates/sessions/src/paging.rs`.

Accettazione: Una risposta tardiva non aggiorna la scheda sbagliata; sessioni da 30 MB rispettano il budget registrato o sono rifiutate. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: corpus 30MB e late/cancel responses`; `npm run build`.

Commit: `feat(c23.3): Mostrare transcript e ricerca nella sessione`. Registrare stato e prove sotto `progress.json` → `steps` → `C23` → `units` → `C23.3`.

### C23.4 · Aggiungere discendenza e copia riservata dei dati grezzi

Lineage esplicita e toggle/copia raw con oscuramento predefinito e avviso.

Stima: 5–10 ore. File: `apps/desktop/src/sessions/lineage.ts`, `apps/desktop/src/sessions/raw.ts`.

Accettazione: Relazioni mancanti/cicliche restano segnate; raw e contenuto copiato non vengono loggati o persistiti dalla UI. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture lineage e privacy raw/copy`; `npm run build`.

Commit: `feat(c23.4): Aggiungere discendenza e copia riservata dei dati grezzi`. Registrare stato e prove sotto `progress.json` → `steps` → `C23` → `units` → `C23.4`.

<a id="c24"></a>
## C24 · Indicizzare e cercare le sessioni in modo incrementale

Ricerca FTS5 con filtri per agente, progetto e data, indicazione delle origini eliminate e controlli espliciti per indicizzazione e rimozione dei dati.

**Ambito:** v1. **Dipendenze:** C23. **Stima:** 12–24 ore.

**File/componenti:** `crates/sessions/src/index.rs`; `apps/desktop/src/search.ts`; `crates/cli/src/doctor.rs`.

**Accettazione osservabile:**

- Le sessioni invariate non vengono rilette; aggiunte, rotazioni ed eliminazioni aggiornano correttamente l'indice.
- Le sessioni native scadute restano ricercabili ma non possono essere riprese nativamente.
- Ricostruire l'indice operativo non rigenera o elimina mai la cronologia autorevole dei thread.

**Verifiche del passo:** `synthetic corpus at realistic size`; `incremental/watch/retention and forget tests`.

**Gate collegati, da registrare separatamente:** S6, S7, CL7, I9.

**Requisiti coperti dal piano:** FRD-0006/R08, FRD-0006/R09, FRD-0006/R10, FRD-0006/R11, FRD-0014/R11, FRD-0014/R27.

**Commit proposto:** `feat(c24): Indicizzare e cercare le sessioni in modo incrementale`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/sessions/src/index.rs, apps/desktop/src/search.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C24.1 · Creare indice FTS5 e ricerca filtrata

Indicizzazione dei transcript sintetici e query tipizzate per agente/progetto/data.

Stima: 4–8 ore. File: `crates/sessions/src/index.rs`, `apps/desktop/src/search.ts`.

Accettazione: Le query non interpolano SQL e risultati/anteprime rimangono limitati. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test ricerca FTS e filtri`; `npm run build`.

Commit: `feat(c24.1): Creare indice FTS5 e ricerca filtrata`. Registrare stato e prove sotto `progress.json` → `steps` → `C24` → `units` → `C24.1`.

### C24.2 · Aggiornare l’indice senza rileggere file invariati

Rilevamento append/rotazione/eliminazione con fingerprint e stato source-deleted.

Stima: 4–8 ore. File: `crates/sessions/src/incremental.rs`.

Accettazione: Sessioni scadute restano ricercabili e non sono proposte come nativamente riprendibili. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: corpus append/rotate/delete/retention`; `npm run build`.

Commit: `feat(c24.2): Aggiornare l’indice senza rileggere file invariati`. Registrare stato e prove sotto `progress.json` → `steps` → `C24` → `units` → `C24.2`.

### C24.3 · Aggiungere esclusione, forget e ricostruzione operativa

Comandi privacy e doctor che toccano soltanto i record previsti.

Stima: 4–8 ore. File: `crates/cli/src/doctor.rs`, `crates/sessions/src/privacy.rs`.

Accettazione: Ricostruire l’indice conserva thread autorevoli; disattivare indicizzazione e forget hanno prove negative sulle origini. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: test forget/rebuild e misure corpus`; `npm run build`.

Commit: `feat(c24.3): Aggiungere esclusione, forget e ricostruzione operativa`. Registrare stato e prove sotto `progress.json` → `steps` → `C24` → `units` → `C24.3`.

<a id="f02"></a>
## F02 · Proprietà del runtime, flusso degli eventi e rilevamento della presenza

Un runtime proprietario dei PTY per finestra, comando hook autenticato, lettori della sequenza eventi e recupero delle istanze terminate basato sull’identità del processo.

**Ambito:** v1. **Dipendenze:** FS03, C15, C11. **Stima:** 4–10 ore.

**File/componenti:** `crates/state/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

- Due finestre non devono scrivere nello stesso PTY né collegare due volte lo stesso segmento nativo.
- Heartbeat e recupero delle istanze devono tollerare la sospensione del portatile senza false segnalazioni di terminazione.
- I lettori devono recuperare gli eventi successivi al proprio seq; misurare latenza del polling e crescita dovuta alla conservazione.
- L’hook deve aggiornare evento e stato nella stessa transazione e rispettare token, stdout e contratto dei codici di uscita.

**Verifiche del passo:** `planned: Two windows cannot write one PTY or attach one native segment twice.`; `planned: Heartbeat/reaper survives laptop sleep without false dead.`; `planned: Consumers catch up by seq; polling latency and retention growth measured.`; `planned: Hook updates event/state transactionally, honors token and stdout/exit contract.`.

**Gate collegati, da registrare separatamente:** P14, S3, S8, I8, M11.

**Requisiti coperti dal piano:** FRD-0010/R05, FRD-0010/R06, FRD-0010/R07, FRD-0010/R08, FRD-0014/R01, FRD-0014/R02, FRD-0014/R03, FRD-0014/R04, FRD-0014/R20, FRD-0014/R22, FRD-0014/R23, FRD-0014/R24.

**Commit proposto:** `feat(f02): Proprietà del runtime, flusso degli eventi e rilevamento della presenza`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c25"></a>
## C25 · Aggiungere thread persistenti e ripresa esclusiva nello stesso agente

Thread di lavoro, segmenti e attivazioni persistenti, ripresa nativa esplicita e ripresa headless opzionale con avviso di costo.

**Ambito:** v1. **Dipendenze:** C22, C24, FS03, F02. **Stima:** 12–24 ore.

**File/componenti:** `crates/threads/**`; `crates/engine/src/resume.rs`; `apps/desktop/src/sessions/actions.ts`.

**Accettazione osservabile:**

- Il riavvio non perde thread; nessun riuso di PID o PTY obsoleti e nessuna ripresa automatica.
- Una sessione nativa non si collega a due istanze attive; Claude riprende soltanto nella propria cartella di lavoro.
- Un'origine nativa mancante propone correttamente un futuro avvio da seed, senza presentarlo come ripresa nativa.
- La colonna dei thread è un pannello laterale ridimensionabile e comprimibile, colorato dalla skin (FRD-0001/R12).

**Verifiche del passo:** `two-process resume race`; `resume/exit/restart fixtures`; `source-store unchanged assertion`.

**Gate collegati, da registrare separatamente:** CL4, CL15, S4, SS5.

**Requisiti coperti dal piano:** FRD-0005/R01, FRD-0005/R03, FRD-0005/R04, FRD-0005/R47, FRD-0006/R08, FRD-0006/R25, FRD-0006/R29, FRD-0014/R17, FRD-0014/R20, FRD-0001/R12.

**Commit proposto:** `feat(c25): Aggiungere thread persistenti e ripresa esclusiva nello stesso agente`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/threads/**, crates/engine/src/resume.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C25.1 · Persistire thread, segmenti e attivazioni

Schema di lavoro e record che sopravvivono a processi e terminali.

Stima: 4–8 ore. File: `crates/threads/**`, `crates/state/src/threads.rs`.

Accettazione: Riavvio e chiusura della vista non eliminano cronologia né avviano processi. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test thread/segment/activation restart`; `npm run build`.

Commit: `feat(c25.1): Persistire thread, segmenti e attivazioni`. Registrare stato e prove sotto `progress.json` → `steps` → `C25` → `units` → `C25.1`.

### C25.2 · Riprendere nativamente con esclusività

Servizio resume che risolve radice e cartella e acquisisce l’unica attivazione valida.

Stima: 4–8 ore. File: `crates/engine/src/resume.rs`.

Accettazione: Due processi concorrenti non collegano la stessa sessione; Claude riprende soltanto nella sua cwd. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: race resume e fixture cwd`; `npm run build`.

Commit: `feat(c25.2): Riprendere nativamente con esclusività`. Registrare stato e prove sotto `progress.json` → `steps` → `C25` → `units` → `C25.2`.

### C25.3 · Collegare azioni UI e ripresa headless esplicita

Resume manuale da sessione o thread con avviso di costo per headless e rifiuti chiari.

Stima: 4–8 ore. File: `apps/desktop/src/sessions/actions.ts`.

Accettazione: Origini scadute non fingono una ripresa nativa; nessuna ripresa automatica dopo restore. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: smoke resume e sessione mancante`; `npm run build`.

Commit: `feat(c25.3): Collegare azioni UI e ripresa headless esplicita`. Registrare stato e prove sotto `progress.json` → `steps` → `C25` → `units` → `C25.3`.

<a id="c26"></a>
## C26 · Aggiungere esportazioni e CLI stabile per le sessioni

Operazioni comuni a UI e CLI per sessions, find, export, resume, doctor e forget, JSON e codici di uscita stabili, con rapporti per STX, Mukade, formato nativo e Markdown.

**Ambito:** v1. **Dipendenze:** C23, C24, C25. **Stima:** 12–24 ore.

**File/componenti:** `crates/export/**`; `crates/cli/**`.

**Accettazione osservabile:**

- Il ragionamento non viene mai esportato; full e portable oscurano entrambi i segreti.
- Output atomico con regole esplicite per sovrascrittura e allegati; versioni STX superiori vengono rifiutate e i campi sconosciuti conservati.
- La CLI non suggerisce supporto all'importazione in v1; l'esportazione nativa compare soltanto dove è provata.

**Verifiche del passo:** `export roundtrip/readback fixtures`; `atomic-output failure and exit-code tests`.

**Gate collegati, da registrare separatamente:** SS6, H14, OC2.

**Requisiti coperti dal piano:** FRD-0006/R31, FRD-0006/R32, FRD-0006/R33, FRD-0006/R35, FRD-0006/R36, FRD-0006/R37, FRD-0006/R38, FRD-0006/R39.

**Commit proposto:** `feat(c26): Aggiungere esportazioni e CLI stabile per le sessioni`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/export/**, crates/cli/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C26.1 · Definire e scrivere STX versionato

Formato, checksum, provenienza e modalità full/portable con oscuramento.

Stima: 4–8 ore. File: `crates/export/src/stx.rs`.

Accettazione: Ragionamento escluso, campi ignoti conservati e versione superiore rifiutata. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fixture STX read/write/version/privacy`; `npm run build`.

Commit: `feat(c26.1): Definire e scrivere STX versionato`. Registrare stato e prove sotto `progress.json` → `steps` → `C26` → `units` → `C26.1`.

### C26.2 · Aggiungere formati e output atomico

Mukade/native/Markdown con rapporti di perdita e policy esplicite su allegati e overwrite.

Stima: 4–8 ore. File: `crates/export/src/formats.rs`, `crates/export/src/output.rs`.

Accettazione: Errore di scrittura non lascia output finale parziale né sovrascrive senza consenso. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: roundtrip export e fault atomic-output`; `npm run build`.

Commit: `feat(c26.2): Aggiungere formati e output atomico`. Registrare stato e prove sotto `progress.json` → `steps` → `C26` → `units` → `C26.2`.

### C26.3 · Esporre le stesse operazioni nella CLI

Subcomandi sessions/find/export/resume/doctor/forget con JSON e codici di uscita stabili.

Stima: 4–8 ore. File: `crates/cli/**`.

Accettazione: UI e CLI chiamano il medesimo servizio; import resta non disponibile in v1. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test CLI JSON schema e exit-code`; `npm run build`.

Commit: `feat(c26.3): Esporre le stesse operazioni nella CLI`. Registrare stato e prove sotto `progress.json` → `steps` → `C26` → `units` → `C26.3`.

<a id="f03p"></a>
## F03P · Controlli di riservatezza, doctor e ricostruzione degli indici operativi

Controllo di integrità doctor, disattivazione dell’indicizzazione per progetto, esclusione dai riepiloghi, eliminazione selettiva e modello delle minacce revisionato.

**Ambito:** v1. **Dipendenze:** FS03, C23, C24. **Stima:** 4–10 ore.

**File/componenti:** `crates/state/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

- L’eliminazione selettiva deve rimuovere i contenuti previsti e i record companion, conservando i dati autorevoli estranei.
- La ricostruzione deve rigenerare soltanto gli indici operativi.
- Database, log e acquisizioni non devono contenere credenziali o traffico in uscita non previsto.
- I percorsi SEC devono ricevere una seconda revisione e prove dei casi negativi.

**Verifiche del passo:** `planned: Forget removes specified retained content and companion records; unrelated authoritative data survives.`; `planned: Rebuild regenerates only operational indexes.`; `planned: No credentials or unintended outbound traffic in database/logs/captures.`; `planned: SEC paths receive second-reader negative-case review.`.

**Gate collegati, da registrare separatamente:** AT-6, I9, X4, X5.

**Requisiti coperti dal piano:** FRD-0014/R11, FRD-0014/R27, FRD-0014/R28, FRD-0014/R29, FRD-0014/R30, FRD-0014/R32, FRD-0017/R17, FRD-0017/R18.

**Commit proposto:** `feat(f03p): Controlli di riservatezza, doctor e ricostruzione degli indici operativi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c16"></a>
## C16 · Aggiungere l'adattatore Codex

Avvio, autenticazione, collegamento e segnali Codex attraverso lo stesso contratto, applicando i vincoli su HOME, socket e insiemi.

**Ambito:** v1. **Dipendenze:** C15. **Stima:** 12–24 ore.

**File/componenti:** `crates/adapter/src/codex/**`; `verify/CX/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- Nessuna sessione nativa ha due scrittori; la sovrapposizione privata conserva autenticazione e cronologia nelle posizioni previste dall'agente.
- Gli insiemi per istanza nella home di sistema vengono rifiutati; coda nativa e segnali dichiarati sono provati.

**Verifiche del passo:** `Codex versioned contract fixtures`; `synthetic box/system-home negative cases`.

**Gate collegati, da registrare separatamente:** CX1-CX14, CX12, D19, P20.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R08, FRD-0001/R09, FRD-0001/R10, FRD-0003/R01, FRD-0003/R02, FRD-0003/R03, FRD-0003/R04, FRD-0003/R07, FRD-0003/R10, FRD-0003/R11, FRD-0003/R12, FRD-0003/R14, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R45, FRD-0006/R46, FRD-0006/R48.

**Commit proposto:** `feat(c16): Aggiungere l'adattatore Codex`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/codex/**, verify/CX/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C16.1 · Riconoscere Codex e preparare home e stato login

Eseguibile, versione, autenticazione e vincoli di socket/overlay privato.

Stima: 4–8 ore. File: `crates/adapter/src/codex/discovery.rs`, `crates/adapter/src/codex/auth.rs`.

Accettazione: Nessuna lettura di credenziali e nessun insieme per istanza nella home di sistema. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture Codex auth/box/system`; `npm run build`.

Commit: `feat(c16.1): Riconoscere Codex e preparare home e stato login`. Registrare stato e prove sotto `progress.json` → `steps` → `C16` → `units` → `C16.1`.

### C16.2 · Collegare avvio e backend Codex

Percorso PTY o attach scelto e versionato attraverso il launch plan comune.

Stima: 4–8 ore. File: `crates/adapter/src/codex/run.rs`.

Accettazione: La TUI reale usa sidecar e checkup; nessun secondo writer della sessione nativa. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: smoke Codex e esclusività della sessione`; `npm run build`.

Commit: `feat(c16.2): Collegare avvio e backend Codex`. Registrare stato e prove sotto `progress.json` → `steps` → `C16` → `units` → `C16.2`.

### C16.3 · Provare segnali e capacità dichiarate Codex

Queue, stato e capacità native aggiunte solo con fixture installate.

Stima: 4–8 ore. File: `crates/adapter/src/codex/**`, `verify/CX/**`.

Accettazione: Una capacità fallita diventa unsupported senza compromettere l’avvio già provato. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: conformance Codex per versione`; `npm run build`.

Commit: `feat(c16.3): Provare segnali e capacità dichiarate Codex`. Registrare stato e prove sotto `progress.json` → `steps` → `C16` → `units` → `C16.3`.

<a id="c17"></a>
## C17 · Aggiungere l'adattatore OpenCode

Ciclo di vita del terminale e server OpenCode, rilevamento dell'autenticazione e contesto di avvio isolato.

**Ambito:** v1. **Dipendenze:** C15. **Stima:** 12–24 ore.

**File/componenti:** `crates/adapter/src/opencode/**`; `verify/OC/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- Il servizio dell'agente termina insieme alla finestra; il ripristino XDG dei sottoprocessi impedisce perdite nell'ambiente.
- L'autenticazione mostra Unknown quando i controlli disponibili senza segreti non soddisfano il criterio sulla versione installata.

**Verifiche del passo:** `OpenCode contract/export/service fixtures`; `XDG child-env negative cases`.

**Gate collegati, da registrare separatamente:** OC1-OC10, D17, P20.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R08, FRD-0001/R09, FRD-0001/R10, FRD-0003/R01, FRD-0003/R02, FRD-0003/R03, FRD-0003/R04, FRD-0003/R07, FRD-0003/R10, FRD-0003/R11, FRD-0003/R12, FRD-0003/R14, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R45, FRD-0006/R46, FRD-0006/R48.

**Commit proposto:** `feat(c17): Aggiungere l'adattatore OpenCode`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/opencode/**, verify/OC/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C17.1 · Riconoscere OpenCode e risolvere l’ambiente isolato

Catalogo, auth secret-free e piano XDG con ripristino per i programmi figli.

Stima: 4–8 ore. File: `crates/adapter/src/opencode/auth.rs`, `crates/adapter/src/opencode/environment.rs`.

Accettazione: Se i controlli auth disponibili non soddisfano il criterio, lo stato resta Unknown. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture auth e ambiente XDG`; `npm run build`.

Commit: `feat(c17.1): Riconoscere OpenCode e risolvere l’ambiente isolato`. Registrare stato e prove sotto `progress.json` → `steps` → `C17` → `units` → `C17.1`.

### C17.2 · Collegare terminale e servizio OpenCode

Avvio esplicito della TUI e proprietà del servizio nella finestra.

Stima: 4–8 ore. File: `crates/adapter/src/opencode/run.rs`.

Accettazione: Il servizio termina con la finestra; nessun processo di sistema dell’utente viene arrestato. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: smoke TUI e lifecycle servizio`; `npm run build`.

Commit: `feat(c17.2): Collegare terminale e servizio OpenCode`. Registrare stato e prove sotto `progress.json` → `steps` → `C17` → `units` → `C17.2`.

### C17.3 · Provare esportazione, segnali e insiemi OpenCode

Capacità disponibili dietro contratto e fixture della versione.

Stima: 4–8 ore. File: `crates/adapter/src/opencode/**`, `verify/OC/**`.

Accettazione: Esportazione e segnali usano timeout e contesto isolato; niente supporto implicito. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: suite contratto OpenCode`; `npm run build`.

Commit: `feat(c17.3): Provare esportazione, segnali e insiemi OpenCode`. Registrare stato e prove sotto `progress.json` → `steps` → `C17` → `units` → `C17.3`.

<a id="c18"></a>
## C18 · Aggiungere l'adattatore Pi

Pi avviato tramite node e vettore di argomenti, con capacità esplicite per skill, estensioni e RPC.

**Ambito:** v1. **Dipendenze:** C15. **Stima:** 12–24 ore.

**File/componenti:** `crates/adapter/src/pi/**`; `verify/PI/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- Non viene presunta una modalità di collegamento alla TUI; lo stato di accesso usa un controllo senza refresh e senza segreti.
- Le cartelle skill dell'utente non entrano negli insiemi per istanza.

**Verifiche del passo:** `Pi contract/RPC/skills fixtures`; `paths with spaces and nonASCII`.

**Gate collegati, da registrare separatamente:** PI1-PI11, D19, P20.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R08, FRD-0001/R09, FRD-0001/R10, FRD-0003/R01, FRD-0003/R02, FRD-0003/R03, FRD-0003/R04, FRD-0003/R07, FRD-0003/R10, FRD-0003/R11, FRD-0003/R12, FRD-0003/R14, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R45, FRD-0006/R48.

**Commit proposto:** `feat(c18): Aggiungere l'adattatore Pi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/pi/**, verify/PI/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C18.1 · Riconoscere Pi e definire argv senza shim

Catalogo Pi, node e cli.js, autenticazione senza refresh e vincoli di versione.

Stima: 4–8 ore. File: `crates/adapter/src/pi/discovery.rs`, `crates/adapter/src/pi/auth.rs`.

Accettazione: Percorsi con spazi/non ASCII non usano .cmd o shell; status non espone segreti. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture installazione, argv e auth Pi`; `npm run build`.

Commit: `feat(c18.1): Riconoscere Pi e definire argv senza shim`. Registrare stato e prove sotto `progress.json` → `steps` → `C18` → `units` → `C18.1`.

### C18.2 · Avviare Pi con skill ed estensioni esplicite

TUI attraverso il launch plan e insiemi che non ereditano skill globali.

Stima: 4–8 ore. File: `crates/adapter/src/pi/run.rs`, `crates/adapter/src/pi/sets.rs`.

Accettazione: Due piani con insiemi diversi non condividono accidentalmente estensioni o skill. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: smoke Pi e fixture insiemi`; `npm run build`.

Commit: `feat(c18.2): Avviare Pi con skill ed estensioni esplicite`. Registrare stato e prove sotto `progress.json` → `steps` → `C18` → `units` → `C18.2`.

### C18.3 · Collegare RPC e provare le capacità Pi

Capacità RPC e segnali effettivi con assenza esplicita di TUI attach.

Stima: 4–8 ore. File: `crates/adapter/src/pi/**`, `verify/PI/**`.

Accettazione: Le operazioni non supportate sono rifiutate e ogni capacità dichiarata ha una fixture. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: suite Pi RPC/refusal`; `npm run build`.

Commit: `feat(c18.3): Collegare RPC e provare le capacità Pi`. Registrare stato e prove sotto `progress.json` → `steps` → `C18` → `units` → `C18.3`.

<a id="c19"></a>
## C19 · Aggiungere Hermes Agent con capacità non supportate esplicite

Chat, autenticazione e gateway Hermes soltanto dove le fixture della versione installata passano; assenze di pacchetti e lettori sono visibili.

**Ambito:** v1. **Dipendenze:** C15. **Stima:** 14–28 ore.

**File/componenti:** `crates/adapter/src/hermes/**`; `verify/HM/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D20 e rinviato con D01. Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- Il gateway appartiene alla finestra e termina con essa; nella home di sistema non vengono iniettate configurazioni.
- Nessuna capacità di lettura Mukade, esportazione o hop in uscita è dichiarata prima che una revisione verificata la fornisca.
- Una procedura sconosciuta di installazione gestita fa rifiutare la modalità own.

**Verifiche del passo:** `Hermes contract and gateway lifecycle fixtures`; `no-package/no-reader negatives`.

**Gate collegati, da registrare separatamente:** HM1-HM14.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R02, FRD-0001/R08, FRD-0001/R09, FRD-0001/R10, FRD-0002/R09, FRD-0002/R15, FRD-0002/R27, FRD-0002/R31, FRD-0003/R01, FRD-0003/R02, FRD-0003/R03, FRD-0003/R05, FRD-0003/R07, FRD-0003/R08, FRD-0003/R10, FRD-0003/R11, FRD-0003/R12, FRD-0003/R14, FRD-0006/R03, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R45, FRD-0006/R46, FRD-0006/R48.

**Commit proposto:** `feat(c19): Aggiungere Hermes Agent con capacità non supportate esplicite`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/hermes/**, verify/HM/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C19.1 · Riconoscere Hermes e lo stato di autenticazione

Catalogo, status per provider e rifiuti per pacchetti o lettori mancanti.

Stima: 4–8 ore. File: `crates/adapter/src/hermes/discovery.rs`, `crates/adapter/src/hermes/auth.rs`.

Accettazione: Nessuna copia di pacchetti di altri host e nessun reader/hop-source implicito. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture Hermes discovery/auth/unsupported`; `npm run build`.

Commit: `feat(c19.1): Riconoscere Hermes e lo stato di autenticazione`. Registrare stato e prove sotto `progress.json` → `steps` → `C19` → `units` → `C19.1`.

### C19.2 · Avviare chat e possedere il gateway Hermes

Launch plan Hermes con box e lifecycle del gateway legato alla finestra.

Stima: 5–10 ore. File: `crates/adapter/src/hermes/run.rs`, `crates/adapter/src/hermes/gateway.rs`.

Accettazione: La home di sistema non riceve configurazioni iniettate e il gateway di test non sopravvive alla finestra. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: smoke chat e gateway lifecycle`; `npm run build`.

Commit: `feat(c19.2): Avviare chat e possedere il gateway Hermes`. Registrare stato e prove sotto `progress.json` → `steps` → `C19` → `units` → `C19.2`.

### C19.3 · Validare le capacità e le limitazioni Hermes

Conformance per versione, ricetta own non disponibile e insieme della home mantenuto.

Stima: 5–10 ore. File: `crates/adapter/src/hermes/**`, `verify/HM/**`.

Accettazione: Solo le capacità provate diventano disponibili; una ricetta di installazione sconosciuta rifiuta own. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: suite Hermes versionata e rifiuti`; `npm run build`.

Commit: `feat(c19.3): Validare le capacità e le limitazioni Hermes`. Registrare stato e prove sotto `progress.json` → `steps` → `C19` → `units` → `C19.3`.

<a id="c20"></a>
## C20 · Aggiungere il pannello ACP DeepSeek

Un client ACP mostra messaggi, strumenti e permessi reali dell'agente e avvia dsh web soltanto su azione esplicita dell'utente.

**Ambito:** v1. **Dipendenze:** C15. **Stima:** 20–40 ore.

**File/componenti:** `crates/adapter/src/deepseek/**`; `apps/desktop/src/acp/**`; `verify/DS/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D20 e rinviato con D01. Il controllo Daodan del checkup di FRD-0001/R08 e rinviato.

- Una richiesta di permesso attende la risposta dell'utente; nessuna esecuzione automatica di strumenti.
- DSH_HOME, DSH_AGENTS_HOME e trasporto del token MCP di sessione vengono provati o rifiutati.
- dsh web resta esterno e nessun processo browser rimane dopo la chiusura; autenticazione Unknown fino a DS9.

**Verifiche del passo:** `ACP protocol fixture replay with denied permissions`; `real installed-version canary when authorized`.

**Gate collegati, da registrare separatamente:** DS1-DS15.

**Requisiti coperti dal piano:** FRD-0001/R01, FRD-0001/R02, FRD-0001/R05, FRD-0001/R06, FRD-0001/R07, FRD-0001/R08, FRD-0002/R28, FRD-0002/R34, FRD-0003/R01, FRD-0003/R02, FRD-0003/R03, FRD-0003/R06, FRD-0003/R09, FRD-0003/R10, FRD-0003/R11, FRD-0003/R12, FRD-0003/R14, FRD-0006/R03, FRD-0006/R40, FRD-0006/R41, FRD-0006/R42, FRD-0006/R45, FRD-0006/R46, FRD-0006/R48.

**Commit proposto:** `feat(c20): Aggiungere il pannello ACP DeepSeek`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/deepseek/**, apps/desktop/src/acp/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C20.1 · Implementare handshake e sessione ACP

Client ACP con inizializzazione, session/new e cancellazione su processo di test.

Stima: 5–10 ore. File: `crates/adapter/src/deepseek/acp.rs`.

Accettazione: Messaggi malformati, timeout e uscita del figlio producono errori visibili senza bloccare l’app. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture handshake/cancel/timeouts ACP`; `npm run build`.

Commit: `feat(c20.1): Implementare handshake e sessione ACP`. Registrare stato e prove sotto `progress.json` → `steps` → `C20` → `units` → `C20.1`.

### C20.2 · Disegnare messaggi e strumenti ACP

Pannello ACP integrato in schede e split con stream reali del protocollo.

Stima: 5–10 ore. File: `apps/desktop/src/acp/**`, `crates/adapter/src/deepseek/events.rs`.

Accettazione: Nessuna lettura dello schermo simulata; output resta limitato e il pannello si sposta senza riavvio. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: replay ACP e smoke pannello`; `npm run build`.

Commit: `feat(c20.2): Disegnare messaggi e strumenti ACP`. Registrare stato e prove sotto `progress.json` → `steps` → `C20` → `units` → `C20.2`.

### C20.3 · Collegare richieste di permesso e input utente

Relè esplicito di prompt e risposte session/request_permission.

Stima: 5–10 ore. File: `crates/adapter/src/deepseek/permissions.rs`, `apps/desktop/src/acp/permissions.ts`.

Accettazione: Uno strumento non parte prima della risposta; annullamento e rifiuto preservano la richiesta corretta. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: canary ACP deny/allow/cancel`; `npm run build`.

Commit: `feat(c20.3): Collegare richieste di permesso e input utente`. Registrare stato e prove sotto `progress.json` → `steps` → `C20` → `units` → `C20.3`.

### C20.4 · Integrare contesto DeepSeek e accesso web esterno

Home DSH, token MCP, stato auth Unknown e avvio dsh web soltanto su richiesta.

Stima: 5–10 ore. File: `crates/adapter/src/deepseek/**`, `verify/DS/**`.

Accettazione: Nessun web incorporato o processo residuo; capacità non provate restano unsupported. La compilazione e l'avvio dell'app esistente restano funzionanti; le capacità successive restano indisponibili.

Verifiche pianificate: `planned: fixture DS home/token e lifecycle web`; `npm run build`.

Commit: `feat(c20.4): Integrare contesto DeepSeek e accesso web esterno`. Registrare stato e prove sotto `progress.json` → `steps` → `C20` → `units` → `C20.4`.

<a id="c27"></a>
## C27 · Creare anteprime seed esatte e oscurate

Budget seed deterministico, rapporto delle perdite, file della trascrizione completa e anteprima immutabile dopo oscuramento, senza effetti sulla destinazione.

**Ambito:** v1. **Dipendenze:** C23, C25, C26, C13. **Stima:** 16–32 ore.

**File/componenti:** `crates/hop/src/seed.rs`; `crates/hop/src/redact.rs`; `apps/desktop/src/hop-preview.ts`.

**Accettazione osservabile:**

- Tutta la cronologia selezionata è rappresentata entro il budget della destinazione con omissioni segnate e tagli rispettosi di turni e UTF-8.
- Ragionamento e vecchi seed annidati sono esclusi; il corpus dei segreti verifica l'oscuramento prima del salvataggio; i byte dell'anteprima coincidono con l'istantanea inviata.
- Nessun file di istruzioni viene creato nel progetto senza consenso; prefissi bootstrap pericolosi non vengono eseguiti; i file completi sono privati.

**Verifiche del passo:** `budget/redaction/hostile-prefix property corpus`; `preview hash and private-file fixtures`.

**Gate collegati, da registrare separatamente:** H2, H3, H4, H7, H11, H14, H16, H18, CL16, SC9, CL2, CL3, CL14, H23, E3.

**Requisiti coperti dal piano:** FRD-0003/R13, FRD-0005/R05, FRD-0005/R06, FRD-0005/R07, FRD-0005/R08, FRD-0005/R09, FRD-0005/R19, FRD-0005/R20, FRD-0005/R21, FRD-0005/R22, FRD-0005/R23, FRD-0005/R24, FRD-0005/R25, FRD-0005/R26, FRD-0005/R27, FRD-0005/R28, FRD-0005/R29, FRD-0005/R31, FRD-0005/R33, FRD-0006/R39.

**Commit proposto:** `feat(c27): Creare anteprime seed esatte e oscurate`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/hop/src/seed.rs, crates/hop/src/redact.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C27.1 · Normalizzare cronologia e oscurare i segreti

Proiezione seed priva di ragionamento, rumore e seed annidati, oscurata prima del salvataggio.

Stima: 4–8 ore. File: `crates/hop/src/redact.rs`, `crates/hop/src/history.rs`.

Accettazione: Corpus di chiavi e prefissi ostili prova richiami e falsi positivi; nessun testo non oscurato viene persistito. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus redaction e normalized-history`; `npm run build`.

Commit: `feat(c27.1): Normalizzare cronologia e oscurare i segreti`. Registrare stato e prove sotto `progress.json` → `steps` → `C27` → `units` → `C27.1`.

### C27.2 · Applicare budget e tagli deterministici

Calcolo dalla finestra del modello, ordine di conservazione e omissioni segnate.

Stima: 4–8 ore. File: `crates/hop/src/seed.rs`.

Accettazione: Tagli solo ai confini UTF-8/turno; minimo seed insufficiente causa rifiuto con numeri. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: proprietà budget e destination-window`; `npm run build`.

Commit: `feat(c27.2): Applicare budget e tagli deterministici`. Registrare stato e prove sotto `progress.json` → `steps` → `C27` → `units` → `C27.2`.

### C27.3 · Preparare file privati e canale sicuro

Trascrizione completa e bootstrap limitato senza shell, con posizioni autorizzate.

Stima: 4–8 ore. File: `crates/hop/src/files.rs`, `crates/adapter/src/seed_channel.rs`.

Accettazione: Nessun file istruzioni viene creato nel progetto senza consenso; file grandi sono letti con limiti. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test full-file ACL e bootstrap escaping`; `npm run build`.

Commit: `feat(c27.3): Preparare file privati e canale sicuro`. Registrare stato e prove sotto `progress.json` → `steps` → `C27` → `units` → `C27.3`.

### C27.4 · Mostrare l’anteprima immutabile e le perdite

Dialogo seed con hash della proiezione, quantità oscurate e conferma separata dagli effetti.

Stima: 4–8 ore. File: `apps/desktop/src/hop-preview.ts`, `crates/hop/src/preview.rs`.

Accettazione: L’istantanea inviata coincide byte per byte con l’anteprima e attraversa invariata il bypass sidecar. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fixture preview/send/hash e SC9`; `npm run build`.

Commit: `feat(c27.4): Mostrare l’anteprima immutabile e le perdite`. Registrare stato e prove sotto `progress.json` → `steps` → `C27` → `units` → `C27.4`.

<a id="c28"></a>
## C28 · Implementare il coordinatore degli hop close/fork con registro persistente

Macchina a stati transazionale degli hop con identità distinte per close e fork, avvio confermato della destinazione, verifica hash e ricevuta, errori visibili e passaggio da cronologia.

**Ambito:** v1. **Dipendenze:** C27. **Stima:** 20–40 ore.

**File/componenti:** `crates/hop/src/coordinator.rs`; `crates/state/src/hops.rs`; `apps/desktop/src/hop/**`.

**Accettazione osservabile:**

- Esattamente un hop concorrente vince; la conferma attesta che nessun effetto sulla destinazione è avvenuto prima del consenso.
- L'origine sopravvive ai guasti e resta attiva nel fork; si chiude soltanto dopo il commit e la gestione confermata di bozze o comandi attivi.
- Il passaggio da cronologia crea un segmento storico derivato senza toccare un'origine attiva.
- Un database occupato o pieno non consente di procedere senza registro; nessuna risposta automatica alle domande iniziali di fiducia.
- Il dialogo hop è un overlay che lascia intatto lo schermo dell'emulatore e restituisce il focus al pannello di origine (FRD-0001/R12).

**Verifiche del passo:** `fake-adapter state transition suite`; `concurrent-hop test`; `effect-boundary fault injection`.

**Gate collegati, da registrare separatamente:** S4, S10, H1, H5, H9, H12, H24, H25, H26, SS4, AT-1, AT-4, AT-10, CL12, H8, H17, H22.

**Requisiti coperti dal piano:** FRD-0005/R02, FRD-0005/R10, FRD-0005/R11, FRD-0005/R12, FRD-0005/R13, FRD-0005/R14, FRD-0005/R15, FRD-0005/R18, FRD-0005/R32, FRD-0005/R35, FRD-0005/R36, FRD-0005/R37, FRD-0005/R38, FRD-0005/R39, FRD-0005/R40, FRD-0005/R43, FRD-0005/R44, FRD-0005/R48, FRD-0005/R49, FRD-0005/R50, FRD-0005/R51, FRD-0006/R26, FRD-0006/R27, FRD-0014/R19, FRD-0014/R21, FRD-0001/R12.

**Commit proposto:** `feat(c28): Implementare il coordinatore degli hop close/fork con registro persistente`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/hop/src/coordinator.rs, crates/state/src/hops.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C28.1 · Persistire la macchina a stati e l’esclusione degli hop

Transizioni e hop_event nello stesso commit SQLite, con unico hop attivo per thread.

Stima: 5–10 ore. File: `crates/state/src/hops.rs`, `crates/hop/src/state.rs`.

Accettazione: Due processi concorrenti producono un vincitore; transizioni non valide non mutano il registro. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test FSM e concurrent-hop`; `npm run build`.

Commit: `feat(c28.1): Persistire la macchina a stati e l’esclusione degli hop`. Registrare stato e prove sotto `progress.json` → `steps` → `C28` → `units` → `C28.1`.

### C28.2 · Collegare consenso, preparazione e ricevuta

Coordinatore con effetti registrati prima dell’avvio, hash della prima voce e nonce.

Stima: 5–10 ore. File: `crates/hop/src/coordinator.rs`.

Accettazione: Nessun effetto prima della conferma; errore o timeout lasciano l’origine utilizzabile. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fake-adapter prepare/receipt/failure`; `npm run build`.

Commit: `feat(c28.2): Collegare consenso, preparazione e ricevuta`. Registrare stato e prove sotto `progress.json` → `steps` → `C28` → `units` → `C28.2`.

### C28.3 · Implementare switch e differenza close/fork

Identità thread corrette e chiusura origine soltanto dopo COMMITTED e controlli sulle bozze.

Stima: 5–10 ore. File: `crates/hop/src/switch.rs`, `apps/desktop/src/hop/**`.

Accettazione: Fork lascia origine intatta e crea un nuovo thread; SQLite busy/full non permette effetti senza registro. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: matrice close/fork/commit/draft`; `npm run build`.

Commit: `feat(c28.3): Implementare switch e differenza close/fork`. Registrare stato e prove sotto `progress.json` → `steps` → `C28` → `units` → `C28.3`.

### C28.4 · Aggiungere il passaggio da sessione storica

Origine storica senza attivazione viva, fingerprint e nuovo thread derivato.

Stima: 5–10 ore. File: `crates/hop/src/history_handoff.rs`, `apps/desktop/src/sessions/actions.ts`.

Accettazione: Nessun wait-idle o stop del processo storico; un cambiamento dell’origine richiede nuova anteprima. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test handoff-history e source-race`; `npm run build`.

Commit: `feat(c28.4): Aggiungere il passaggio da sessione storica`. Registrare stato e prove sotto `progress.json` → `steps` → `C28` → `units` → `C28.4`.

<a id="c29"></a>
## C29 · Aggiungere recupero degli hop, aggiornamento, ritorno e standby misurato

Riconciliazione al riavvio senza duplicare avvii incerti, aggiornamento limitato, ritorno nativo/UNDO e standby non inizializzato attivabile esplicitamente.

**Ambito:** v1. **Dipendenze:** C28. **Stima:** 20–40 ore.

**File/componenti:** `crates/hop/src/recovery.rs`; `crates/hop/src/catchup.rs`; `crates/hop/src/standby.rs`.

**Accettazione osservabile:**

- Un crash in ogni stato dell'hop produce gli esiti documentati senza eliminare record nativi o duplicare sessioni.
- Istantanea, hash della prima voce e nonce vengono verificati; nuovi turni nell'origine attivano al massimo due aggiornamenti.
- Lo standby è inizialmente disattivato, non usa token prima del seed e scade dopo trenta minuti o un cambio di versione.

**Verifiche del passo:** `kill-at-each-transition matrix`; `history-only canary and hop-back tests when synthetic live budget authorized`; `warm/cold latency measurements`.

**Gate collegati, da registrare separatamente:** H1, H6, H9, H10, H12, H13, H15, H21, H24, AT-1, AT-2, AT-3.

**Requisiti coperti dal piano:** FRD-0005/R06, FRD-0005/R34, FRD-0005/R37, FRD-0005/R38, FRD-0005/R39, FRD-0005/R40, FRD-0005/R41, FRD-0005/R42, FRD-0005/R45, FRD-0005/R46, FRD-0005/R47, FRD-0005/R51.

**Commit proposto:** `feat(c29): Aggiungere recupero degli hop, aggiornamento, ritorno e standby misurato`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/hop/src/recovery.rs, crates/hop/src/catchup.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C29.1 · Riconciliare gli avvii incerti dopo crash

Recupero per stato e osservazione di effetti già tentati senza rilancio automatico.

Stima: 5–10 ore. File: `crates/hop/src/recovery.rs`.

Accettazione: Un kill in ogni confine transazionale conserva integrity_check e non duplica destinazioni. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fault matrix recovery per stato`; `npm run build`.

Commit: `feat(c29.1): Riconciliare gli avvii incerti dopo crash`. Registrare stato e prove sotto `progress.json` → `steps` → `C29` → `units` → `C29.1`.

### C29.2 · Inviare aggiornamenti limitati durante lo switch

Cursor e catch-up dei turni nuovi con massimo due passaggi e stesso controllo ricevuta.

Stima: 5–10 ore. File: `crates/hop/src/catchup.rs`.

Accettazione: Origine occupata offre attendi/interrompi/annulla e non viene fermata silenziosamente. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test source-gained-turns e busy-source`; `npm run build`.

Commit: `feat(c29.2): Inviare aggiornamenti limitati durante lo switch`. Registrare stato e prove sotto `progress.json` → `steps` → `C29` → `units` → `C29.2`.

### C29.3 · Riprendere il segmento precedente e applicare UNDO

Hop-back nativo, finestra UNDO e ripiego seed quando l’origine non esiste più.

Stima: 5–10 ore. File: `crates/hop/src/hop_back.rs`.

Accettazione: Nessuna doppia attivazione; un canary presente soltanto nella cronologia resta disponibile al ritorno. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test hop-back/undo/retention`; `npm run build`.

Commit: `feat(c29.3): Riprendere il segmento precedente e applicare UNDO`. Registrare stato e prove sotto `progress.json` → `steps` → `C29` → `units` → `C29.3`.

### C29.4 · Aggiungere standby opt-in e misurare le latenze

Istanza nascosta non inizializzata, scadenza e misure warm/cold senza promesse non provate.

Stima: 5–10 ore. File: `crates/hop/src/standby.rs`, `docs/verification/**`.

Accettazione: Off iniziale, zero token prima del seed e scadenza dopo trenta minuti o cambio versione. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test standby expiry e misure warm/cold`; `npm run build`.

Commit: `feat(c29.4): Aggiungere standby opt-in e misurare le latenze`. Registrare stato e prove sotto `progress.json` → `steps` → `C29` → `units` → `C29.4`.

<a id="c30"></a>
## C30 · Completare la matrice degli hop ordinati tra i sei agenti

Prove con canary per ogni combinazione ordinata origine, destinazione e versione; origini sconosciute o illeggibili vengono disattivate singolarmente.

**Ambito:** v1. **Dipendenze:** C19, C20, C29, C16, C17, C18. **Stima:** 16–32 ore.

**File/componenti:** `verify/H1/**`; `docs/verification/**`.

**Accettazione osservabile:**

- Ogni combinazione pubblicizzata supera il canary basato soltanto sulla cronologia oppure rifiuta o degrada esplicitamente.
- Hermes e DeepSeek diventano origini soltanto quando esistono lettori Mukade verificati; Gemini resta escluso.
- Nessuna promessa di trasferimento senza perdite; latenze a caldo e a freddo, dati trasferiti o persi e ragioni dei ripieghi sono documentati.

**Verifiche del passo:** `all ordered pairs synthetic-fixture suite`; `separately authorized bounded live canaries`.

**Gate collegati, da registrare separatamente:** H1, HM3, DS3, AT-2, AT-10.

**Requisiti coperti dal piano:** FRD-0005/R06, FRD-0005/R14, FRD-0005/R15, FRD-0005/R18, FRD-0006/R41, FRD-0006/R42.

**Commit proposto:** `feat(c30): Completare la matrice degli hop ordinati tra i sei agenti`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/H1/**, docs/verification/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C30.1 · Costruire il banco delle combinazioni ordinate

Generatore di fixture sintetiche e matrice capacità/versioni per tutte le coppie.

Stima: 4–8 ore. File: `verify/H1/**`, `crates/hop/tests/pairs.rs`.

Accettazione: Ogni combinazione non implementata o non leggibile è esplicitamente rifiutata; Gemini escluso. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test enumerazione e capability matrix`; `npm run build`.

Commit: `feat(c30.1): Costruire il banco delle combinazioni ordinate`. Registrare stato e prove sotto `progress.json` → `steps` → `C30` → `units` → `C30.1`.

### C30.2 · Eseguire canary e ritorno per coppie disponibili

Prove autorizzate del contenuto presente soltanto nella cronologia e hop-back.

Stima: 6–12 ore. File: `verify/H1/**`, `docs/verification/**`.

Accettazione: Usare solo home, account e budget sintetici autorizzati; registrare anche ogni rifiuto e risultato negativo. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: matrice canary per coppia/versione`; `npm run build`.

Commit: `feat(c30.2): Eseguire canary e ritorno per coppie disponibili`. Registrare stato e prove sotto `progress.json` → `steps` → `C30` → `units` → `C30.2`.

### C30.3 · Pubblicare la matrice di supporto effettiva

Capacità UI/doctor derivate dalle prove, con latenze e perdita dichiarate.

Stima: 6–12 ore. File: `crates/adapter/src/capabilities.rs`, `apps/desktop/src/hop/**`.

Accettazione: Hermes/DeepSeek sono origini solo dopo reader verificati; versioni fuori fixture degradano senza falsa promessa lossless. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test support-report e version drift`; `npm run build`.

Commit: `feat(c30.3): Pubblicare la matrice di supporto effettiva`. Registrare stato e prove sotto `progress.json` → `steps` → `C30` → `units` → `C30.3`.

<a id="f03h"></a>
## F03H · Prove delle transazioni hop, dell’unicità e del recupero dopo arresto anomalo

Chiusura delle verifiche hop sullo stato condiviso mediante casi concorrenti e interruzioni delle transazioni.

**Ambito:** v1. **Dipendenze:** F03P, C29. **Stima:** 3–8 ore.

**File/componenti:** `crates/state/`; `crates/hop/`; `docs/verification/`.

**Accettazione osservabile:**

- Tra due hop concorrenti deve riuscirne esattamente uno.
- Ogni effetto deve seguire il journal e il recupero deve essere idempotente.
- L’integrità deve restare valida dopo arresti anomali indotti; l’archivio nativo sorgente deve rimanere invariato.

**Verifiche del passo:** `planned: Exactly one concurrent hop wins.`; `planned: Every effect follows journal and recovery is idempotent.`; `planned: Integrity remains clean after injected crashes; source native store unchanged.`.

**Gate collegati, da registrare separatamente:** S4, S10, AT-1, AT-4.

**Requisiti coperti dal piano:** FRD-0014/R12, FRD-0014/R19, FRD-0014/R21.

**Commit proposto:** `feat(f03h): Prove delle transazioni hop, dell’unicità e del recupero dopo arresto anomalo`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/state/, crates/hop/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c31"></a>
## C31 · Costruire l'API di messaggistica limitata e circoscritta tra istanze

Motore tipizzato instances, send, inbox e ack, messaggi per sessione con scadenza, deduplicazione, ambiti e controllo degli eccessi.

**Ambito:** v1. **Dipendenze:** C25, C27, F02. **Stima:** 14–28 ore.

**File/componenti:** `crates/messaging/**`; `crates/state/src/messages.rs`.

**Accettazione osservabile:**

- Il comportamento predefinito rifiuta messaggi tra progetti; la consegna almeno una volta non viene chiamata lettura o consegna esattamente una volta.
- Il contenuto dei pari resta dato citato, non affidabile e con escape; non risponde mai ai permessi e non causa da solo chiamate a strumenti.
- Scadenza entro un'ora, massimo cinque tentativi e limiti di profondità e frequenza sono imposti dal server; destinatari offline e duplicati sono verificati.
- Nessuno scheduler o casella persistente di attività, né facoltà dell'agente di avviare hop, istanze o modificare impostazioni.

**Verifiche del passo:** `offline/duplicate/pingpong/cross-project tests`; `injection canaries and no-approval negatives`.

**Gate collegati, da registrare separatamente:** M4-M7, AT-6, AT-7.

**Requisiti coperti dal piano:** FRD-0007/R01, FRD-0007/R02, FRD-0007/R03, FRD-0007/R04, FRD-0007/R05, FRD-0007/R06, FRD-0007/R07, FRD-0007/R08, FRD-0007/R09, FRD-0007/R10, FRD-0007/R11, FRD-0007/R12, FRD-0007/R13, FRD-0007/R14, FRD-0007/R15, FRD-0007/R16, FRD-0007/R17, FRD-0007/R23, FRD-0007/R24, FRD-0007/R39, FRD-0014/R20.

**Commit proposto:** `feat(c31): Costruire l'API di messaggistica limitata e circoscritta tra istanze`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/messaging/**, crates/state/src/messages.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C31.1 · Persistire messaggi con ambito, scadenza e deduplicazione

API tipizzata instances/send/inbox/ack con vincoli e quoted-data.

Stima: 4–8 ore. File: `crates/messaging/src/model.rs`, `crates/state/src/messages.rs`.

Accettazione: Cross-project deny iniziale, corpo limitato e scadenza entro un’ora; segreti oscurati prima del salvataggio. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test scope/expiry/dedupe/redaction`; `npm run build`.

Commit: `feat(c31.1): Persistire messaggi con ambito, scadenza e deduplicazione`. Registrare stato e prove sotto `progress.json` → `steps` → `C31` → `units` → `C31.1`.

### C31.2 · Consegnare con retry e stato dichiarato correttamente

Coda bounded, al-meno-una-volta e canali per capacità del destinatario.

Stima: 5–10 ore. File: `crates/messaging/src/delivery.rs`.

Accettazione: Massimo cinque retry; delivered non significa read e destinatari offline oltre scadenza non ricevono il messaggio. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test offline/retry/ack/channel`; `npm run build`.

Commit: `feat(c31.2): Consegnare con retry e stato dichiarato correttamente`. Registrare stato e prove sotto `progress.json` → `steps` → `C31` → `units` → `C31.2`.

### C31.3 · Bloccare cicli e messaggi che simulano approvazioni

Limiti server su profondità e frequenza con allerta visibile e canary di iniezione.

Stima: 5–10 ore. File: `crates/messaging/src/limits.rs`, `crates/messaging/tests/injection.rs`.

Accettazione: I messaggi non rispondono ai permessi e non concedono strumenti; il ping-pong si ferma senza perdita silenziosa. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: canary approval e test flood/depth`; `npm run build`.

Commit: `feat(c31.3): Bloccare cicli e messaggi che simulano approvazioni`. Registrare stato e prove sotto `progress.json` → `steps` → `C31` → `units` → `C31.3`.

<a id="c32"></a>
## C32 · Esporre quattro strumenti MCP e hook iniettati all'avvio

Punti di ingresso MCP e hook nello stesso eseguibile, verifica per avvio dei token salvati come hash e soltanto quattro strumenti approvati.

**Ambito:** v1. **Dipendenze:** C31, C11, C14, C16, C17, C18, C19, C20. **Stima:** 18–36 ore.

**File/componenti:** `crates/cli/src/mcp.rs`; `crates/cli/src/hook.rs`; `crates/adapter/src/injection/**`.

**Accettazione osservabile:**

- Identità sconosciute, terminate o con token non corrispondente vengono rifiutate; nessun token in chiaro in database, log o trascrizioni.
- Stdout MCP contiene soltanto il protocollo; gli hook non fanno fallire gli agenti; entrambi terminano con il proprietario.
- Nessun ripiego di consegna al PTY non sicuro; la preapprovazione riguarda soltanto i quattro strumenti nominati ed è verificata per versione.

**Verifiche del passo:** `stdio JSONRPC and malformed-input tests`; `impersonation/token-negative fixtures`; `startup/hook latency measurement`; `per-agent hook/injection conformance`.

**Gate collegati, da registrare separatamente:** M1-M3, M8-M15, CL8-CL11, OC6, PI7, HM5, DS5, SC8, T1.

**Requisiti coperti dal piano:** FRD-0002/R30, FRD-0002/R31, FRD-0002/R32, FRD-0002/R33, FRD-0002/R34, FRD-0006/R45, FRD-0007/R18, FRD-0007/R19, FRD-0007/R20, FRD-0007/R21, FRD-0007/R22, FRD-0007/R23, FRD-0007/R24, FRD-0007/R25, FRD-0007/R39, FRD-0007/R40.

**Commit proposto:** `feat(c32): Esporre quattro strumenti MCP e hook iniettati all'avvio`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/cli/src/mcp.rs, crates/cli/src/hook.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C32.1 · Esporre quattro strumenti MCP con stdout pulito

Subcomando stdio JSON-RPC che chiama soltanto instances/send/inbox/ack.

Stima: 6–12 ore. File: `crates/cli/src/mcp.rs`.

Accettazione: Input malformati sono rifiutati senza stampare diagnostica nel protocollo; nessuno strumento di avvio/hop/roundtable. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test stdio MCP schema e malformed-input`; `npm run build`.

Commit: `feat(c32.1): Esporre quattro strumenti MCP con stdout pulito`. Registrare stato e prove sotto `progress.json` → `steps` → `C32` → `units` → `C32.1`.

### C32.2 · Verificare identità e vita del chiamante MCP

Controllo di istanza attiva e token hash, preapprovazione dei soli quattro strumenti.

Stima: 6–12 ore. File: `crates/messaging/src/auth.rs`, `crates/adapter/src/injection/**`.

Accettazione: Token errati, identità di pari e istanze terminate non possono inviare o confermare come un altro agente. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test impersonation/token/ended-instance`; `npm run build`.

Commit: `feat(c32.2): Verificare identità e vita del chiamante MCP`. Registrare stato e prove sotto `progress.json` → `steps` → `C32` → `units` → `C32.2`.

### C32.3 · Collegare hook e misurare avvio/uscita

Hook per evento con aggiornamento atomico, output previsto dall’harness e lifecycle del proprietario.

Stima: 6–12 ore. File: `crates/cli/src/hook.rs`, `crates/adapter/src/injection/**`.

Accettazione: Gli hook non fanno fallire l’agente; resume/clear non perdono identità e il bili shim coesiste. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: conformance hook e misure startup/latency`; `npm run build`.

Commit: `feat(c32.3): Collegare hook e misurare avvio/uscita`. Registrare stato e prove sotto `progress.json` → `steps` → `C32` → `units` → `C32.3`.

<a id="c33"></a>
## C33 · Consegnare una tavola rotonda headless con limiti rigidi

Proposte indipendenti, critica e sintesi moderate avviate dall'utente, con modello per partecipante, limiti e risultati parziali.

**Ambito:** v1. **Dipendenze:** C32, C13, C24, C28, C29. **Stima:** 20–40 ore.

**File/componenti:** `crates/roundtable/**`; `apps/desktop/src/roundtable/**`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate R1, che legge moe di Daodan, e rinviato con D02.

- Soltanto nuove sessioni headless; nessun ripiego su PTY o lettura dello schermo e nessun riuso dei lavoratori attivi.
- I canary answer-only provano l'assenza di scritture, rete e strumenti senza permesso esplicito; l'iniezione tra partecipanti viene misurata.
- Limiti di costo, token e round, arresto e intervento dell'utente funzionano; risultati ricercabili e continuazione tramite seed di un thread di lavoro.
- I partecipanti di questo ciclo usano gli harness supportati; il bridge Daodan per bare-model e rinviato a D02, senza un sostituto costruito appositamente.

**Verifiche del passo:** `fake-headless timeout/cap/stop suite`; `per-agent deny-tools canaries`; `authorized three-topic cap calibration`.

**Gate collegati, da registrare separatamente:** R2-R7, AT-8, SC10.

**Requisiti coperti dal piano:** FRD-0007/R26, FRD-0007/R27, FRD-0007/R28, FRD-0007/R30, FRD-0007/R31, FRD-0007/R32, FRD-0007/R33, FRD-0007/R34, FRD-0007/R35, FRD-0007/R36, FRD-0007/R37.

**Commit proposto:** `feat(c33): Consegnare una tavola rotonda headless con limiti rigidi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/roundtable/**, apps/desktop/src/roundtable/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C33.1 · Avviare partecipanti headless con politica answer-only

Sessioni nuove per partecipante e modelli per lancio, senza riciclare worker o leggere PTY.

Stima: 5–10 ore. File: `crates/roundtable/src/participants.rs`.

Accettazione: I canary provano assenza di scritture e strumenti non autorizzati; host non conformi restano esclusi. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fake-headless e deny-tools per adattatore`; `npm run build`.

Commit: `feat(c33.1): Avviare partecipanti headless con politica answer-only`. Registrare stato e prove sotto `progress.json` → `steps` → `C33` → `units` → `C33.1`.

### C33.2 · Coordinare proposte, critica e sintesi

Round moderati di harness supportati con dati dei pari citati; il bridge Daodan per bare-model e rinviato.

Stima: 5–10 ore. File: `crates/roundtable/src/coordinator.rs`.

Accettazione: Le risposte non autorizzano strumenti e tutto il traffico resta coperto dal sidecar. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test round ordering e cross-participant injection`; `npm run build`.

Commit: `feat(c33.2): Coordinare proposte, critica e sintesi`. Registrare stato e prove sotto `progress.json` → `steps` → `C33` → `units` → `C33.2`.

### C33.3 · Imporre limiti, stop e interventi dell’utente

Budget rigidi e stato parziale con interrupt alla prima soglia raggiunta.

Stima: 5–10 ore. File: `crates/roundtable/src/caps.rs`.

Accettazione: Stop/costo/token/round non proseguono silenziosamente; interiezioni arrivano solo tra round e le misure sono registrate. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test caps/timeout/interject e calibrazione autorizzata`; `npm run build`.

Commit: `feat(c33.3): Imporre limiti, stop e interventi dell’utente`. Registrare stato e prove sotto `progress.json` → `steps` → `C33` → `units` → `C33.3`.

### C33.4 · Mostrare, cercare e continuare la discussione

UI roundtable e thread persistente, con seed di un thread di lavoro dalla posizione scelta.

Stima: 5–10 ore. File: `apps/desktop/src/roundtable/**`, `crates/roundtable/src/history.rs`.

Accettazione: Il risultato è ricercabile e continuare non riprende una sessione viva di un partecipante. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: smoke roundtable/search/continue`; `npm run build`.

Commit: `feat(c33.4): Mostrare, cercare e continuare la discussione`. Registrare stato e prove sotto `progress.json` → `steps` → `C33` → `units` → `C33.4`.

<a id="c34"></a>
## C34 · Mostrare lo stato Daodan in sola lettura e digitare i comandi

Lettore agenda con schema e limiti, collegamenti derivati tra thread e attività, righe nei progetti, palette e proposta iniziale una sola volta.

**Ambito:** deferred. **Dipendenze:** C14, C25, C32, D01. **Stima:** 16–32 ore.

**File/componenti:** `crates/daodan/src/agenda.rs`; `crates/daodan/src/links.rs`; `apps/desktop/src/tasks/**`; `apps/desktop/src/palette.ts`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su richiesta esplicita dell utente del 2026-11-02: ignorare per adesso l integrazione con Daodan. Riprendere solo su nuova indicazione.

- Shinatama non scrive file Daodan, non esegue i suoi script e non ne modifica i worktree; i comandi scelti vengono digitati e mai inviati.
- Agenda sconosciuta, illeggibile o su disco rimosso è visibile; l'ultima copia osservata serve solo alla visualizzazione; il titolare remoto non è mai giudicato obsoleto.
- I collegamenti esatti o per posizione spariscono quando non validi; gli agenti senza pacchetto mostrano le attività ma nessun comando.
- Rifiutare l'inizializzazione di un nuovo progetto sopprime le proposte successive; soltanto due verifiche di directory definiscono lo stato iniziale.

**Verifiche del passo:** `synthetic/torn/unreadable agenda fixtures`; `main-checkout/worktree link corpus`; `typed-not-sent terminal assertions`; `first-use parity tests`.

**Gate collegati, da registrare separatamente:** D8-D12, D14, D20.

**Requisiti coperti dal piano:** FRD-0004/R09, FRD-0004/R14, FRD-0004/R17, FRD-0004/R23, FRD-0004/R24, FRD-0004/R25, FRD-0008/R11, FRD-0008/R12, FRD-0008/R13, FRD-0008/R14, FRD-0008/R15, FRD-0008/R16, FRD-0008/R17, FRD-0008/R18, FRD-0008/R19, FRD-0008/R20, FRD-0008/R21, FRD-0008/R22, FRD-0008/R23, FRD-0008/R24, FRD-0008/R25, FRD-0008/R26, FRD-0008/R27, FRD-0008/R34, FRD-0008/R35, FRD-0008/R36, FRD-0008/R37, FRD-0008/R38, FRD-0008/R39, FRD-0008/R40.

**Commit proposto:** `feat(c34): Mostrare lo stato Daodan in sola lettura e digitare i comandi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/daodan/src/agenda.rs, crates/daodan/src/links.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C34.1 · Leggere agenda e risolvere il checkout principale

Lettore schema-checked e polling limitato della sorgente Daodan in sola lettura.

Stima: 4–8 ore. File: `crates/daodan/src/agenda.rs`.

Accettazione: Schema sconosciuto, lettura interrotta e disco mancante sono errori visibili, non elenchi vuoti. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus agenda/schema/torn/common-dir`; `npm run build`.

Commit: `feat(c34.1): Leggere agenda e risolvere il checkout principale`. Registrare stato e prove sotto `progress.json` → `steps` → `C34` → `units` → `C34.1`.

### C34.2 · Derivare collegamenti tra attività e thread

Link esatto o per posizione ricostruibile con stato del titolare.

Stima: 4–8 ore. File: `crates/daodan/src/links.rs`.

Accettazione: Il link sparisce se non valido; titolari remoti non sono giudicati obsoleti e attività finite non offrono continue. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fixture worktree/branch/session links`; `npm run build`.

Commit: `feat(c34.2): Derivare collegamenti tra attività e thread`. Registrare stato e prove sotto `progress.json` → `steps` → `C34` → `units` → `C34.2`.

### C34.3 · Mostrare attività e digitare comandi della palette

Righe progetto e comandi nativi dal manifesto, inseriti nel prompt senza invio.

Stima: 4–8 ore. File: `apps/desktop/src/tasks/**`, `apps/desktop/src/palette.ts`.

Accettazione: Nessun file o script Daodan viene scritto/eseguito; Hermes/DeepSeek mostrano attività senza comandi non disponibili. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: smoke typed-not-sent e no-package`; `npm run build`.

Commit: `feat(c34.3): Mostrare attività e digitare comandi della palette`. Registrare stato e prove sotto `progress.json` → `steps` → `C34` → `units` → `C34.3`.

### C34.4 · Aggiungere la proposta iniziale una sola volta

Due verifiche di directory e memoria per-progetto di Not now, coordinata con la creazione.

Stima: 4–8 ore. File: `crates/daodan/src/onboarding.rs`, `apps/desktop/src/projects/onboarding.ts`.

Accettazione: Rifiutare nella creazione sopprime la proposta successiva; Shinatama non domanda o risponde alle domande Daodan. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: first-use parity e offer-once`; `npm run build`.

Commit: `feat(c34.4): Aggiungere la proposta iniziale una sola volta`. Registrare stato e prove sotto `progress.json` → `steps` → `C34` → `units` → `C34.4`.

<a id="c35"></a>
## C35 · Trasferire il contesto delle attività Daodan dopo un hop close completato

Blocco attività esatto in anteprima e seed, seguito da un messaggio di passaggio separato dopo la chiusura; il fork non eredita l'attività.

**Ambito:** deferred. **Dipendenze:** C29, C34. **Stima:** 8–16 ore.

**File/componenti:** `crates/hop/src/daodan.rs`; `apps/desktop/src/hop/task-preview.ts`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su richiesta esplicita dell utente del 2026-11-02: ignorare per adesso l integrazione con Daodan. Riprendere solo su nuova indicazione.

- Una sola conferma; nessun messaggio di subentro prima di COMMITTED e della chiusura confermata dell'origine.
- Chiusura rifiutata, incerta o interrotta da crash e destinazione senza pacchetto non ricevono istruzioni di subentro.
- Nessun percorso Shinatama modifica lo stato Daodan o assegna una sola attività a entrambi i thread divergenti.

**Verifiche del passo:** `close/fork/uncertain-stop task-hop matrix`; `preview-vs-delivery equality`.

**Gate collegati, da registrare separatamente:** D13, D20.

**Requisiti coperti dal piano:** FRD-0005/R30, FRD-0005/R60, FRD-0005/R61, FRD-0005/R62, FRD-0008/R28, FRD-0008/R29, FRD-0008/R30, FRD-0008/R31, FRD-0008/R32, FRD-0008/R33.

**Commit proposto:** `feat(c35): Trasferire il contesto delle attività Daodan dopo un hop close completato`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/hop/src/daodan.rs, apps/desktop/src/hop/task-preview.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c36"></a>
## C36 · Aggiungere preferiti, metadati git e avvio in worktree collegati

Progetti preferiti, fatti git limitati e memorizzati temporaneamente, ultimo agente/thread e opzione esplicita di avvio in un worktree dedicato.

**Ambito:** v1. **Dipendenze:** C25. **Stima:** 12–24 ore.

**File/componenti:** `crates/projects/src/git.rs`; `crates/engine/src/worktree_launch.rs`; `apps/desktop/src/project-picker.ts`.

**Accettazione osservabile:**

- La scansione git di trenta progetti resta limitata, parallela e con timeout; un errore git produce Unknown.
- Il worktree dedicato e una scelta esplicita della nuova istanza; nessun legame con task esterni e necessario.
- Nessuna UI Git Diff dedicata e nessuna creazione o eliminazione implicita di worktree.

**Verifiche del passo:** `synthetic git/common-dir/worktree corpus`; `parallel-scan timeout measurements`.

**Gate collegati, da registrare separatamente:** PM2, PM3, H20, I3.

**Requisiti coperti dal piano:** FRD-0004/R07, FRD-0004/R08, FRD-0004/R16, FRD-0004/R21.

**Commit proposto:** `feat(c36): Aggiungere preferiti, metadati git e avvio in worktree collegati`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/projects/src/git.rs, crates/engine/src/worktree_launch.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C36.1 · Aggiungere preferiti e fatti git con cache

Preferiti persistenti e scansione git parallela con timeout per radici/progetti.

Stima: 4–8 ore. File: `crates/projects/src/git.rs`, `apps/desktop/src/project-picker.ts`.

Accettazione: Trenta progetti non bloccano la UI; errori git restano Unknown e nessuna vista diff viene introdotta. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus git e misura parallel-scan`; `npm run build`.

Commit: `feat(c36.1): Aggiungere preferiti e fatti git con cache`. Registrare stato e prove sotto `progress.json` → `steps` → `C36` → `units` → `C36.1`.

### C36.2 · Mostrare ultimo agente/thread e avvio dell’attività

Righe progetto collegate alla cronologia e al worktree dell’attività.

Stima: 4–8 ore. File: `apps/desktop/src/project-picker.ts`, `crates/projects/src/threads.rs`.

Accettazione: Un’attività collegata si apre nel proprio worktree; link obsoleti non vengono riparati per supposizione. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fixture last-thread e task-worktree`; `npm run build`.

Commit: `feat(c36.2): Mostrare ultimo agente/thread e avvio dell’attività`. Registrare stato e prove sotto `progress.json` → `steps` → `C36` → `units` → `C36.2`.

### C36.3 · Creare un worktree soltanto come opzione di nuova istanza

Piano esplicito di worktree collegato con controlli e gestione del fallimento dell’avvio.

Stima: 4–8 ore. File: `crates/engine/src/worktree_launch.rs`, `apps/desktop/src/launch-review.ts`.

Accettazione: L overview non crea worktree; l opzione di nuova istanza crea solo il worktree confermato dall utente. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test linked-worktree launch/failure`; `npm run build`.

Commit: `feat(c36.3): Creare un worktree soltanto come opzione di nuova istanza`. Registrare stato e prove sotto `progress.json` → `steps` → `C36` → `units` → `C36.3`.

<a id="c39"></a>
## C39 · Vincolare importazione e trasferimento nativo post-v1 alle verifiche

Percorso nativo attivato separatamente per agente e versione, con revisione Mukade verificata, anteprima esatta preparata, consenso, identità riservata e riconciliazione della ricevuta.

**Ambito:** post-v1. **Dipendenze:** C30, C26. **Stima:** 24–48 ore.

**File/componenti:** `crates/hop/src/native.rs`; `crates/import/**`; `verify/H27/**`.

**Accettazione osservabile:**

- ReasoningPolicy Drop; l'oscuramento copre dati grezzi, strumenti e allegati; isolamento dei figli e scritture sulla destinazione sono serializzati.
- Una scrittura incerta non viene ritentata automaticamente; la ricevuta sopravvive a un avvio fallito; l'origine non viene modificata o eliminata.
- Il trasferimento tra due home dello stesso harness resta rifiutato finché SS5 non passa; perdite dell'importazione o compattazione nativa sono esplicite.

**Verifiche del passo:** `post-v1 writer/privacy/store/recovery fixture suite`; `source-fingerprint race and double-click tests`; `native import roundtrip law per supported destination`.

**Gate collegati, da registrare separatamente:** H27, SS4, SS5, OC2, CX4.

**Requisiti coperti dal piano:** FRD-0005/R16, FRD-0005/R17, FRD-0005/R52, FRD-0005/R53, FRD-0005/R54, FRD-0005/R55, FRD-0005/R56, FRD-0005/R57, FRD-0005/R58, FRD-0005/R59, FRD-0006/R28, FRD-0006/R30, FRD-0006/R34, FRD-0006/R36.

**Commit proposto:** `feat(c39): Vincolare importazione e trasferimento nativo post-v1 alle verifiche`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/hop/src/native.rs, crates/import/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C39.1 · Preparare anteprima nativa e destinazione riservata

Percorso post-v1 separato con payload preparato, fingerprint e consenso esplicito.

Stima: 6–12 ore. File: `crates/hop/src/native_preview.rs`.

Accettazione: ReasoningPolicy Drop e oscuramento completo; nessun writer è disponibile in v1. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test staged-preview/privacy/reserved-id`; `npm run build`.

Commit: `feat(c39.1): Preparare anteprima nativa e destinazione riservata`. Registrare stato e prove sotto `progress.json` → `steps` → `C39` → `units` → `C39.1`.

### C39.2 · Eseguire una scrittura nativa verificata per destinazione

Wrapper Mukade con radici esplicite, journal-before-effect e scritture serializzate.

Stima: 6–12 ore. File: `crates/hop/src/native.rs`, `crates/import/**`.

Accettazione: Una versione o coppia non certificata rifiuta; nessuna sorgente viene modificata o eliminata. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: conformance writer per destinazione/versione`; `npm run build`.

Commit: `feat(c39.2): Eseguire una scrittura nativa verificata per destinazione`. Registrare stato e prove sotto `progress.json` → `steps` → `C39` → `units` → `C39.2`.

### C39.3 · Persistire ricevute e riconciliare esiti incerti

Recupero di WriteUncertain/ResumeFailed senza retry automatico né eliminazione dei file nativi.

Stima: 6–12 ore. File: `crates/hop/src/native_recovery.rs`.

Accettazione: La ricevuta sopravvive a un avvio fallito; doppio clic o retry non creano sessioni duplicate. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fault matrix receipt/reconcile/double-click`; `npm run build`.

Commit: `feat(c39.3): Persistire ricevute e riconciliare esiti incerti`. Registrare stato e prove sotto `progress.json` → `steps` → `C39` → `units` → `C39.3`.

### C39.4 · Provare due home e roundtrip del formato

Abilitazione puntuale di trasferimenti tra home e dichiarazione delle perdite native.

Stima: 6–12 ore. File: `verify/H27/**`, `crates/import/tests/**`.

Accettazione: SS5 e legge di roundtrip devono passare prima dell’offerta; compattazione nativa richiede il percorso documentato. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: H27/SS4/SS5 e import-export roundtrip`; `npm run build`.

Commit: `feat(c39.4): Provare due home e roundtrip del formato`. Registrare stato e prove sotto `progress.json` → `steps` → `C39` → `units` → `C39.4`.

<a id="c21"></a>
## C21 · Aggiungere Gemini per il solo avvio

Gemini si avvia soltanto attraverso il sidecar, con ricerca, hop e lettura dichiarati non supportati.

**Ambito:** v1. **Dipendenze:** C13, C14, C15. **Stima:** 6–12 ore.

**File/componenti:** `crates/adapter/src/gemini/**`.

**Accettazione osservabile:**

- Gemini non compare mai tra origini o destinazioni degli hop; traffico non coperto è visibile e non viene dichiarato supportato.

**Verifiche del passo:** `launch-only contract negative cases`; `provider coverage fixture`.

**Gate collegati, da registrare separatamente:** SC11.

**Requisiti coperti dal piano:** FRD-0001/R03, FRD-0001/R04, FRD-0006/R40, FRD-0006/R42.

**Commit proposto:** `feat(c21): Aggiungere Gemini per il solo avvio`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/gemini/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c37"></a>
## C37 · Aggiungere installazione, aggiornamento e ripristino dei programmi gestiti

Cartelle di versione per agente, procedure d'installazione verificate, passaggio di versione subordinato alle fixture e ripristino senza toccare eseguibili di sistema.

**Ambito:** v1. **Dipendenze:** C14, C15, C16, C17, C18, C19, C20. **Stima:** 16–32 ore.

**File/componenti:** `crates/agents/src/install.rs`; `crates/agents/src/versions.rs`.

**Accettazione osservabile:**

- Le istanze attive conservano il vecchio eseguibile; la combinazione own e home di sistema è sempre esclusa.
- Ogni percorso di aggiornamento deve essere controllabile prima di offrire own; procedure sconosciute di Codex o Hermes causano un rifiuto.
- Archivi migrati e incompatibili vengono segnalati e mai riparati automaticamente; l'osservazione degli aggiornamenti per una settimana è registrata.

**Verifiche del passo:** `installer/uninstall/failure/locked-executable fixtures`; `version-switch rollback suite`; `one-week updater observation`.

**Gate collegati, da registrare separatamente:** D16, D18, HM10, DS10.

**Requisiti coperti dal piano:** FRD-0002/R04, FRD-0002/R05, FRD-0002/R07, FRD-0002/R08, FRD-0002/R09, FRD-0002/R10, FRD-0002/R11, FRD-0002/R39.

**Commit proposto:** `feat(c37): Aggiungere installazione, aggiornamento e ripristino dei programmi gestiti`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/agents/src/install.rs, crates/agents/src/versions.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C37.1 · Definire installazioni gestite per versione

Manifesti delle ricette e cartelle versionate senza alterare programmi di sistema.

Stima: 4–8 ore. File: `crates/agents/src/install.rs`.

Accettazione: Ricette o updater non controllabili rifiutano own; own con home di sistema resta vietato. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fixture ricette e placement rifiutati`; `npm run build`.

Commit: `feat(c37.1): Definire installazioni gestite per versione`. Registrare stato e prove sotto `progress.json` → `steps` → `C37` → `units` → `C37.1`.

### C37.2 · Installare una versione accanto a quella attiva

Installazione di test verificata e selezione per nuovi avvii senza cambiare istanze già aperte.

Stima: 6–12 ore. File: `crates/agents/src/versions.rs`, `crates/agents/src/install.rs`.

Accettazione: Errore, file bloccato o hash errato non sostituiscono l’eseguibile attivo. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fault test install e locked-executable`; `npm run build`.

Commit: `feat(c37.2): Installare una versione accanto a quella attiva`. Registrare stato e prove sotto `progress.json` → `steps` → `C37` → `units` → `C37.2`.

### C37.3 · Ripristinare la versione precedente dopo fixture fallite

Switch atomico, conservazione versione precedente e osservazione degli updater.

Stima: 6–12 ore. File: `crates/agents/src/rollback.rs`, `docs/verification/**`.

Accettazione: Archivi incompatibili sono segnalati e non riparati; il ripristino non scrive nella home di sistema. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: suite rollback e osservazione updater`; `npm run build`.

Commit: `feat(c37.3): Ripristinare la versione precedente dopo fixture fallite`. Registrare stato e prove sotto `progress.json` → `steps` → `C37` → `units` → `C37.3`.

<a id="c41"></a>
## C41 · Aggiungere navigazione limitata e in sola lettura nei file del progetto

Albero e ricerca dei file nelle radici di progetto esplicite, con attraversamento asincrono limitato e apertura in una scheda.

**Ambito:** v1. **Dipendenze:** C07, C04. **Stima:** 10–18 ore.

**File/componenti:** `crates/files/src/navigator.rs`; `apps/desktop/src/files/**`.

**Accettazione osservabile:**

- Aprire una voce non esegue mai un file.
- Attraversamento e risoluzione dei collegamenti restano nelle radici autorizzate, destinazioni dei symlink comprese; alberi grandi o lenti si annullano senza bloccare i terminali.

**Verifiche del passo:** `path traversal/symlink/deep-tree corpus`; `async cancellation/large-tree desktop checks`.

**Gate collegati, da registrare separatamente:** TL4.

**Requisiti coperti dal piano:** FRD-0001/R35.

**Commit proposto:** `feat(c41): Aggiungere navigazione limitata e in sola lettura nei file del progetto`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/files/src/navigator.rs, apps/desktop/src/files/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C41.1 · Esplorare file soltanto nelle radici autorizzate

Traversata asincrona limitata con identità canonica e cancellazione.

Stima: 5–9 ore. File: `crates/files/src/navigator.rs`.

Accettazione: Symlink esterni, profondità eccessiva e percorsi ambigui non ampliano le concessioni. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus traversal/symlink/depth`; `npm run build`.

Commit: `feat(c41.1): Esplorare file soltanto nelle radici autorizzate`. Registrare stato e prove sotto `progress.json` → `steps` → `C41` → `units` → `C41.1`.

### C41.2 · Collegare albero e ricerca alle schede

UI del navigatore con azione esplicita di apertura e stato di errore.

Stima: 5–9 ore. File: `apps/desktop/src/files/**`.

Accettazione: Un clic non esegue mai il file e scansioni lente non congelano i terminali. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: smoke navigatore/open/cancel`; `npm run build`.

Commit: `feat(c41.2): Collegare albero e ricerca alle schede`. Registrare stato e prove sotto `progress.json` → `steps` → `C41` → `units` → `C41.2`.

<a id="c42"></a>
## C42 · Aggiungere lettura sicura di Markdown e modifica del sorgente

Buffer condivisi per file canonico, modalità sorgente, anteprima e affiancata, salvataggio esplicito, annulla/ripeti e gestione di modifiche non salvate e conflitti.

**Ambito:** v1. **Dipendenze:** C41. **Stima:** 16–32 ore.

**File/componenti:** `crates/documents/src/buffers.rs`; `crates/documents/src/markdown.rs`; `apps/desktop/src/markdown/**`.

**Accettazione osservabile:**

- Markdown non esegue script, comandi incorporati o richieste remote senza concessione esplicita; parsing e disegno sono limitati e i risultati obsoleti scartati.
- Le scritture concorrenti degli agenti non sostituiscono silenziosamente un buffer modificato; il salvataggio riverifica la versione e sostituisce atomicamente dove supportato.
- La chiusura di un buffer modificato offre salva, scarta o annulla; il layout non contiene buffer modificati o testo di recupero.
- Riallineare il vecchio vincolo Mermaid native/no-JS alla shell Tauri autorizzata: renderer locale isolato, input non eseguibile, nessun accesso ambientale o risorsa remota; nessuna falsa dichiarazione di renderer nativo.

**Verifiche del passo:** `hostile Markdown and links/assets corpus`; `concurrent external write/save failure/late render tests`.

**Gate collegati, da registrare separatamente:** TL4.

**Requisiti coperti dal piano:** FRD-0001/R36, FRD-0001/R38, FRD-0001/R41.

**Commit proposto:** `feat(c42): Aggiungere lettura sicura di Markdown e modifica del sorgente`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/documents/src/buffers.rs, crates/documents/src/markdown.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C42.1 · Gestire buffer condivisi e revisioni dei file

Un buffer per file canonico con dirty state, undo e redo.

Stima: 4–8 ore. File: `crates/documents/src/buffers.rs`.

Accettazione: Due viste condividono modifiche senza buffer indipendenti; il layout non contiene il testo modificato. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test buffer condivisi e undo/redo`; `npm run build`.

Commit: `feat(c42.1): Gestire buffer condivisi e revisioni dei file`. Registrare stato e prove sotto `progress.json` → `steps` → `C42` → `units` → `C42.1`.

### C42.2 · Mostrare Markdown sanitizzato e anteprima affiancata

Rendering limitato con sorgente/preview/split e risoluzione sicura degli asset.

Stima: 6–12 ore. File: `crates/documents/src/markdown.rs`, `apps/desktop/src/markdown/**`.

Accettazione: Script, comandi, link fuori root e richieste remote non autorizzate sono bloccati; risultati obsoleti scartati. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus Markdown ostile e stale-render`; `npm run build`.

Commit: `feat(c42.2): Mostrare Markdown sanitizzato e anteprima affiancata`. Registrare stato e prove sotto `progress.json` → `steps` → `C42` → `units` → `C42.2`.

### C42.3 · Salvare senza sovrascritture silenziose

Salvataggio con verifica della versione, sostituzione sicura e dialogo dirty-close.

Stima: 6–12 ore. File: `crates/documents/src/save.rs`, `apps/desktop/src/markdown/save.ts`.

Accettazione: Scritture degli agenti e guasti di salvataggio preservano il buffer; salva/scarta/annulla hanno esiti distinti. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test external-write/save-failure/dirty-close`; `npm run build`.

Commit: `feat(c42.3): Salvare senza sovrascritture silenziose`. Registrare stato e prove sotto `progress.json` → `steps` → `C42` → `units` → `C42.3`.

<a id="c43"></a>
## C43 · Abilitare la modifica visuale Markdown solo dopo le verifiche di fedeltà

Editor visuale disponibile soltanto per la sintassi che supera le fixture di andata e ritorno, preservando il sorgente non supportato.

**Ambito:** v1. **Dipendenze:** C42. **Stima:** 16–32 ore.

**File/componenti:** `crates/documents/src/visual.rs`; `apps/desktop/src/markdown/visual.ts`.

**Accettazione osservabile:**

- Le modifiche conservano la sintassi non supportata senza eliminarla silenziosamente.
- I casi che non superano sintassi o fedeltà restano disponibili soltanto nel sorgente e sono segnalati chiaramente.

**Verifiche del passo:** `Markdown source-visual-source corpus`; `undo/redo and concurrent write fidelity tests`.

**Gate collegati, da registrare separatamente:** TL4.

**Requisiti coperti dal piano:** FRD-0001/R37.

**Commit proposto:** `feat(c43): Abilitare la modifica visuale Markdown solo dopo le verifiche di fedeltà`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/documents/src/visual.rs, apps/desktop/src/markdown/visual.ts. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C43.1 · Definire il sottoinsieme visuale e il corpus di fedeltà

Matrice della sintassi supportata e trasformazioni sorgente-documento-sorgente.

Stima: 4–8 ore. File: `crates/documents/src/visual.rs`, `crates/documents/tests/visual.rs`.

Accettazione: Ogni caso che perde sintassi resta source-only e non viene abilitato nell’editor visuale. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus roundtrip Markdown`; `npm run build`.

Commit: `feat(c43.1): Definire il sottoinsieme visuale e il corpus di fedeltà`. Registrare stato e prove sotto `progress.json` → `steps` → `C43` → `units` → `C43.1`.

### C43.2 · Collegare modifica visuale e conservazione degli elementi ignoti

Editor visuale dei soli nodi provati, con blocchi non supportati preservati.

Stima: 6–12 ore. File: `apps/desktop/src/markdown/visual.ts`.

Accettazione: Modifiche e undo/redo non eliminano contenuti sconosciuti e condividono lo stesso buffer del sorgente. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: fixture editing e nodi ignoti`; `npm run build`.

Commit: `feat(c43.2): Collegare modifica visuale e conservazione degli elementi ignoti`. Registrare stato e prove sotto `progress.json` → `steps` → `C43` → `units` → `C43.2`.

### C43.3 · Provare conflitti e abilitare la modalità per capacità

Confronto delle versioni e abilitazione visuale solo dopo le fixture passate.

Stima: 6–12 ore. File: `crates/documents/src/visual.rs`, `apps/desktop/src/markdown/visual.ts`.

Accettazione: Un aggiornamento esterno non distrugge modifiche visuali; i casi falliti restano chiaramente indisponibili. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: test visuale external-write e capability-refusal`; `npm run build`.

Commit: `feat(c43.3): Provare conflitti e abilitare la modalità per capacità`. Registrare stato e prove sotto `progress.json` → `steps` → `C43` → `units` → `C43.3`.

<a id="c44"></a>
## C44 · Aggiungere anteprime degli artefatti e lettura DOCX verificata separatamente

Anteprime limitate di testo e immagini; lettore DOCX scelto con fedeltà dichiarata, senza presumere supporto alla modifica Office.

**Ambito:** v1. **Dipendenze:** C41. **Stima:** 12–24 ore.

**File/componenti:** `crates/documents/src/preview.rs`; `apps/desktop/src/previews/**`.

**Accettazione osservabile:**

- Input troppo grandi o non supportati vengono rifiutati senza allocazioni illimitate.
- La disponibilità DOCX dipende dalle fixture del renderer; collegamenti e risorse remote non aggirano le radici autorizzate.

**Verifiche del passo:** `artifact size/decompression/fidelity corpus`; `private asset lifecycle checks`.

**Gate collegati, da registrare separatamente:** TL6.

**Requisiti coperti dal piano:** FRD-0001/R39.

**Commit proposto:** `feat(c44): Aggiungere anteprime degli artefatti e lettura DOCX verificata separatamente`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/documents/src/preview.rs, apps/desktop/src/previews/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C44.1 · Mostrare artefatti di testo e immagini con limiti

Preview in sola lettura, dimensioni massime e gestione degli input non supportati.

Stima: 4–8 ore. File: `crates/documents/src/preview.rs`, `apps/desktop/src/previews/**`.

Accettazione: Input grandi o immagini ostili non causano allocazioni senza limite né accesso a risorse esterne. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus artefatti e limiti memoria`; `npm run build`.

Commit: `feat(c44.1): Mostrare artefatti di testo e immagini con limiti`. Registrare stato e prove sotto `progress.json` → `steps` → `C44` → `units` → `C44.1`.

### C44.2 · Scegliere e provare il lettore DOCX

Renderer isolato e corpus che dichiara formato e fedeltà effettivi.

Stima: 4–8 ore. File: `crates/documents/src/docx.rs`, `docs/verification/**`.

Accettazione: Documenti non fedeli o non sicuri restano non disponibili; nessuna promessa di modifica Office. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus DOCX fidelity/decompression/assets`; `npm run build`.

Commit: `feat(c44.2): Scegliere e provare il lettore DOCX`. Registrare stato e prove sotto `progress.json` → `steps` → `C44` → `units` → `C44.2`.

### C44.3 · Integrare DOCX come capacità esplicita

Pannello DOCX disponibile soltanto sulle piattaforme e sui casi provati.

Stima: 4–8 ore. File: `apps/desktop/src/previews/docx.ts`.

Accettazione: Errore o renderer assente preservano app e altre anteprime senza fallback non autorizzati. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: smoke pannello e renderer mancante`; `npm run build`.

Commit: `feat(c44.3): Integrare DOCX come capacità esplicita`. Registrare stato e prove sotto `progress.json` → `steps` → `C44` → `units` → `C44.3`.

<a id="c45"></a>
## C45 · Vincolare browser incorporati e mini-app post-v1 a verifiche separate

Proposta separata e implementazione dell'isolamento soltanto dopo i controlli browser e mini-app; segnaposto non disponibili fino ad allora.

**Ambito:** post-v1. **Dipendenze:** C07, C44. **Stima:** 24–48 ore.

**File/componenti:** `crates/web-content/**`; `apps/desktop/src/integrated-apps/**`.

**Accettazione osservabile:**

- Il contenuto incorporato non ha accesso implicito a filesystem, PTY, credenziali o comandi Rust.
- Non si presumono compatibilità JanuraCode, ripiego automatico del motore o trasferimento di sessioni browser.
- Il percorso d'integrazione browser possiede i requisiti di sicurezza del motore e delle sessioni; questo passo aggiunge soltanto l'ospitalità isolata nello spazio di lavoro.

**Verifiche del passo:** `capability escape corpus`; `lifecycle/OS compatibility matrix`.

**Gate collegati, da registrare separatamente:** TL6, B-series.

**Requisiti coperti dal piano:** FRD-0001/R40.

**Commit proposto:** `feat(c45): Vincolare browser incorporati e mini-app post-v1 a verifiche separate`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/web-content/**, apps/desktop/src/integrated-apps/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### C45.1 · Definire le concessioni dei contenuti incorporati

Proposta post-v1 e contratto di isolamento con nessun privilegio ambientale.

Stima: 6–12 ore. File: `crates/web-content/src/grants.rs`, `docs/verification/**`.

Accettazione: L’implementazione resta disabilitata finché i gate browser e mini-app non ne consentono l’avvio. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: revisione grants e threat model`; `npm run build`.

Commit: `feat(c45.1): Definire le concessioni dei contenuti incorporati`. Registrare stato e prove sotto `progress.json` → `steps` → `C45` → `units` → `C45.1`.

### C45.2 · Creare il contenitore isolato senza comandi applicativi

WebView o contenitore separato con lifecycle della finestra e concessioni minime.

Stima: 6–12 ore. File: `crates/web-content/src/host.rs`.

Accettazione: Tentativi di filesystem, PTY, credenziali e IPC Rust non concessi vengono rifiutati. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: corpus escape e lifecycle`; `npm run build`.

Commit: `feat(c45.2): Creare il contenitore isolato senza comandi applicativi`. Registrare stato e prove sotto `progress.json` → `steps` → `C45` → `units` → `C45.2`.

### C45.3 · Integrare pannelli e stati non disponibili

Schede di contenuto incorporato con stato esplicito per piattaforme o motori mancanti.

Stima: 6–12 ore. File: `apps/desktop/src/integrated-apps/**`.

Accettazione: Ripristinare un layout non riapre browser interattivi; nessun trasferimento o fallback automatico del motore. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: smoke layout e rifiuto restore`; `npm run build`.

Commit: `feat(c45.3): Integrare pannelli e stati non disponibili`. Registrare stato e prove sotto `progress.json` → `steps` → `C45` → `units` → `C45.3`.

### C45.4 · Certificare la combinazione piattaforma e contenuto

Matrice di compatibilità e prove negative necessarie prima dell’abilitazione.

Stima: 6–12 ore. File: `crates/web-content/tests/**`, `docs/verification/**`.

Accettazione: La compatibilità JanuraCode non viene presunta e i casi non provati restano indisponibili. La compilazione e l'avvio dell'app esistente restano funzionanti; i gate del passo padre restano aperti fino alle rispettive prove.

Verifiche pianificate: `planned: matrice TL6/B-series per piattaforma`; `npm run build`.

Commit: `feat(c45.4): Certificare la combinazione piattaforma e contenuto`. Registrare stato e prove sotto `progress.json` → `steps` → `C45` → `units` → `C45.4`.

<a id="f04"></a>
## F04 · Hook di stato nativi e prima panoramica delle istanze

Panoramica delle istanze prevalentemente di consultazione, con origine dello stato, dati di utilizzo non disponibili dichiarati e identificazione accessibile degli harness.

**Ambito:** v1. **Dipendenze:** F02, C15. **Stima:** 4–10 ore.

**File/componenti:** `apps/desktop/src/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

- Gli eventi nativi devono prevalere sul transcript e sulle euristiche; le euristiche devono sempre indicare incertezza.
- Nessuna euristica del testo sullo schermo deve poter autorizzare un’approvazione.
- Il contenuto delle richieste in attesa deve essere oscurato quando fornito dall’harness.
- Offrire soltanto azioni di focalizzazione, hop e pausa; non inventare intenzioni di collaborazione.

**Verifiche del passo:** `planned: Native event precedes transcript and heuristic; heuristic always unsure.`; `planned: No screen-text heuristic can authorize approval.`; `planned: Pending payload redacted where harness supplies it.`; `planned: Only focus/hop/pause actions; no invented collaboration intent.`.

**Gate collegati, da registrare separatamente:** I1, I10, CL8, CX7, OC5, PI4, PI6, HM4, DS4.

**Requisiti coperti dal piano:** FRD-0010/R01, FRD-0010/R02, FRD-0010/R03, FRD-0010/R04, FRD-0010/R09, FRD-0010/R10, FRD-0010/R11, FRD-0010/R12, FRD-0010/R13, FRD-0010/R14, FRD-0010/R15, FRD-0010/R16, FRD-0010/R36, FRD-0010/R40, FRD-0010/R41, FRD-0010/R43.

**Commit proposto:** `feat(f04): Hook di stato nativi e prima panoramica delle istanze`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/src/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f05"></a>
## F05 · Utilizzo, dati git, collisioni e cronologia della collaborazione

Utilizzo, contesto e costi effettivi, dati git su branch e worktree e cronologia degli eventi; attribuzione delle collisioni basata sulle prove.

**Ambito:** v1. **Dipendenze:** F04, C23, C29. **Stima:** 4–10 ore.

**File/componenti:** `apps/desktop/src/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

- I casi detached, linked, bare, submodule e non-git devono mostrare dati corretti.
- Due autori attribuiti devono essere nominati entrambi; una modifica non attribuita non deve nominare alcuna istanza.
- Escludere i file generati; worktree distinti non devono collidere.
- Confrontare l’utilizzo con i dati dei provider e misurare il costo del controllo di stato nei repository grandi.

**Verifiche del passo:** `planned: Detached, linked, bare, submodule and non-git fixtures display correct facts.`; `planned: Two attributed writers name both; unattributed change names no instance.`; `planned: Generated files excluded; distinct worktrees never collide.`; `planned: Usage compared to provider reports; large-repository status cost measured.`.

**Gate collegati, da registrare separatamente:** I2, I3, I4, Q3, OC11, I10.

**Requisiti coperti dal piano:** FRD-0010/R09, FRD-0010/R10, FRD-0010/R11, FRD-0010/R12, FRD-0010/R13, FRD-0010/R14, FRD-0010/R15, FRD-0010/R16, FRD-0010/R36, FRD-0010/R40, FRD-0010/R41, FRD-0010/R43.

**Commit proposto:** `feat(f05): Utilizzo, dati git, collisioni e cronologia della collaborazione`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/src/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f06"></a>
## F06 · Titoli testuali originali con impostazioni, esclusioni e limiti di costo

Titoli di attività senza modello e interfaccia per pausa, impostazioni e limiti di costo, utilizzabili senza scaricare modelli.

**Ambito:** v1. **Dipendenze:** F03P, F04. **Stima:** 2–6 ore.

**File/componenti:** `apps/desktop/src/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

- I titoli devono ripiegare sul testo originale dell’ultimo strumento o messaggio.
- I thread esclusi non devono generare richieste di titolazione.
- Limitare gli aggiornamenti su cambio di stato e timer; conservare il log dei riepiloghi secondo la politica prevista.

**Verifiche del passo:** `planned: Titles fall back to last tool/message verbatim.`; `planned: Excluded threads generate no title requests.`; `planned: State-change/timer refresh bounded; summary log retained under policy.`.

**Gate collegati, da registrare separatamente:** I7, I8, I9.

**Requisiti coperti dal piano:** FRD-0010/R17, FRD-0010/R18, FRD-0010/R19, FRD-0010/R20, FRD-0010/R21, FRD-0010/R22, FRD-0010/R38, FRD-0010/R39, FRD-0010/R42.

**Commit proposto:** `feat(f06): Titoli testuali originali con impostazioni, esclusioni e limiti di costo`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/src/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f07"></a>
## F07 · Runtime condiviso per modelli locali e download facoltativi

Runtime locale supervisionato e condiviso da titoli e julia1, con consenso al download del modello, licenza e verifica del checksum.

**Ambito:** v1. **Dipendenze:** F06, C13. **Stima:** 6–16 ore.

**File/componenti:** `crates/local-model/`; `apps/desktop/src/settings/`; `docs/verification/`.

**Accettazione osservabile:**

- Rifiutare o non scaricare il modello deve mantenere il backend nessuno e non produrre chiamate in uscita.
- Un modello usato per due ruoli deve richiedere un solo runtime e un solo download.
- Misurare CPU, memoria e latenza con quattro agenti e valutare la qualità su circa 50 istantanee.

**Verifiche del passo:** `planned: Decline/no download leaves backend none and no outbound call.`; `planned: One model for two roles is one runtime/download.`; `planned: CPU/memory/latency measured with four agents; quality rubric on about 50 snapshots.`.

**Gate collegati, da registrare separatamente:** I5, I6, T12, X3.

**Requisiti coperti dal piano:** FRD-0010/R19, FRD-0010/R20, FRD-0010/R21, FRD-0010/R22, FRD-0010/R23, FRD-0010/R38, FRD-0013/R17, FRD-0013/R18, FRD-0013/R19, FRD-0013/R20, FRD-0013/R21, FRD-0017/R13, FRD-0017/R14, FRD-0017/R15, FRD-0017/R16.

**Commit proposto:** `feat(f07): Runtime condiviso per modelli locali e download facoltativi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/local-model/, apps/desktop/src/settings/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f08"></a>
## F08 · Titoli facoltativi dei provider dai cataloghi degli account autenticati

Backend dei titoli del provider con catalogo dinamico, consenso, oscuramento dei dati e ripiego dopo il limite giornaliero.

**Ambito:** v1. **Dipendenze:** F06, C13, C14. **Stima:** 3–8 ore.

**File/componenti:** `apps/desktop/src/`; `apps/desktop/src-tauri/src/`; `docs/verification/`.

**Accettazione osservabile:**

- Non fissare nomi di modelli nel codice; indisponibilità o errori del provider devono ripiegare sul testo originale.
- Limite di costo ed esclusioni devono impedire le richieste prima dell’invio.
- Registrare qualità e prezzo e confrontarli con il riferimento iniziale.

**Verifiche del passo:** `planned: No hardcoded model name; unavailable/provider failure falls back verbatim.`; `planned: Cap and exclusion prevent requests before send.`; `planned: Record quality/price and compare against baseline.`.

**Gate collegati, da registrare separatamente:** I5, I7, I9, SC10.

**Requisiti coperti dal piano:** FRD-0010/R17, FRD-0010/R18, FRD-0010/R19, FRD-0010/R20, FRD-0010/R21, FRD-0010/R22, FRD-0010/R23, FRD-0010/R38, FRD-0010/R39, FRD-0010/R42.

**Commit proposto:** `feat(f08): Titoli facoltativi dei provider dai cataloghi degli account autenticati`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/desktop/src/, apps/desktop/src-tauri/src/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f09"></a>
## F09 · Protocollo delle schede companion, azioni predefinite e rendering sicuro

Parser e visualizzazione delle schede con limiti, modelli di testo con operandi protetti e impostazioni iniziali in italiano e inglese.

**Ambito:** v1. **Dipendenze:** C42, C11. **Stima:** 4–10 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- Un blocco scheda non valido o troppo grande deve conservare il testo visibile e scartare soltanto gli elementi non validi.
- Applicare i limiti di normalizzazione NFKC, identità, quiz e fonti.
- Un operando non attendibile non deve poter chiudere il contenitore né eseguire script o richieste.
- La scelta del renderer deve risolvere esplicitamente la vecchia formulazione nativo/senza JavaScript rispetto a Tauri.

**Verifiche del passo:** `planned: Malformed/oversized card block retains visible prose and drops only invalid items.`; `planned: NFKC/identity/quiz/source limits enforced.`; `planned: Untrusted operand cannot close wrapper or execute scripts/requests.`; `planned: Renderer selection explicitly resolves legacy native/no-JS wording for Tauri.`.

**Gate collegati, da registrare separatamente:** CS-4, CS-11, CS-13, CS-16, U3.

**Requisiti coperti dal piano:** FRD-0012/R12, FRD-0012/R13, FRD-0012/R18, FRD-0012/R19, FRD-0012/R24, FRD-0012/R25, FRD-0012/R26, FRD-0012/R27, FRD-0012/R33, FRD-0012/R34, FRD-0012/R35, FRD-0012/R36, FRD-0012/R37, FRD-0012/R38, FRD-0012/R39, FRD-0012/R40, FRD-0012/R41, FRD-0012/R42, FRD-0012/R51, FRD-0012/R52, FRD-0012/R53, FRD-0012/R92, FRD-0012/R93.

**Commit proposto:** `feat(f09): Protocollo delle schede companion, azioni predefinite e rendering sicuro`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f10"></a>
## F10 · Companion R1: integrazione Claude su richiesta

Companion Claude con politica di sola lettura, spiegazione della selezione, azioni predefinite e Aided Decision da richieste native; anteprima facoltativa dichiarata fino al confinamento.

**Ambito:** v1. **Dipendenze:** F09, C15, C13, F04. **Stima:** 6–16 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- Input e contesto del worker devono restare invariati e le sue approvazioni immediatamente rispondibili.
- Non esporre canali verso PTY, posta, approvazioni o MCP del worker.
- Verificare pulizia dello scratch, un solo turno in corso, priorità della coda e limite di costo.
- Non produrre schede automatiche per turno senza le verifiche del lettore incrementale e del segnale di fine turno.

**Verifiche del passo:** `planned: Worker input/context unchanged and approval immediately answerable.`; `planned: No worker PTY/inbox/approval/MCP channel exposed.`; `planned: Scratch cleanup, one in-flight turn, priority queue and cost cap behave.`; `planned: No automatic per-turn cards without incremental-reader/turn-end gate.`.

**Gate collegati, da registrare separatamente:** CS-1, CS-6, CS-7, CS-8, CS-9, CS-10, CS-11, CS-12, CS-13, CS-16, E1.

**Requisiti coperti dal piano:** FRD-0012/R01, FRD-0012/R02, FRD-0012/R03, FRD-0012/R04, FRD-0012/R05, FRD-0012/R06, FRD-0012/R12, FRD-0012/R13, FRD-0012/R14, FRD-0012/R15, FRD-0012/R16, FRD-0012/R18, FRD-0012/R19, FRD-0012/R23, FRD-0012/R24, FRD-0012/R25, FRD-0012/R26, FRD-0012/R27, FRD-0012/R28, FRD-0012/R29, FRD-0012/R30, FRD-0012/R31, FRD-0012/R33, FRD-0012/R34, FRD-0012/R35, FRD-0012/R36, FRD-0012/R37, FRD-0012/R38, FRD-0012/R39, FRD-0012/R40, FRD-0012/R41, FRD-0012/R42, FRD-0012/R51, FRD-0012/R52, FRD-0012/R53, FRD-0012/R66, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R74, FRD-0012/R75, FRD-0012/R77, FRD-0012/R78, FRD-0012/R79, FRD-0012/R80, FRD-0012/R82, FRD-0012/R83, FRD-0012/R84, FRD-0012/R87, FRD-0012/R88, FRD-0012/R90, FRD-0012/R91, FRD-0012/R92, FRD-0012/R93, FRD-0012/R94.

**Commit proposto:** `feat(f10): Companion R1: integrazione Claude su richiesta`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f11-windows"></a>
## F11-windows · Confinamento del companion a livello di sistema operativo: Windows

Lista positiva dei permessi di lettura e scrittura e collegamenti alle credenziali per Windows.

**Ambito:** v1. **Dipendenze:** F10. **Stima:** 6–20 ore.

**File/componenti:** `crates/confinement/windows/`; `docs/verification/`.

**Accettazione osservabile:**

- Consentire scritture soltanto nello scratch, comprese le restrizioni allo stato locale dell’harness.
- Worktree, programma e collegamenti originali alle credenziali devono essere in sola lettura; Shinatama non deve aprire o copiare le credenziali.
- Altre directory personali, progetti e database devono essere inaccessibili; mostrare il badge confinato soltanto dopo la verifica.

**Verifiche del passo:** `planned: Only scratch writable, including harness-local state restrictions.`; `planned: Worktree/program and original credential bindings read-only; Shinatama never opens/copies credentials.`; `planned: Other homes/projects/DB inaccessible; confined badge withheld until pass.`.

**Gate collegati, da registrare separatamente:** CS-2, CS-9.

**Requisiti coperti dal piano:** FRD-0012/R66, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R94.

**Commit proposto:** `feat(f11-windows): Confinamento del companion a livello di sistema operativo: Windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/confinement/windows/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### F11-windows.1 · Verifica del meccanismo e inventario dei permessi su Windows

Interfaccia di confinamento disattivata per impostazione iniziale, sonda delle capacita del sistema e manifesto dei percorsi del companion; valutare token ristretto, identita di processo e regole di accesso effettivamente applicabili senza cambiare le ACL dei file originali.

Stima: 2–6 ore. File: `crates/confinement/windows/`, `docs/verification/`.

Accettazione: L'app e il companion in anteprima continuano ad avviarsi con il badge di sola politica harness; nessuna esecuzione viene dichiarata confinata. L'inventario distingue scratch scrivibile, worktree/programmi e credenziali originali in sola lettura, e percorsi sempre vietati; non legge o copia credenziali reali.

Verifiche pianificate: `pianificato: Compilare e avviare la versione corrente con il nuovo modulo disattivato.`; `pianificato: Eseguire la sonda su file sintetici e registrare prerequisiti, limiti e disponibilita del meccanismo.`.

Commit: `feat(f11-windows.1): Verifica del meccanismo e inventario dei permessi su Windows`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-windows` → `units` → `F11-windows.1`.

### F11-windows.2 · Applicazione dei permessi positivi al processo companion su Windows

Avvio confinato sperimentale con lo scratch come unica destinazione scrivibile e i soli percorsi autorizzati leggibili; collegamento alle credenziali originali soltanto in lettura, se il sistema lo consente.

Stima: 2–8 ore. File: `crates/confinement/windows/`, `docs/verification/`.

Accettazione: Il processo e i discendenti non possono scrivere nel worktree, nella sua .git o nello stato locale del progetto, ne leggere altre home, progetti o il database. Un meccanismo mancante o incapace di vincolare le credenziali non viene aggirato; la capacita confined resta disabilitata e l'anteprima rimane utilizzabile.

Verifiche pianificate: `pianificato: Provare letture e scritture consentite e vietate con file canarino, collegamenti simbolici e percorsi alternativi.`; `pianificato: Verificare che nessuna API Shinatama apra o copi credenziali e che non siano necessarie modifiche permanenti ai permessi dei file originali.`.

Commit: `feat(f11-windows.2): Applicazione dei permessi positivi al processo companion su Windows`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-windows` → `units` → `F11-windows.2`.

### F11-windows.3 · Integrazione del ciclo di vita e prove prima del badge confined su Windows

Integrazione nell'adattatore companion con pulizia del processo e dello scratch, controllo degli hook di progetto e registrazione CS-2/CS-9; badge confined soltanto per la combinazione harness/versione/piattaforma provata.

Stima: 2–6 ore. File: `crates/confinement/windows/`, `docs/verification/`.

Accettazione: Chiusura, annullamento e fallimento di avvio non lasciano scritture esterne, processi gestiti o canali al worker; la normale UI continua a funzionare. Aggiornamento delle credenziali o dipendenze non compatibili rendono l'host non idoneo al confinamento; ogni caso non provato conserva il badge di anteprima esplicito.

Verifiche pianificate: `pianificato: Eseguire i casi negativi CS-2/CS-9 e il confronto dei byte/input del worker con e senza companion.`; `pianificato: Rieseguire avvio/chiusura e prova di confinamento della versione installata, allegando record; segnare il padre completo soltanto dopo tutte le unita e le prove richieste.`.

Commit: `feat(f11-windows.3): Integrazione del ciclo di vita e prove prima del badge confined su Windows`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-windows` → `units` → `F11-windows.3`.

<a id="f12"></a>
## F12 · Companion R2: flusso automatico dei turni e continuità dopo gli hop

Delta dei turni con limiti e accorpamento, schede Learning automatiche e Aided Decision dai turni conclusi.

**Ambito:** v1. **Dipendenze:** F10, C23, C29, F04. **Stima:** 5–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il contesto task/fase Daodan di FRD-0012/R07 e rinviato; il feed dal transcript del worker resta attivo.

- Richiedere un lettore incrementale con limiti e il segnale nativo turn_end_signal.
- Non inviare mai il ragionamento; dichiarare campi troncati o sintetizzati e turni accorpati.
- Classificare soltanto il più recente messaggio dell’assistente senza risposta; in modalità su richiesta non eseguire valutazioni automatiche.
- Gli hop close/fork devono preservare il corretto legame con un solo worker.

**Verifiche del passo:** `planned: Requires bounded incremental reader and native turn_end_signal.`; `planned: Reasoning never sent; truncated/digested fields and merged turns disclosed.`; `planned: Only newest unanswered assistant message classified; on-demand mode sends no automatic judgment.`; `planned: Close/fork hop retains correct single-worker relationship.`.

**Gate collegati, da registrare separatamente:** CS-3, CS-5, CS-6, CS-8, CS-10, CS-14.

**Requisiti coperti dal piano:** FRD-0012/R07, FRD-0012/R08, FRD-0012/R09, FRD-0012/R10, FRD-0012/R11, FRD-0012/R12, FRD-0012/R13, FRD-0012/R14, FRD-0012/R15, FRD-0012/R16, FRD-0012/R17, FRD-0012/R18, FRD-0012/R19, FRD-0012/R20, FRD-0012/R21, FRD-0012/R23, FRD-0012/R32, FRD-0012/R80, FRD-0012/R81, FRD-0012/R84.

**Commit proposto:** `feat(f12): Companion R2: flusso automatico dei turni e continuità dopo gli hop`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f13"></a>
## F13 · Companion R3: memoria personale, quiz ed esportazione

Registro personale dei concetti con consultazione, riduzione del livello, eliminazione, azzeramento e disattivazione; scrittura dopo due secondi di visibilità, quiz ed esportazione dei companion.

**Ambito:** v1. **Dipendenze:** F12, FS03. **Stima:** 5–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- Una scheda nascosta mai vista non deve scrivere voci; Aided Decision non deve scriverne.
- Inviare soltanto metadati dei concetti pertinenti entro i limiti; non memorizzare il testo della spiegazione né URL.
- Non sovrascrivere database più nuovi, in sola lettura o illeggibili; il quiz deve usare un solo turno headless.
- Mantenere il backend locale alimentato solo dal flusso dietro le verifiche di contesto e qualità.

**Verifiche del passo:** `planned: Unviewed hidden card writes no entry; Aided Decision writes none.`; `planned: Only bounded relevant concept metadata sent; no explanation text/URL stored.`; `planned: Newer/read-only or unreadable DB never overwritten; quiz uses one headless turn.`; `planned: Feed-only local backend remains gated on context/quality.`.

**Gate collegati, da registrare separatamente:** CS-5, CS-15, CS-16, CS-17, CS-18.

**Requisiti coperti dal piano:** FRD-0012/R20, FRD-0012/R22, FRD-0012/R34, FRD-0012/R40, FRD-0012/R43, FRD-0012/R44, FRD-0012/R45, FRD-0012/R46, FRD-0012/R47, FRD-0012/R48, FRD-0012/R49, FRD-0012/R50, FRD-0012/R53, FRD-0012/R76, FRD-0012/R82, FRD-0012/R84, FRD-0012/R89, FRD-0012/R90.

**Commit proposto:** `feat(f13): Companion R3: memoria personale, quiz ed esportazione`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f14-codex"></a>
## F14-codex · Adattatore host del companion: Codex

Aggiungere soltanto le capacità companion di Codex provate sulla versione installata.

**Ambito:** v1. **Dipendenze:** F10, C15, C16, F11-windows. **Stima:** 4–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- La prova di sola lettura con letture abilitate deve passare indipendentemente dai test di sola risposta.
- Dichiarare resume o ripiego senza stato; verificare aggiornamento delle credenziali e hook di progetto.
- Segnalare correttamente flusso parziale o ACP e copertura del sidecar; nessun canale verso il worker.

**Verifiche del passo:** `planned: Read-only-with-reads row passes independently of answer-only tests.`; `planned: Resume or stateless fallback labelled; credentials refresh/project hooks accounted for.`; `planned: Partial/ACP feed and sidecar coverage reported honestly; no worker channel.`.

**Gate collegati, da registrare separatamente:** CS-1, CS-2, CS-3, CS-7, CS-8, CS-9, E1.

**Requisiti coperti dal piano:** FRD-0012/R01, FRD-0012/R03, FRD-0012/R15, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R74, FRD-0012/R75, FRD-0012/R79, FRD-0012/R80, FRD-0012/R81, FRD-0012/R82, FRD-0012/R83, FRD-0012/R84, FRD-0012/R87, FRD-0012/R91, FRD-0012/R94, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f14-codex): Adattatore host del companion: Codex`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f14-opencode"></a>
## F14-opencode · Adattatore host del companion: OpenCode

Aggiungere soltanto le capacità companion di OpenCode provate sulla versione installata.

**Ambito:** v1. **Dipendenze:** F10, C15, C17, F11-windows. **Stima:** 4–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- La prova di sola lettura con letture abilitate deve passare indipendentemente dai test di sola risposta.
- Dichiarare resume o ripiego senza stato; verificare aggiornamento delle credenziali e hook di progetto.
- Segnalare correttamente flusso parziale o ACP e copertura del sidecar; nessun canale verso il worker.

**Verifiche del passo:** `planned: Read-only-with-reads row passes independently of answer-only tests.`; `planned: Resume or stateless fallback labelled; credentials refresh/project hooks accounted for.`; `planned: Partial/ACP feed and sidecar coverage reported honestly; no worker channel.`.

**Gate collegati, da registrare separatamente:** CS-1, CS-2, CS-3, CS-7, CS-8, CS-9, E1.

**Requisiti coperti dal piano:** FRD-0012/R01, FRD-0012/R03, FRD-0012/R15, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R74, FRD-0012/R75, FRD-0012/R79, FRD-0012/R80, FRD-0012/R81, FRD-0012/R82, FRD-0012/R83, FRD-0012/R84, FRD-0012/R87, FRD-0012/R91, FRD-0012/R94, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f14-opencode): Adattatore host del companion: OpenCode`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f14-pi"></a>
## F14-pi · Adattatore host del companion: Pi

Aggiungere soltanto le capacità companion di Pi provate sulla versione installata.

**Ambito:** v1. **Dipendenze:** F10, C15, C18, F11-windows. **Stima:** 4–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- La prova di sola lettura con letture abilitate deve passare indipendentemente dai test di sola risposta.
- Dichiarare resume o ripiego senza stato; verificare aggiornamento delle credenziali e hook di progetto.
- Segnalare correttamente flusso parziale o ACP e copertura del sidecar; nessun canale verso il worker.

**Verifiche del passo:** `planned: Read-only-with-reads row passes independently of answer-only tests.`; `planned: Resume or stateless fallback labelled; credentials refresh/project hooks accounted for.`; `planned: Partial/ACP feed and sidecar coverage reported honestly; no worker channel.`.

**Gate collegati, da registrare separatamente:** CS-1, CS-2, CS-3, CS-7, CS-8, CS-9, E1.

**Requisiti coperti dal piano:** FRD-0012/R01, FRD-0012/R03, FRD-0012/R15, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R74, FRD-0012/R75, FRD-0012/R79, FRD-0012/R80, FRD-0012/R81, FRD-0012/R82, FRD-0012/R83, FRD-0012/R84, FRD-0012/R87, FRD-0012/R91, FRD-0012/R94, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f14-pi): Adattatore host del companion: Pi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f14-hermes"></a>
## F14-hermes · Adattatore host del companion: Hermes Agent

Aggiungere soltanto le capacità companion di Hermes Agent provate sulla versione installata.

**Ambito:** v1. **Dipendenze:** F10, C15, C19, F11-windows. **Stima:** 4–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- La prova di sola lettura con letture abilitate deve passare indipendentemente dai test di sola risposta.
- Dichiarare resume o ripiego senza stato; verificare aggiornamento delle credenziali e hook di progetto.
- Segnalare correttamente flusso parziale o ACP e copertura del sidecar; nessun canale verso il worker.

**Verifiche del passo:** `planned: Read-only-with-reads row passes independently of answer-only tests.`; `planned: Resume or stateless fallback labelled; credentials refresh/project hooks accounted for.`; `planned: Partial/ACP feed and sidecar coverage reported honestly; no worker channel.`.

**Gate collegati, da registrare separatamente:** CS-1, CS-2, CS-3, CS-7, CS-8, CS-9, E1.

**Requisiti coperti dal piano:** FRD-0012/R01, FRD-0012/R03, FRD-0012/R15, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R74, FRD-0012/R75, FRD-0012/R79, FRD-0012/R80, FRD-0012/R81, FRD-0012/R82, FRD-0012/R83, FRD-0012/R84, FRD-0012/R87, FRD-0012/R91, FRD-0012/R94, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f14-hermes): Adattatore host del companion: Hermes Agent`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f14-deepseek"></a>
## F14-deepseek · Adattatore host del companion: DeepSeek Harness

Aggiungere soltanto le capacità companion di DeepSeek Harness provate sulla versione installata.

**Ambito:** v1. **Dipendenze:** F10, C15, C20, F11-windows. **Stima:** 4–12 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- La prova di sola lettura con letture abilitate deve passare indipendentemente dai test di sola risposta.
- Dichiarare resume o ripiego senza stato; verificare aggiornamento delle credenziali e hook di progetto.
- Segnalare correttamente flusso parziale o ACP e copertura del sidecar; nessun canale verso il worker.

**Verifiche del passo:** `planned: Read-only-with-reads row passes independently of answer-only tests.`; `planned: Resume or stateless fallback labelled; credentials refresh/project hooks accounted for.`; `planned: Partial/ACP feed and sidecar coverage reported honestly; no worker channel.`.

**Gate collegati, da registrare separatamente:** CS-1, CS-2, CS-3, CS-7, CS-8, CS-9, E1.

**Requisiti coperti dal piano:** FRD-0012/R01, FRD-0012/R03, FRD-0012/R15, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R74, FRD-0012/R75, FRD-0012/R79, FRD-0012/R80, FRD-0012/R81, FRD-0012/R82, FRD-0012/R83, FRD-0012/R84, FRD-0012/R87, FRD-0012/R91, FRD-0012/R94, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f14-deepseek): Adattatore host del companion: DeepSeek Harness`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f15"></a>
## F15 · Companion R4: ricerca web senza lettura delle pagine per gli harness idonei

Turni web senza stato con osservazione degli eventi nativi, avviso, limite delle ricerche e collegamenti ai risultati verificati.

**Ambito:** v1. **Dipendenze:** F10, F13, C15, F11-windows. **Stima:** 5–14 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- Misurare C1/C2/C3/C5 per ogni harness; non offrire host non supportati.
- Le fonti devono essere su richiesta per impostazione iniziale; gli altri turni non devono avere strumenti web.
- Gli URL non osservati devono essere inerti; violazioni di riservatezza o del budget devono interrompere il turno e disabilitare il web.
- Pi resta non disponibile e la ricerca ospitata Codex non idonea, finché non viene provato un punto di integrazione qualificante.

**Verifiche del passo:** `planned: C1/C2/C3/C5 measured per harness; no unsupported host offered.`; `planned: Sources default on-demand; other turns have no web tools.`; `planned: Unseen URLs inert; query/privacy/budget breach aborts and disables web.`; `planned: Pi unavailable and Codex hosted search ineligible unless qualifying seam proved.`.

**Gate collegati, da registrare separatamente:** CS-19(a-f).

**Requisiti coperti dal piano:** FRD-0012/R26, FRD-0012/R54, FRD-0012/R55, FRD-0012/R56, FRD-0012/R57, FRD-0012/R58, FRD-0012/R59, FRD-0012/R60, FRD-0012/R61, FRD-0012/R62, FRD-0012/R64, FRD-0012/R65, FRD-0012/R84, FRD-0012/R85, FRD-0012/R86, FRD-0012/R91, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f15): Companion R4: ricerca web senza lettura delle pagine per gli harness idonei`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f16"></a>
## F16 · Companion R5: lettura delle pagine limitata ai risultati

Strumenti di lettura limitati agli URL riportati nello stesso turno web, un harness per commit.

**Ambito:** v1. **Dipendenze:** F15. **Stima:** 4–10 ore.

**File/componenti:** `crates/companion/`; `apps/desktop/src/companion/`; `docs/verification/`.

**Accettazione osservabile:**

- Verificare C4 con restrizioni agli URL esatti, redirect ostili e argomenti degli strumenti.
- Attribuire l’etichetta fonte letta soltanto a letture effettive.
- In assenza della restrizione mantenere la modalità di sola ricerca.

**Verifiche del passo:** `planned: C4 exact-URL restriction tested with hostile redirects/tool arguments.`; `planned: Only actual reads get read source label.`; `planned: Missing restriction leaves search-only mode.`.

**Gate collegati, da registrare separatamente:** CS-19(g).

**Requisiti coperti dal piano:** FRD-0012/R26, FRD-0012/R59, FRD-0012/R63, FRD-0012/R84, FRD-0012/R85, FRD-0012/R91, FRD-0012/R95, FRD-0012/R96.

**Commit proposto:** `feat(f16): Companion R5: lettura delle pagine limitata ai risultati`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/companion/, apps/desktop/src/companion/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f17"></a>
## F17 · Contratto delle richieste browser e instradamento immutabile

Richieste, stati ed esiti browser tipizzati e instradamento con ambito, senza avviare motori all’apertura dell’app.

**Ambito:** v1. **Dipendenze:** C11, C13, C32. **Stima:** 4–10 ore.

**File/componenti:** `crates/browser/`; `apps/desktop/src/browser/`; `docs/verification/`.

**Accettazione osservabile:**

- Derivare l’identità utente/agente lato server; mantenere immutabili motore, ambito e permessi.
- Nessun ripiego dopo errore, trasferimento tra motori o approvazione autorizzata dalla pagina.
- La vista delle sessioni deve mostrare proprietario, controllore, motivazione ed esito effettivi.

**Verifiche del passo:** `planned: User/agent identity derived server-side; engine/scope/permission immutable.`; `planned: No failure fallback, cross-engine transfer or page-authorized approval.`; `planned: Session view shows real owner/controller/reason/outcome.`.

**Gate collegati, da registrare separatamente:** B5, B6, B7, B8, B9, AT-11, M15, X5.

**Requisiti coperti dal piano:** FRD-0011/R01, FRD-0011/R02, FRD-0011/R03, FRD-0011/R04, FRD-0011/R05, FRD-0011/R06, FRD-0011/R07, FRD-0011/R08, FRD-0011/R09, FRD-0011/R10, FRD-0011/R11, FRD-0011/R12, FRD-0011/R13, FRD-0011/R14, FRD-0011/R22, FRD-0011/R23, FRD-0011/R24, FRD-0011/R25, FRD-0011/R26, FRD-0011/R27, FRD-0011/R28, FRD-0011/R30, FRD-0011/R31, FRD-0011/R32, FRD-0011/R33, FRD-0011/R34, FRD-0011/R35, FRD-0011/R36, FRD-0011/R40, FRD-0011/R41, FRD-0011/R42, FRD-0011/R43, FRD-0011/R44, FRD-0011/R46, FRD-0011/R47, FRD-0011/R48, FRD-0011/R52.

**Commit proposto:** `feat(f17): Contratto delle richieste browser e instradamento immutabile`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/browser/, apps/desktop/src/browser/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f18"></a>
## F18 · Moli: estrazione singola con limiti e anteprima statica

Involucro di fetch con operazioni fisse e versione vincolata, estrazione limitata e artefatti di anteprima privati.

**Ambito:** v1. **Dipendenze:** F17, C42. **Stima:** 6–16 ore.

**File/componenti:** `crates/browser/`; `apps/desktop/src/browser/`; `docs/verification/`.

**Accettazione osservabile:**

- Non esporre serve, listener o eval generico; il percorso testuale non deve caricare risorse facoltative.
- Applicare gli ambiti a collegamenti simbolici, redirect, sottorisorse, servizi e proxy.
- L’interfaccia MCP deve funzionare entro un fetch singolo oppure essere esplicitamente rinviata.
- Misurare carichi equivalenti a freddo e a caldo sull’intero albero dei processi prima di dichiarare prestazioni migliori.

**Verifiche del passo:** `planned: No serve/listener/general eval; text path loads no optional resources.`; `planned: Symlink/redirect/subresource/service/proxy scope enforced.`; `planned: MCP UI works within one-shot fetch or explicitly deferred.`; `planned: Equivalent whole-tree cold/warm workload measured before performance claim.`.

**Gate collegati, da registrare separatamente:** B1, B2, B3, B4, B6, B7, B8, B10, X3.

**Requisiti coperti dal piano:** FRD-0011/R01, FRD-0011/R07, FRD-0011/R09, FRD-0011/R13, FRD-0011/R14, FRD-0011/R15, FRD-0011/R16, FRD-0011/R17, FRD-0011/R18, FRD-0011/R20, FRD-0011/R21, FRD-0011/R27, FRD-0011/R28, FRD-0011/R29, FRD-0011/R30, FRD-0011/R35, FRD-0011/R37, FRD-0011/R38, FRD-0011/R39, FRD-0011/R40, FRD-0011/R41, FRD-0011/R42, FRD-0011/R43, FRD-0011/R44, FRD-0011/R45, FRD-0011/R48, FRD-0011/R49, FRD-0011/R50.

**Commit proposto:** `feat(f18): Moli: estrazione singola con limiti e anteprima statica`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/browser/, apps/desktop/src/browser/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f19"></a>
## F19 · Browser Patchright esplicito e ciclo di vita con un solo controllore

Companion Chrome esterno caricato su richiesta, con trasporto privato autenticato, profili consentiti e riconciliazione dopo annullamento o arresto anomalo.

**Ambito:** v1. **Dipendenze:** F17, C13. **Stima:** 8–20 ore.

**File/componenti:** `crates/browser/`; `apps/desktop/src/browser/`; `docs/verification/`.

**Accettazione osservabile:**

- Il consenso generale non deve avviare nulla; richiedere un comando UI esplicito; non installare automaticamente componenti mancanti.
- Il controllo dell’utente deve sospendere l’agente; invalidare azioni e approvazioni obsolete.
- Chiusura normale, annullamento, errori, arresti anomali e riutilizzo dei PID non devono lasciare processi o listener gestiti.

**Verifiche del passo:** `planned: Opt-in alone starts nothing; explicit UI request required; missing binary never auto-installs.`; `planned: User takeover suspends agent; stale actions/approvals invalidated.`; `planned: Normal/cancel/error/crash/PID-reuse cleanup leaves no managed process/listener.`.

**Gate collegati, da registrare separatamente:** B4, B5, B6, B7, B8, B9, B10, AT-11.

**Requisiti coperti dal piano:** FRD-0011/R02, FRD-0011/R03, FRD-0011/R04, FRD-0011/R05, FRD-0011/R06, FRD-0011/R07, FRD-0011/R08, FRD-0011/R09, FRD-0011/R10, FRD-0011/R11, FRD-0011/R12, FRD-0011/R13, FRD-0011/R18, FRD-0011/R19, FRD-0011/R21, FRD-0011/R22, FRD-0011/R23, FRD-0011/R24, FRD-0011/R25, FRD-0011/R26, FRD-0011/R27, FRD-0011/R28, FRD-0011/R29, FRD-0011/R30, FRD-0011/R31, FRD-0011/R32, FRD-0011/R33, FRD-0011/R34, FRD-0011/R35, FRD-0011/R36, FRD-0011/R37, FRD-0011/R38, FRD-0011/R39, FRD-0011/R41, FRD-0011/R42, FRD-0011/R43, FRD-0011/R44, FRD-0011/R45, FRD-0011/R48, FRD-0011/R49, FRD-0011/R50, FRD-0011/R51, FRD-0011/R52.

**Commit proposto:** `feat(f19): Browser Patchright esplicito e ciclo di vita con un solo controllore`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/browser/, apps/desktop/src/browser/. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### F19.1 · Protocollo privato del companion browser e verifica del trasporto

Contratto tipizzato per il companion Patchright con identita, richiesta e ambito immutabili; trasporto privato autenticato disattivato finche B4 non passa.

Stima: 2–5 ore. File: `crates/browser/`, `browser-companion/`, `docs/verification/`.

Accettazione: L'app resta avviabile senza helper o Chrome e open-ui non supportato restituisce un esito esplicito. Nessuna pagina, redirect o messaggio agente puo raggiungere un endpoint di controllo non autenticato o cambiare motore e permessi.

Verifiche pianificate: `pianificato: Compilare il contratto e provare messaggi validi, errati e privi di autenticazione con helper sintetico.`; `pianificato: Inventariare listener, processi e tentativi da pagine ostili; registrare B4 prima di abilitare il trasporto.`.

Commit: `feat(f19.1): Protocollo privato del companion browser e verifica del trasporto`. Registrare stato e prove sotto `progress.json` → `steps` → `F19` → `units` → `F19.1`.

### F19.2 · Avvio esplicito di Chrome e profilo separato

Caricamento su richiesta del componente Patchright verificato, con opt-in generale e comando utente per ogni sessione; profilo temporaneo separato per motore.

Stima: 3–7 ore. File: `crates/browser/`, `browser-companion/`, `apps/desktop/src/browser/`, `docs/verification/`.

Accettazione: Abilitare l'impostazione non avvia processi; l'assenza di componenti non causa download o installazioni automatici. Una richiesta esplicita apre Chrome esterno solo nel relativo ambito, senza profilo personale, cookie trasferiti o ripiego da Moli.

Verifiche pianificate: `pianificato: Provare componente assente, impostazione disattivata, consenso senza richiesta e avvio esplicito.`; `pianificato: Verificare profili, inventario dei processi e richieste di rete prima e dopo il comando; mantenere la funzione disabilitata se B4/B5/B8 falliscono.`.

Commit: `feat(f19.2): Avvio esplicito di Chrome e profilo separato`. Registrare stato e prove sotto `progress.json` → `steps` → `F19` → `units` → `F19.2`.

### F19.3 · Un solo controllore e chiusura completa delle sessioni browser

Passaggio esplicito del controllo utente/agente, invalidazione delle operazioni obsolete e percorso unico di chiusura con timeout e pulizia dei discendenti.

Stima: 2–5 ore. File: `crates/browser/`, `browser-companion/`, `apps/desktop/src/browser/`, `docs/verification/`.

Accettazione: Durante il controllo utente le azioni agente restano sospese; richieste, approvazioni o risultati di una sessione chiusa non hanno effetto. Chiusura, annullamento, avvio fallito e arresto anomalo non lasciano discendenti o endpoint gestiti; l'identita evita di terminare processi estranei per riutilizzo del PID.

Verifiche pianificate: `pianificato: Eseguire casi concorrenti con effetti esterni sintetici per secondo controller, takeover, resume e azioni obsolete.`; `pianificato: Provare il ciclo di vita completo e il recupero sui sistemi previsti, registrando B7/B9 e i casi non disponibili.`.

Commit: `feat(f19.3): Un solo controllore e chiusura completa delle sessioni browser`. Registrare stato e prove sotto `progress.json` → `steps` → `F19` → `units` → `F19.3`.

### F19.4 · Verifica integrata di riservatezza e prestazioni del browser UI

Record finale B4-B10 con profili, ambiti, artefatti privati e misurazioni riproducibili dell’intero albero dei processi; chiusura del passo padre soltanto dopo i gate applicabili.

Stima: 1–3 ore. File: `crates/browser/`, `browser-companion/`, `docs/verification/`.

Accettazione: Redirect, sottorisorse, servizi locali e contenuti ostili non ampliano gli ambiti, non aprono un altro motore e non espongono credenziali. Non dichiarare risparmi o successo visivo da soli risultati testuali; pubblicare misure e limiti, mantenendo utilizzabile l'app con il browser UI disabilitato dove fallisce.

Verifiche pianificate: `pianificato: Eseguire B6/B8/AT-11 con segreti canarino e verificare log, database e conservazione degli artefatti.`; `pianificato: Misurare B10 a freddo e a caldo con versioni e carichi equivalenti; annotare il budget ancora da decidere senza inventare risultati.`.

Commit: `feat(f19.4): Verifica integrata di riservatezza e prestazioni del browser UI`. Registrare stato e prove sotto `progress.json` → `steps` → `F19` → `units` → `F19.4`.

<a id="f20"></a>
## F20 · Registro dei token per sessione, riferimento iniziale e regole di proprietà

Registro per sessione di utilizzo, cache, compattazioni e ripetizioni degli strumenti; riferimento iniziale e stato derivati dalla configurazione effettiva del sidecar.

**Ambito:** v1. **Dipendenze:** C23, C13. **Stima:** 4–12 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** FRD-0013/R08-R12/R29 e FRD-0007/R40 si applicano alle componenti attive: nessuna installazione Daodan per baseline o ownership.

- Richiedere dieci sessioni reali per harness prima delle dichiarazioni; non riusare percentuali dei fornitori.
- Escludere addestramento o calibrazione dei modelli, integrazione CAM, secondo proxy e riscrittura del testo umano.
- Collegare ai record le misurazioni di definizioni degli strumenti, cache della verbosità e peso delle immagini.

**Verifiche del passo:** `planned: Ten real sessions per harness before claims; vendor percentages not reused.`; `planned: No model training/calibration, CAM integration, second proxy or rewritten human text.`; `planned: Tool definitions, verbosity cache and image-weight measurements linked to records.`.

**Gate collegati, da registrare separatamente:** T1, T4, T5, T6, T7, T8, T9, T10, T11, T16, X4.

**Requisiti coperti dal piano:** FRD-0013/R01, FRD-0013/R02, FRD-0013/R03, FRD-0013/R04, FRD-0013/R05, FRD-0013/R06, FRD-0013/R07, FRD-0013/R08, FRD-0013/R09, FRD-0013/R11, FRD-0013/R12, FRD-0013/R29, FRD-0013/R30, FRD-0013/R31, FRD-0013/R32, FRD-0013/R33, FRD-0013/R34, FRD-0013/R35, FRD-0013/R36, FRD-0013/R38, FRD-0013/R39, FRD-0013/R40, FRD-0017/R08.

**Commit proposto:** `feat(f20): Registro dei token per sessione, riferimento iniziale e regole di proprietà`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f21"></a>
## F21 · Compattazione decisionale Claude basata soltanto su regole

Coppie chiamata/risultato degli strumenti con prefisso e messaggi recenti protetti, decisioni deterministiche ed endpoint locale basato su regole.

**Ambito:** v1. **Dipendenze:** F20, C15, C13. **Stima:** 6–16 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Conservazione, troncamento e rimozione devono preservare l’associazione degli strumenti e percorsi, errori e vincoli necessari.
- Intercettare o disabilitare la compattazione nativa; un solo componente deve possederla.
- Errori dello scorer o riduzione insufficiente devono usare il ripiego strutturato; il turno del modello principale non deve attendere lo scoring.

**Verifiche del passo:** `planned: Keep/truncate/remove preserve tool pairing and required paths/errors/constraints.`; `planned: Native compaction intercepted or stood down; one owner only.`; `planned: Scorer failure/insufficient reduction follows structured fallback; no frontier-turn scoring wait.`.

**Gate collegati, da registrare separatamente:** T11, T13, T14.

**Requisiti coperti dal piano:** FRD-0013/R02, FRD-0013/R03, FRD-0013/R04, FRD-0013/R06, FRD-0013/R07, FRD-0013/R11, FRD-0013/R13, FRD-0013/R14, FRD-0013/R15, FRD-0013/R16, FRD-0013/R17, FRD-0013/R18, FRD-0013/R19, FRD-0013/R20, FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f21): Compattazione decisionale Claude basata soltanto su regole`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f22"></a>
## F22 · Scoring incrementale julia1 su stato esteso solo in coda

Scoring locale System One con stato incrementale limitato, caratteristiche deterministiche e soglia fornita con il modello esterno.

**Ambito:** v1. **Dipendenze:** F07, F21. **Stima:** 4–12 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Confrontare ripetizioni degli strumenti, cache ed esito dei compiti con il riferimento basato soltanto su regole.
- Misurare la latenza incrementale con quattro agenti.
- La configurazione di routing ospitato non deve mai inviare compattazioni o sintesi degli strumenti all’endpoint remoto.

**Verifiche del passo:** `planned: Compare re-run/cache/task outcome against rules-only baseline.`; `planned: Incremental latency measured with four agents.`; `planned: Hosted routing configuration never sends compaction/digest to remote endpoint.`.

**Gate collegati, da registrare separatamente:** T11, T12, T14, T16.

**Requisiti coperti dal piano:** FRD-0013/R13, FRD-0013/R14, FRD-0013/R15, FRD-0013/R16, FRD-0013/R17, FRD-0013/R18, FRD-0013/R19, FRD-0013/R20, FRD-0013/R21, FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f22): Scoring incrementale julia1 su stato esteso solo in coda`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f23-codex"></a>
## F23-codex · Adattatore di compattazione decisionale: Codex

Un punto di integrazione Codex vincolato alla versione, con un solo proprietario del traffico e della cronologia.

**Ambito:** v1. **Dipendenze:** F22, C15, C13, C16. **Stima:** 4–14 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Il tasso di utilizzo della cache deve restare entro il rumore misurato e la compattazione nativa deve essere assente.
- Le modifiche nel percorso del traffico devono essere apportate all’unica sorgente upstream del sidecar.
- Hermes e DeepSeek richiedono copertura del proxy; l’assenza di disattivazione della compattazione Hermes ne impedisce l’abilitazione.

**Verifiche del passo:** `planned: Cache hit rate remains within measured noise and native compaction absent.`; `planned: Wire-path changes land in upstream sidecar single source.`; `planned: Hermes/dsh require proxy coverage; Hermes suppression absence blocks enablement.`.

**Gate collegati, da registrare separatamente:** T11, T14, T15, HM12, DS12.

**Requisiti coperti dal piano:** FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R28, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f23-codex): Adattatore di compattazione decisionale: Codex`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f23-opencode"></a>
## F23-opencode · Adattatore di compattazione decisionale: OpenCode

Un punto di integrazione OpenCode vincolato alla versione, con un solo proprietario del traffico e della cronologia.

**Ambito:** v1. **Dipendenze:** F22, C15, C13, C17. **Stima:** 4–14 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Il tasso di utilizzo della cache deve restare entro il rumore misurato e la compattazione nativa deve essere assente.
- Le modifiche nel percorso del traffico devono essere apportate all’unica sorgente upstream del sidecar.
- Hermes e DeepSeek richiedono copertura del proxy; l’assenza di disattivazione della compattazione Hermes ne impedisce l’abilitazione.

**Verifiche del passo:** `planned: Cache hit rate remains within measured noise and native compaction absent.`; `planned: Wire-path changes land in upstream sidecar single source.`; `planned: Hermes/dsh require proxy coverage; Hermes suppression absence blocks enablement.`.

**Gate collegati, da registrare separatamente:** T11, T14, T15, HM12, DS12.

**Requisiti coperti dal piano:** FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R28, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f23-opencode): Adattatore di compattazione decisionale: OpenCode`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f23-pi"></a>
## F23-pi · Adattatore di compattazione decisionale: Pi

Un punto di integrazione Pi vincolato alla versione, con un solo proprietario del traffico e della cronologia.

**Ambito:** v1. **Dipendenze:** F22, C15, C13, C18. **Stima:** 4–14 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Il tasso di utilizzo della cache deve restare entro il rumore misurato e la compattazione nativa deve essere assente.
- Le modifiche nel percorso del traffico devono essere apportate all’unica sorgente upstream del sidecar.
- Hermes e DeepSeek richiedono copertura del proxy; l’assenza di disattivazione della compattazione Hermes ne impedisce l’abilitazione.

**Verifiche del passo:** `planned: Cache hit rate remains within measured noise and native compaction absent.`; `planned: Wire-path changes land in upstream sidecar single source.`; `planned: Hermes/dsh require proxy coverage; Hermes suppression absence blocks enablement.`.

**Gate collegati, da registrare separatamente:** T11, T14, T15, HM12, DS12.

**Requisiti coperti dal piano:** FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R28, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f23-pi): Adattatore di compattazione decisionale: Pi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f23-hermes"></a>
## F23-hermes · Adattatore di compattazione decisionale: Hermes Agent

Un punto di integrazione Hermes Agent vincolato alla versione, con un solo proprietario del traffico e della cronologia.

**Ambito:** v1. **Dipendenze:** F22, C15, C13, C19. **Stima:** 4–14 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Il tasso di utilizzo della cache deve restare entro il rumore misurato e la compattazione nativa deve essere assente.
- Le modifiche nel percorso del traffico devono essere apportate all’unica sorgente upstream del sidecar.
- Hermes e DeepSeek richiedono copertura del proxy; l’assenza di disattivazione della compattazione Hermes ne impedisce l’abilitazione.

**Verifiche del passo:** `planned: Cache hit rate remains within measured noise and native compaction absent.`; `planned: Wire-path changes land in upstream sidecar single source.`; `planned: Hermes/dsh require proxy coverage; Hermes suppression absence blocks enablement.`.

**Gate collegati, da registrare separatamente:** T11, T14, T15, HM12, DS12.

**Requisiti coperti dal piano:** FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R28, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f23-hermes): Adattatore di compattazione decisionale: Hermes Agent`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f23-deepseek"></a>
## F23-deepseek · Adattatore di compattazione decisionale: DeepSeek Harness

Un punto di integrazione DeepSeek Harness vincolato alla versione, con un solo proprietario del traffico e della cronologia.

**Ambito:** v1. **Dipendenze:** F22, C15, C13, C20. **Stima:** 4–14 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Il tasso di utilizzo della cache deve restare entro il rumore misurato e la compattazione nativa deve essere assente.
- Le modifiche nel percorso del traffico devono essere apportate all’unica sorgente upstream del sidecar.
- Hermes e DeepSeek richiedono copertura del proxy; l’assenza di disattivazione della compattazione Hermes ne impedisce l’abilitazione.

**Verifiche del passo:** `planned: Cache hit rate remains within measured noise and native compaction absent.`; `planned: Wire-path changes land in upstream sidecar single source.`; `planned: Hermes/dsh require proxy coverage; Hermes suppression absence blocks enablement.`.

**Gate collegati, da registrare separatamente:** T11, T14, T15, HM12, DS12.

**Requisiti coperti dal piano:** FRD-0013/R23, FRD-0013/R24, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R28, FRD-0013/R35, FRD-0013/R36, FRD-0013/R37, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f23-deepseek): Adattatore di compattazione decisionale: DeepSeek Harness`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f24"></a>
## F24 · Sintesi asincrone degli strumenti e originali recuperabili

Parser per formati stabili e sintesi locali asincrone con recupero degli originali, senza modificare il testo dell’utente o dell’assistente.

**Ambito:** v1. **Dipendenze:** F20, F22, C13. **Stima:** 4–12 ore.

**File/componenti:** `crates/token-economy/`; `docs/verification/`.

**Accettazione osservabile:**

- Output di build, test, elenchi e confronti prodotti dagli strumenti devono conservare i segnali essenziali.
- Non riscrivere il prefisso fuori cache né sovrapporre riduzioni con perdita.
- Misurare effetti su latenza, cache e ripetizioni prima di dichiarare abilitate le capacità.

**Verifiche del passo:** `planned: Build/test/listing/diff-shaped tool output retains critical signal.`; `planned: No uncached-prefix rewrite or stacked lossy reducers.`; `planned: Latency/cache/re-run impact measured before enabled capability claims.`.

**Gate collegati, da registrare separatamente:** T6, T8, T9, T10, T11, T12, T14, T16.

**Requisiti coperti dal piano:** FRD-0013/R03, FRD-0013/R04, FRD-0013/R05, FRD-0013/R06, FRD-0013/R08, FRD-0013/R09, FRD-0013/R10, FRD-0013/R11, FRD-0013/R12, FRD-0013/R17, FRD-0013/R18, FRD-0013/R19, FRD-0013/R20, FRD-0013/R25, FRD-0013/R26, FRD-0013/R27, FRD-0013/R28, FRD-0013/R29, FRD-0013/R30, FRD-0013/R31, FRD-0013/R33, FRD-0013/R34, FRD-0013/R35, FRD-0013/R36, FRD-0013/R38, FRD-0013/R39.

**Commit proposto:** `feat(f24): Sintesi asincrone degli strumenti e originali recuperabili`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/token-economy/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f25"></a>
## F25 · Instradamento locale del modello all’avvio della sessione

Filtri su un insieme chiuso di modelli, selettore choice/signals/off e conservazione del modello iniziale in caso di errore.

**Ambito:** v1. **Dipendenze:** F07, F20, C15, C14. **Stima:** 5–14 ore.

**File/componenti:** `crates/routing/`; `apps/desktop/src/settings/`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** signals basato su task_router.py di FRD-0015/R08 e rinviato; choice/off ed executor indipendenti restano attivi.

- Non inventare modelli né sceglierne fuori dall’insieme; non cambiare modello durante una conversazione.
- Scarsa confidenza, reject o timeout devono mantenere il modello predefinito dell’istanza.
- Confrontare il punto di avvio di ogni harness e il registro dei costi/esiti con il riferimento del modello predefinito.

**Verifiche del passo:** `planned: No model invented/outside set; no within-conversation model switch.`; `planned: Low confidence/reject/timeout keeps instance default.`; `planned: Per-harness launch seam and cost/task outcome ledger compared with default baseline.`.

**Gate collegati, da registrare separatamente:** MR1, MR2, MR-3, MR4.

**Requisiti coperti dal piano:** FRD-0013/R22, FRD-0015/R01, FRD-0015/R02, FRD-0015/R03, FRD-0015/R04, FRD-0015/R05, FRD-0015/R06, FRD-0015/R07, FRD-0015/R08, FRD-0015/R09, FRD-0015/R10, FRD-0015/R11, FRD-0015/R12, FRD-0015/R15, FRD-0015/R16, FRD-0015/R17, FRD-0015/R18, FRD-0015/R24, FRD-0015/R25.

**Commit proposto:** `feat(f25): Instradamento locale del modello all’avvio della sessione`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/routing/, apps/desktop/src/settings/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f26"></a>
## F26 · Consenso alle decisioni ospitate, chiave nel vault e integrazioni dei sottoagenti

Adattatori delle fonti decisionali e chiave conservata soltanto nel vault del sistema operativo, con sintesi oscurata su consenso e routing per chiamata dove provato.

**Ambito:** v1. **Dipendenze:** F25, C11. **Stima:** 5–14 ore.

**File/componenti:** `crates/routing/`; `crates/credentials/`; `docs/verification/`.

**Accettazione osservabile:**

- Inviare all’esecutore ospitato soltanto sintesi e schede dopo il consenso.
- La chiave non deve comparire in database, configurazioni, log o manifesti; non leggere credenziali degli agenti.
- Dichiarare non supportate le integrazioni sconosciute dei sottoagenti; il servizio non deve occupare il percorso di esecuzione delle richieste.
- Compattazioni e sintesi degli strumenti devono restare locali con le stesse impostazioni.

**Verifiche del passo:** `planned: Only digest/cards reach hosted executor after opt-in.`; `planned: Key never appears in DB/config/log/manifest; no agent credential read.`; `planned: Unknown subagent seam unsupported; service never takes request-serving wire.`; `planned: Compaction/digests still local under same settings.`.

**Gate collegati, da registrare separatamente:** MR1, MR2, MR-3, MR4, T16, X4.

**Requisiti coperti dal piano:** FRD-0013/R21, FRD-0013/R22, FRD-0014/R28, FRD-0015/R01, FRD-0015/R02, FRD-0015/R03, FRD-0015/R04, FRD-0015/R05, FRD-0015/R06, FRD-0015/R07, FRD-0015/R08, FRD-0015/R09, FRD-0015/R10, FRD-0015/R11, FRD-0015/R12, FRD-0015/R13, FRD-0015/R14, FRD-0015/R15, FRD-0015/R16, FRD-0015/R17, FRD-0015/R18, FRD-0015/R24, FRD-0015/R25.

**Commit proposto:** `feat(f26): Consenso alle decisioni ospitate, chiave nel vault e integrazioni dei sottoagenti`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/routing/, crates/credentials/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c40"></a>
## C40 · Verificare i confini delle credenziali nelle chiamate ausiliarie, nel vault del router e nei browser

Verifiche trasversali sulle credenziali condivise con le implementazioni di titoli, router e browser.

**Ambito:** v1. **Dipendenze:** C13, C15, F26, F19. **Stima:** 8–16 ore.

**File/componenti:** `crates/sidecar/src/auxiliary.rs`; `crates/security/tests/credential_boundaries.rs`.

**Accettazione osservabile:**

- Le chiamate ausiliarie al provider usano l'autenticazione già presente nell'agente o sidecar; soltanto la chiave del router può risiedere nel vault del sistema e soltanto il sidecar router la legge.
- Gli ambienti dei sottoprocessi browser non ereditano credenziali degli agenti; cookie, token e intestazioni di autorizzazione non raggiungono log, database, trascrizioni o sintesi.
- Titoli, router e browser non disponibili restano rifiutati finché non passano le verifiche delle rispettive FRD.

**Verifiche del passo:** `synthetic token-canary environment/log tests`; `OS-vault binding test without production credentials`.

**Gate collegati, da registrare separatamente:** SC10, I7, B8, X5.

**Requisiti coperti dal piano:** FRD-0003/R16, FRD-0003/R18, FRD-0003/R19.

**Commit proposto:** `feat(c40): Verificare i confini delle credenziali nelle chiamate ausiliarie, nel vault del router e nei browser`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/sidecar/src/auxiliary.rs, crates/security/tests/credential_boundaries.rs. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f27"></a>
## F27 · Selezione degli strumenti differiti senza invalidare la cache

Selettore per prompt con lo stesso esecutore decisionale; strumenti di recupero sempre precaricati.

**Ambito:** v1. **Dipendenze:** F25, C15. **Stima:** 4–12 ore.

**File/componenti:** `crates/routing/`; `docs/verification/`.

**Accettazione osservabile:**

- Usare un insieme chiuso di strumenti differiti e, facoltativamente, le osservazioni TE; se la soglia non è raggiunta non precaricare nulla.
- Aggiungere gli schemi soltanto alla coda della richiesta e preservare il prefisso.
- Abilitare soltanto hook verificati e misurare separatamente la latenza del selettore.

**Verifiche del passo:** `planned: Closed deferred set and optional TE prior; threshold failure preloads nothing.`; `planned: Schemas added only at request tail; prefix unchanged.`; `planned: Only verified hook seams enabled; chooser latency measured separately.`.

**Gate collegati, da registrare separatamente:** MR4, T14.

**Requisiti coperti dal piano:** FRD-0013/R08, FRD-0013/R22, FRD-0013/R36, FRD-0015/R19, FRD-0015/R20, FRD-0015/R21, FRD-0015/R22, FRD-0015/R23, FRD-0015/R24, FRD-0015/R25.

**Commit proposto:** `feat(f27): Selezione degli strumenti differiti senza invalidare la cache`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/routing/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f28"></a>
## F28 · Installazione facoltativa TokenEconomizer e rimozione degli strumenti solo su proposta

Modulo con versione fissata e stato nel box, disattivato inizialmente, con verifica di interprete/versione, prove proprie e lista di divieti per l’avvio successivo.

**Ambito:** v1. **Dipendenze:** F20, C15, C13. **Stima:** 6–16 ore.

**File/componenti:** `crates/modules/tokeneconomizer/`; `apps/desktop/src/settings/`; `docs/verification/`.

**Accettazione osservabile:**

- Non offrire il modulo a Hermes o DeepSeek; non scrivere nello stato della home di sistema o nelle impostazioni del repository.
- Proteggere bili e image_media, chiudere il percorso apply e il relativo invito, applicare il proprio timer di inattività.
- Il percorso B di OpenCode deve considerare i metadati Code Mode e i nomi protetti.
- La lista degli strumenti deve restare fissa durante una sessione attiva o ripresa.

**Verifiche del passo:** `planned: No Hermes/dsh offer; no system-home state/repository settings edits.`; `planned: Protect bili/image_media; close apply route and notice; own idle timer enforced.`; `planned: OpenCode route B includes Code Mode metadata and protected names.`; `planned: Tool list remains fixed during running/resumed session.`.

**Gate collegati, da registrare separatamente:** TE1, TE2, TE3, TE5, TE6, TE7, TE8, TE12, TE13.

**Requisiti coperti dal piano:** FRD-0013/R30, FRD-0013/R32, FRD-0016/R01, FRD-0016/R02, FRD-0016/R03, FRD-0016/R04, FRD-0016/R05, FRD-0016/R06, FRD-0016/R07, FRD-0016/R08, FRD-0016/R09, FRD-0016/R10, FRD-0016/R11, FRD-0016/R12, FRD-0016/R13, FRD-0016/R21, FRD-0016/R22, FRD-0016/R23, FRD-0016/R24.

**Commit proposto:** `feat(f28): Installazione facoltativa TokenEconomizer e rimozione degli strumenti solo su proposta`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/modules/tokeneconomizer/, apps/desktop/src/settings/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f29"></a>
## F29 · TokenEconomizer: envfacts e protezione dai cicli subordinata alle verifiche

Envfacts basati soltanto su modelli fissi e protezione dai cicli per Claude/Pi vincolata a versione e provider.

**Ambito:** v1. **Dipendenze:** F28, F07. **Stima:** 4–12 ore.

**File/componenti:** `crates/modules/tokeneconomizer/`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** FRD-0016/R17 misura il solo inventario effettivamente abilitato, senza installare Daodan; il confronto con la memoria Daodan R19 e rinviato.

- Mantenere il proposer disattivato; i fatti fissi devono sopravvivere a promptRewrite.
- Verificare costo e latenza del giudice e falsi fallimenti del recupero prima dell’abilitazione.
- Mantenere disattivata la protezione Codex/OpenCode finché upstream non supporta endpoint e inoltro corretti.
- Nessun nuovo proxy o riduttore della cronologia; confrontare modulo attivo e inattivo senza sommare percentuali di risparmio.

**Verifiche del passo:** `planned: Proposer off; fixed facts survive promptRewrite.`; `planned: Judge cost/latency and recovery false-failure tests pass before enabling.`; `planned: Codex/OpenCode guard remains off pending upstream endpoint/redirect support.`; `planned: No new proxy/history reducer; ledger compares module on/off without additive saving claims.`.

**Gate collegati, da registrare separatamente:** TE4, TE6, TE7, TE8, TE9, TE10, TE11.

**Requisiti coperti dal piano:** FRD-0016/R14, FRD-0016/R15, FRD-0016/R16, FRD-0016/R17, FRD-0016/R18, FRD-0016/R20, FRD-0016/R21, FRD-0016/R22, FRD-0016/R23, FRD-0016/R24.

**Commit proposto:** `feat(f29): TokenEconomizer: envfacts e protezione dai cicli subordinata alle verifiche`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/modules/tokeneconomizer/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f30"></a>
## F30 · Widget finale della V1: prova per piattaforma e avvisi di consultazione

Widget in un processo separato alimentato dagli eventi, senza sottrarre il focus, con apertura della sessione su clic e ripiego per compositor.

**Ambito:** v1. **Dipendenze:** F02, F05, F06, C29, C15, F16, F19, F24, F27, F29, C30, C33, C36, C37, C43, C44. **Stima:** 6–16 ore.

**File/componenti:** `apps/widget/`; `docs/verification/`.

**Accettazione osservabile:**

- Verificare risorse a riposo e posizionamento del toolkit sulle piattaforme previste prima dell’abilitazione.
- La bolla deve ricevere gli eventi entro un secondo e non sottrarre mai la digitazione.
- Funzionare senza finestra principale e riconciliare l’identità con più finestre.
- Verificare silenziamento, disattivazione, movimento ridotto e ripiego Wayland su sole notifiche.

**Verifiche del passo:** `planned: Toolkit idle resources/placement tested on target platforms before enablement.`; `planned: Bubble receives events under one second and never steals typing.`; `planned: No main window required; multiple windows reconcile identity.`; `planned: Muted/disabled/reduced-motion and Wayland notification-only fallback verified.`.

**Gate collegati, da registrare separatamente:** W1, W2, W3, W4, W6, W10.

**Requisiti coperti dal piano:** FRD-0010/R24, FRD-0010/R25, FRD-0010/R26, FRD-0010/R28, FRD-0010/R29, FRD-0010/R30, FRD-0010/R31, FRD-0010/R36, FRD-0010/R37, FRD-0010/R44.

**Commit proposto:** `feat(f30): Widget finale della V1: prova per piattaforma e avvisi di consultazione`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/widget/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f31"></a>
## F31 · Widget: risposte rapide legate alla richiesta e pipeline delle animazioni revisionata

Consenti/rifiuta soltanto per richieste native e atlante di animazione approvato; segnaposto originale fino alla decisione sulla distribuzione grafica.

**Ambito:** v1. **Dipendenze:** F30, C11. **Stima:** 6–16 ore.

**File/componenti:** `apps/widget/`; `docs/assets/`; `docs/verification/`.

**Accettazione osservabile:**

- Rifiutare una risposta obsoleta se la richiesta cambia prima del clic; non approvare da euristiche del testo sullo schermo.
- Non sottrarre il focus della tastiera; mostrare il comando effettivo del contenuto nativo.
- Realizzare un pilota di tre fotogrammi prima della generazione completa e verificare trasparenza, scala, dimensioni e movimento ridotto.
- Mantenere esplicita la decisione sulla distribuzione di Oni; non formulare affermazioni legali non verificate.

**Verifiche del passo:** `planned: Prompt changed before click refuses stale answer; no screen heuristic approvals.`; `planned: No key focus theft; real command shown from native payload.`; `planned: Three-frame pilot before full generation; alpha/scaling/size/reduced-motion tested.`; `planned: Oni distribution decision remains explicit; no unsupported legal claim.`.

**Gate collegati, da registrare separatamente:** W3, W5, W7, W8, W9.

**Requisiti coperti dal piano:** FRD-0010/R27, FRD-0010/R33, FRD-0010/R34, FRD-0010/R35, FRD-0010/R37, FRD-0010/R44, FRD-0017/R15.

**Commit proposto:** `feat(f31): Widget: risposte rapide legate alla richiesta e pipeline delle animazioni revisionata`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire apps/widget/, docs/assets/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f32pkg"></a>
## F32PKG · Programmi di installazione, fornitura dei runtime e primo hop su sistema pulito

Artefatti installabili e gestione esplicita di Node e componenti facoltativi, con tempi misurati su sistemi puliti.

**Ambito:** v1. **Dipendenze:** F32CI, C29, C14, C13, C16. **Stima:** 6–18 ore.

**File/componenti:** `packaging/`; `.github/workflows/`; `docs/verification/`.

**Accettazione osservabile:**

- Misurare l'installazione fino al primo hop su Windows pulito ed elencare i fallimenti, compreso il runtime WebView2.
- Non modificare installatori o aggiornamenti degli agenti di sistema; installare copie proprie soltanto su scelta dell’utente.
- Gestire componenti offline o facoltativi mancanti con errori chiari, senza download automatici.
- Provare Windows x64; macOS e Linux restano rinviati.

**Verifiche del passo:** `planned: Windows clean install to first hop timed; failures listed.`; `planned: No system-agent installer/update side effects; own install only by user choice.`; `planned: Offline/missing optional components fail clearly without automatic downloads.`; `planned: Windows x64 proved; other targets reported as deferred.`.

**Gate collegati, da registrare separatamente:** X1, X2, X3, X4, SC13, P17, P18, AT-12.

**Requisiti coperti dal piano:** FRD-0017/R01, FRD-0017/R02, FRD-0017/R03, FRD-0017/R04, FRD-0017/R05, FRD-0017/R08, FRD-0017/R09, FRD-0017/R13, FRD-0017/R14, FRD-0017/R15, FRD-0017/R16, FRD-0017/R17, FRD-0017/R18.

**Commit proposto:** `feat(f32pkg): Programmi di installazione, fornitura dei runtime e primo hop su sistema pulito`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire packaging/, .github/workflows/. Preparare la prima prova di accettazione prima di modificare il comportamento.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### F32PKG.1 · Composizione del pacchetto e runtime facoltativi

Manifesto dei componenti distribuiti, strategia Node e controllo offline delle dipendenze richieste o facoltative; nessuna installazione implicita di agenti di sistema.

Stima: 2–6 ore. File: `packaging/`, `docs/implementation/`, `docs/verification/`.

Accettazione: La build normale resta avviabile e i componenti mancanti producono esiti espliciti senza download automatici. Runtime, licenze, hash e provenienza del sidecar sono inventariati; copie agenti proprie richiedono la scelta dell'utente.

Verifiche pianificate: `pianificato: Eseguire build e avvio senza componenti facoltativi e in ambiente offline.`; `pianificato: Controllare il manifesto del pacchetto contro versioni vincolate e avvisi di licenza, registrando SC13/X3.`.

Commit: `feat(f32pkg.1): Composizione del pacchetto e runtime facoltativi`. Registrare stato e prove sotto `progress.json` → `steps` → `F32PKG` → `units` → `F32PKG.1`.

### F32PKG.2 · Pacchetto installabile per Windows

Ricette riproducibili per gli artefatti Windows x64 previste, lasciando firma e notarizzazione al passo F32SIGN.

Stima: 2–6 ore. File: `packaging/`, `.github/workflows/`, `docs/verification/`.

Accettazione: Ogni artefatto generato installa e avvia la stessa applicazione senza modificare programmi o impostazioni degli agenti di sistema. Il pacchetto Windows x64 è prodotto e provato; nessuna parità dichiarata con macOS o Linux, che restano rinviati.

Verifiche pianificate: `pianificato: Generare il pacchetto dalla ricetta CI su Windows e confrontare il manifesto.`; `pianificato: Provare installazione, avvio e disinstallazione preservando i dati dell'utente e registrando i prerequisiti mancanti.`.

Commit: `feat(f32pkg.2): Pacchetti installabili per piattaforma e architettura`. Registrare stato e prove sotto `progress.json` → `steps` → `F32PKG` → `units` → `F32PKG.2`.

### F32PKG.3 · Percorso da installazione pulita al primo hop

Prove cronometrate su sistemi Windows 11, macOS e Ubuntu puliti, con elenco dei fallimenti e documentazione dei prerequisiti effettivi.

Stima: 2–6 ore. File: `packaging/`, `docs/verification/`, `README.md`.

Accettazione: Installare, completare il controllo iniziale, aprire una sessione e arrivare al primo hop soltanto tramite operazioni esplicite; riportare tempi e impedimenti. Mancanza di rete, account o componenti non deve essere mascherata come successo; l'app resta utilizzabile nelle capacita disponibili e non invia traffico non previsto.

Verifiche pianificate: `pianificato: Eseguire X1/X2/AT-12 e acquisire soltanto prove sintetiche, distinguendo verifiche riuscite e piattaforme non eseguite.`; `pianificato: Aggiornare istruzioni e record del passo padre; confermare completamento soltanto dopo tutte le unita e i gate di rilascio pertinenti.`.

Commit: `feat(f32pkg.3): Percorso da installazione pulita al primo hop`. Registrare stato e prove sotto `progress.json` → `steps` → `F32PKG` → `units` → `F32PKG.3`.

<a id="f33"></a>
## F33 · Aggiornamenti facoltativi e nuova verifica delle dipendenze vincolate

Aggiornamenti dell’app abilitati dall’utente, ripristino della versione precedente degli agenti propri e aggiornamento del sidecar soltanto tramite l’app.

**Ambito:** v1. **Dipendenze:** F32PKG, C15. **Stima:** 5–14 ore.

**File/componenti:** `crates/update/`; `apps/desktop/src/settings/`; `docs/verification/`.

**Accettazione osservabile:**

- Non produrre traffico di aggiornamento o catalogo prima dell’abilitazione.
- Rifiutare firme o hash non validi; mantenere recuperabili app e agente precedenti.
- Il sidecar non deve aggiornarsi da solo; non modificare impostazioni o comandi di aggiornamento dei programmi di sistema.
- Rieseguire le verifiche API, licenza e capacità quando cambia una revisione di dipendenza vincolata.

**Verifiche del passo:** `planned: No update/catalog traffic until enabled.`; `planned: Invalid signature/hash rejected; previous app/agent remains recoverable.`; `planned: Sidecar never self-updates; system program settings/update commands untouched.`; `planned: Pinned dependency revision changes rerun API/license/capability gates.`.

**Gate collegati, da registrare separatamente:** X4, X6, D16.

**Requisiti coperti dal piano:** FRD-0017/R10, FRD-0017/R11, FRD-0017/R12, FRD-0017/R13, FRD-0017/R14, FRD-0017/R15, FRD-0017/R16, FRD-0017/R17, FRD-0017/R18.

**Commit proposto:** `feat(f33): Aggiornamenti facoltativi e nuova verifica delle dipendenze vincolate`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/update/, apps/desktop/src/settings/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="fe01"></a>
## FE01 · Valutazione post-V1 del gateway code-mode

Record di valutazione dopo il riferimento T1 delle dimensioni e la soglia scelta dall’utente; nessuna consegna del gateway nella V1.

**Ambito:** evaluation. **Dipendenze:** F20. **Stima:** 4–16 ore.

**File/componenti:** `verify/T3/`; `docs/verification/`.

**Accettazione osservabile:**

- Leggere le fonti primarie delle affermazioni dei fornitori.
- Realizzare il prototipo soltanto se dimensioni e soglia lo giustificano; misurare riuscita dei compiti, sicurezza e latenza.
- Conservare le chiamate dirette e la coesistenza tra ricerca degli strumenti e percorsi programmatici dove necessaria.

**Verifiche del passo:** `planned: Read primary sources for vendor claims.`; `planned: Prototype only if footprint/threshold warrants; task success/security/latency measured.`; `planned: Direct calls remain available and tool search/programmatic paths coexist where needed.`.

**Gate collegati, da registrare separatamente:** T1, T2, T3.

**Requisiti coperti dal piano:** FRD-0013/R37.

**Commit proposto:** `docs(fe01): Valutazione post-V1 del gateway code-mode`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/T3/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="c38"></a>
## C38 · Valutare le varianti aperte di roundtable e il rinvio del gateway

Soltanto misure e decisioni registrate, senza introdurre gateway, pianificazione o conversazione libera non richiesti.

**Ambito:** evaluation. **Dipendenze:** C32, C33, FE01. **Stima:** 8–16 ore.

**File/componenti:** `docs/evaluations/code-mode.md`; `docs/evaluations/roundtable-variants.md`.

**Accettazione osservabile:**

- La misura iniziale del costo delle definizioni degli strumenti precede la valutazione del gateway; le fonti primarie sono controllate.
- Il gateway richiede una soglia di risparmio stabilita dall'utente senza calo dei risultati prima dell'implementazione.
- La variante libera tra due agenti e la pianificazione restano decisioni esplicite dell'utente.

**Verifiche del passo:** `T1/T2/T3 measurement report`; `record unresolved decisions`.

**Gate collegati, da registrare separatamente:** T1, T2, T3.

**Requisiti coperti dal piano:** FRD-0007/R38, FRD-0007/R41, FRD-0007/R42.

**Commit proposto:** `docs(c38): Valutare le varianti aperte di roundtable e il rinvio del gateway`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire docs/evaluations/code-mode.md, docs/evaluations/roundtable-variants.md. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="fe02"></a>
## FE02 · Valutazione successiva dell’escalation tra modelli

Valutazione facoltativa di turni con un modello economico seguiti, su decisione del giudice, dal riavvio a partire dal compito originale.

**Ambito:** evaluation. **Dipendenze:** F25, F20. **Stima:** 4–16 ore.

**File/componenti:** `verify/routing-cascade/`; `docs/verification/`.

**Accettazione osservabile:**

- Non cambiare mai modello durante una conversazione.
- Misurare costo dei compiti conclusi e rilavorazione rispetto al modello predefinito fisso.
- Non dichiarare una consegna del primo incremento o della V1 sulla base di una valutazione non eseguita.

**Verifiche del passo:** `planned: Never changes model mid-conversation.`; `planned: Measure finished-task cost and rework against fixed-default baseline.`; `planned: No first-build or V1 delivery claim from an unrun evaluation.`.

**Gate collegati, da registrare separatamente:** MR-3, MR4.

**Requisiti coperti dal piano:** FRD-0015/R26.

**Commit proposto:** `docs(fe02): Valutazione successiva dell’escalation tra modelli`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/routing-cascade/, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qset-claude"></a>
## QSET-claude · Provare set e tool reali per claude

Due istanze sintetiche claude dimostrano i set effettivamente caricati, assenza di contaminazione e costo delle definizioni tool, con revisione del capability catalog. Usare set generici scelti dall utente; nessun pacchetto Daodan e prerequisito.

**Ambito:** v1. **Dipendenze:** C14, C15. **Stima:** 3–6 ore.

**File/componenti:** `crates/adapter/src/claude/**`; `verify/D19/claude/**`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D4 e rinviato con D01; qui si misura solo l'inventario attivo.

- Due set diversi producono esattamente plugin/skill/MCP previsti e restano distinti.
- I file della home system non vengono cambiati oltre quanto autorizzato; le prove usano box e dati sintetici.
- Versione e costo tool registrati; se manca una seam si mantiene la capacità indisponibile con prova del rifiuto.

**Verifiche del passo:** `fixture live D19 per claude`; `misura D4 definizioni tool per claude`.

**Gate collegati, da registrare separatamente:** D19 per set generici.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(qset-claude): Provare set e tool reali per claude`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/claude/**, verify/D19/claude/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qset-codex"></a>
## QSET-codex · Provare set e tool reali per codex

Due istanze sintetiche codex dimostrano i set effettivamente caricati, assenza di contaminazione e costo delle definizioni tool, con revisione del capability catalog. Usare set generici scelti dall utente; nessun pacchetto Daodan e prerequisito.

**Ambito:** v1. **Dipendenze:** C14, C16. **Stima:** 3–6 ore.

**File/componenti:** `crates/adapter/src/codex/**`; `verify/D19/codex/**`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D4 e rinviato con D01; qui si misura solo l'inventario attivo.

- Due set diversi producono esattamente plugin/skill/MCP previsti e restano distinti.
- I file della home system non vengono cambiati oltre quanto autorizzato; le prove usano box e dati sintetici.
- Versione e costo tool registrati; se manca una seam si mantiene la capacità indisponibile con prova del rifiuto.

**Verifiche del passo:** `fixture live D19 per codex`; `misura D4 definizioni tool per codex`.

**Gate collegati, da registrare separatamente:** D19 per set generici.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(qset-codex): Provare set e tool reali per codex`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/codex/**, verify/D19/codex/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qa-codex-windows"></a>
## QA-codex-windows · Certificare terminale codex su windows

Evidenza riproducibile per un vero codex su windows, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** v1. **Dipendenze:** C09, C16. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/windows/codex/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di codex registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Ripetere i fault RES applicabili all agente Windows; nessun passaggio di test estranei qualifica la resilienza.

**Verifiche del passo:** `matrice P1–P20 codex/windows applicabile`; `fixture TUI reale codex con account/test sintetici autorizzati`; `planned: RES1-RES4 applicabili per agente/versione Windows con sessioni sintetiche, sibling sano, budget fissati e misure memoria/CPU/latenza`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7, RES1-RES4 (agente reale).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R63.

**Commit proposto:** `feat(qa-codex-windows): Certificare terminale codex su windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/platform/windows/codex/**, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qset-opencode"></a>
## QSET-opencode · Provare set e tool reali per opencode

Due istanze sintetiche opencode dimostrano i set effettivamente caricati, assenza di contaminazione e costo delle definizioni tool, con revisione del capability catalog. Usare set generici scelti dall utente; nessun pacchetto Daodan e prerequisito.

**Ambito:** v1. **Dipendenze:** C14, C17. **Stima:** 3–6 ore.

**File/componenti:** `crates/adapter/src/opencode/**`; `verify/D19/opencode/**`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D4 e rinviato con D01; qui si misura solo l'inventario attivo.

- Due set diversi producono esattamente plugin/skill/MCP previsti e restano distinti.
- I file della home system non vengono cambiati oltre quanto autorizzato; le prove usano box e dati sintetici.
- Versione e costo tool registrati; se manca una seam si mantiene la capacità indisponibile con prova del rifiuto.

**Verifiche del passo:** `fixture live D19 per opencode`; `misura D4 definizioni tool per opencode`.

**Gate collegati, da registrare separatamente:** D19 per set generici.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(qset-opencode): Provare set e tool reali per opencode`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/opencode/**, verify/D19/opencode/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qa-opencode-windows"></a>
## QA-opencode-windows · Certificare terminale opencode su windows

Evidenza riproducibile per un vero opencode su windows, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** v1. **Dipendenze:** C09, C17. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/windows/opencode/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di opencode registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Ripetere i fault RES applicabili all agente Windows; nessun passaggio di test estranei qualifica la resilienza.

**Verifiche del passo:** `matrice P1–P20 opencode/windows applicabile`; `fixture TUI reale opencode con account/test sintetici autorizzati`; `planned: RES1-RES4 applicabili per agente/versione Windows con sessioni sintetiche, sibling sano, budget fissati e misure memoria/CPU/latenza`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7, RES1-RES4 (agente reale).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R63.

**Commit proposto:** `feat(qa-opencode-windows): Certificare terminale opencode su windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/platform/windows/opencode/**, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qset-pi"></a>
## QSET-pi · Provare set e tool reali per pi

Due istanze sintetiche pi dimostrano i set effettivamente caricati, assenza di contaminazione e costo delle definizioni tool, con revisione del capability catalog. Usare set generici scelti dall utente; nessun pacchetto Daodan e prerequisito.

**Ambito:** v1. **Dipendenze:** C14, C18. **Stima:** 3–6 ore.

**File/componenti:** `crates/adapter/src/pi/**`; `verify/D19/pi/**`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D4 e rinviato con D01; qui si misura solo l'inventario attivo.

- Due set diversi producono esattamente plugin/skill/MCP previsti e restano distinti.
- I file della home system non vengono cambiati oltre quanto autorizzato; le prove usano box e dati sintetici.
- Versione e costo tool registrati; se manca una seam si mantiene la capacità indisponibile con prova del rifiuto.

**Verifiche del passo:** `fixture live D19 per pi`; `misura D4 definizioni tool per pi`.

**Gate collegati, da registrare separatamente:** D19 per set generici.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(qset-pi): Provare set e tool reali per pi`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/pi/**, verify/D19/pi/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qa-pi-windows"></a>
## QA-pi-windows · Certificare terminale pi su windows

Evidenza riproducibile per un vero pi su windows, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** v1. **Dipendenze:** C09, C18. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/windows/pi/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di pi registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Ripetere i fault RES applicabili all agente Windows; nessun passaggio di test estranei qualifica la resilienza.

**Verifiche del passo:** `matrice P1–P20 pi/windows applicabile`; `fixture TUI reale pi con account/test sintetici autorizzati`; `planned: RES1-RES4 applicabili per agente/versione Windows con sessioni sintetiche, sibling sano, budget fissati e misure memoria/CPU/latenza`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7, RES1-RES4 (agente reale).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R63.

**Commit proposto:** `feat(qa-pi-windows): Certificare terminale pi su windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/platform/windows/pi/**, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qset-hermes"></a>
## QSET-hermes · Provare set e tool reali per hermes

Due istanze sintetiche hermes dimostrano i set effettivamente caricati, assenza di contaminazione e costo delle definizioni tool, con revisione del capability catalog. Usare set generici scelti dall utente; nessun pacchetto Daodan e prerequisito.

**Ambito:** v1. **Dipendenze:** C14, C19. **Stima:** 3–6 ore.

**File/componenti:** `crates/adapter/src/hermes/**`; `verify/D19/hermes/**`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D4 e rinviato con D01; qui si misura solo l'inventario attivo.

- Due set diversi producono esattamente plugin/skill/MCP previsti e restano distinti.
- I file della home system non vengono cambiati oltre quanto autorizzato; le prove usano box e dati sintetici.
- Versione e costo tool registrati; se manca una seam si mantiene la capacità indisponibile con prova del rifiuto.

**Verifiche del passo:** `fixture live D19 per hermes`; `misura D4 definizioni tool per hermes`.

**Gate collegati, da registrare separatamente:** D19 per set generici.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(qset-hermes): Provare set e tool reali per hermes`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/hermes/**, verify/D19/hermes/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qa-hermes-windows"></a>
## QA-hermes-windows · Certificare terminale hermes su windows

Evidenza riproducibile per un vero hermes su windows, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** v1. **Dipendenze:** C09, C19. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/windows/hermes/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di hermes registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Ripetere i fault RES applicabili all agente Windows; nessun passaggio di test estranei qualifica la resilienza.

**Verifiche del passo:** `matrice P1–P20 hermes/windows applicabile`; `fixture TUI reale hermes con account/test sintetici autorizzati`; `planned: RES1-RES4 applicabili per agente/versione Windows con sessioni sintetiche, sibling sano, budget fissati e misure memoria/CPU/latenza`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7, RES1-RES4 (agente reale).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R63.

**Commit proposto:** `feat(qa-hermes-windows): Certificare terminale hermes su windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/platform/windows/hermes/**, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qset-deepseek"></a>
## QSET-deepseek · Provare set e tool reali per deepseek

Due istanze sintetiche deepseek dimostrano i set effettivamente caricati, assenza di contaminazione e costo delle definizioni tool, con revisione del capability catalog. Usare set generici scelti dall utente; nessun pacchetto Daodan e prerequisito.

**Ambito:** v1. **Dipendenze:** C14, C20. **Stima:** 3–6 ore.

**File/componenti:** `crates/adapter/src/deepseek/**`; `verify/D19/deepseek/**`; `docs/verification/`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** Il gate D4 e rinviato con D01; qui si misura solo l'inventario attivo.

- Due set diversi producono esattamente plugin/skill/MCP previsti e restano distinti.
- I file della home system non vengono cambiati oltre quanto autorizzato; le prove usano box e dati sintetici.
- Versione e costo tool registrati; se manca una seam si mantiene la capacità indisponibile con prova del rifiuto.

**Verifiche del passo:** `fixture live D19 per deepseek`; `misura D4 definizioni tool per deepseek`.

**Gate collegati, da registrare separatamente:** D19 per set generici.

**Requisiti coperti dal piano:** FRD-0002/R21, FRD-0002/R22, FRD-0002/R23, FRD-0002/R24, FRD-0002/R25, FRD-0002/R26, FRD-0002/R27, FRD-0002/R28, FRD-0002/R29.

**Commit proposto:** `feat(qset-deepseek): Provare set e tool reali per deepseek`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire crates/adapter/src/deepseek/**, verify/D19/deepseek/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="qa-deepseek-windows"></a>
## QA-deepseek-windows · Certificare il pannello ACP DeepSeek su windows

Prova del vero client ACP Shinatama/DeepSeek su windows: messaggi, tool, permessi e chiusura del processo, senza inventare una TUI o un PTY DeepSeek.

**Ambito:** v1. **Dipendenze:** C09, C20. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/windows/deepseek/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

- Handshake ACP, stream messaggi/tool e richieste di permesso correlati per versione e request ID.
- Focus/modal, Unicode, backpressure e cleanup del processo verificati; nessuna risposta a una richiesta scaduta.
- La promozione riguarda questa combinazione; restano indisponibili le capacità non provate.
- Ripetere i fault RES applicabili all agente Windows; nessun passaggio di test estranei qualifica la resilienza.

**Verifiche del passo:** `fixture DS1–DS15/ACP applicabili su windows`; `request/permission correlation e crash cleanup`; `planned: RES1-RES4 applicabili per agente/versione Windows con sessioni sintetiche, sibling sano, budget fissati e misure memoria/CPU/latenza`.

**Gate collegati, da registrare separatamente:** DS1–DS15 applicabili, P14, U3, U7, RES1-RES4 (agente reale).

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58, FRD-0001/R59, FRD-0001/R63.

**Commit proposto:** `feat(qa-deepseek-windows): Certificare il pannello ACP DeepSeek su windows`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire verify/platform/windows/deepseek/**, docs/verification/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="f32sign"></a>
## F32SIGN · Artefatti di rilascio firmati e notarizzazione

Firma e checksum dei rilasci Windows, con distribuzione delle risorse grafiche risolta separatamente.

**Ambito:** v1. **Dipendenze:** F32PKG, QSET-claude, QA-claude-windows, QSET-codex, QA-codex-windows, QSET-opencode, QA-opencode-windows, QSET-pi, QA-pi-windows, QSET-hermes, QA-hermes-windows, QSET-deepseek, QA-deepseek-windows. **Stima:** 4–12 ore.

**File/componenti:** `packaging/`; `.github/workflows/`; `docs/verification/`.

**Accettazione osservabile:**

- Verificare la firma Windows su un sistema pulito.
- Le chiavi private di firma devono restare fuori dai sorgenti e dai log.
- Pubblicare checksum e avvisi delle dipendenze; dichiarare nelle note di rilascio che è supportato soltanto Windows.

**Verifiche del passo:** `planned: Verify Windows signature on a clean machine.`; `planned: Private signing secrets stay outside source and logs.`; `planned: Publish checksums/dependency notices; Windows-only support named in release notes.`.

**Gate collegati, da registrare separatamente:** X1, X2, X3, W9, AT-12.

**Requisiti coperti dal piano:** FRD-0017/R01, FRD-0017/R02, FRD-0017/R05, FRD-0017/R14, FRD-0017/R15.

**Commit proposto:** `feat(f32sign): Artefatti di rilascio firmati e notarizzazione`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire packaging/, .github/workflows/. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="v1-acceptance"></a>
## V1-acceptance · Verificare e distribuire il candidato dello scope attivo

Candidato verificato dello scope attualmente autorizzato, con esclusione esplicita dell integrazione Daodan rinviata dall utente.

**Ambito:** v1. **Dipendenze:** B00, C01, C02, C03, B02, C04, C05, C06, C07, C08, C09, C10, C11, C12, C13, C14, C15, C16, C17, C18, C19, C20, C21, C22, C23, C24, C25, C26, C27, C28, C29, C30, C31, C32, C33, C36, C37, C40, C41, C42, C43, C44, FS03, F02, F03P, F03H, F04, F05, F06, F07, F08, F09, F10, F11-windows, F12, F13, F14-codex, F14-opencode, F14-pi, F14-hermes, F14-deepseek, F15, F16, F17, F18, F19, F20, F21, F22, F23-codex, F23-opencode, F23-pi, F23-hermes, F23-deepseek, F24, F25, F26, F27, F28, F29, F30, F31, F32CI, F32PKG, F32SIGN, F33, B01, QSET-claude, QA-claude-windows, QSET-codex, QA-codex-windows, QSET-opencode, QA-opencode-windows, QSET-pi, QA-pi-windows, QSET-hermes, QA-hermes-windows, QSET-deepseek, QA-deepseek-windows, C09R. **Stima:** 8–16 ore.

**File/componenti:** `docs/verification/release-v1/**`; `docs/releases/**`; `release-manifest.json`.

**Accettazione osservabile:**

**Parti rinviate nello scope corrente:** La V1 storica completa resta distinta dal candidato senza Daodan: riportare il rinvio nelle note, senza falsi completamenti.

- Ogni requisito obbligatorio dello scope attivo e ogni gate pertinente ha evidenza su Windows; i requisiti Daodan e le parti macOS e Linux rinviati restano tracciati e non sono dichiarati consegnati.
- Le indisponibilità sono ammesse solo dove le specifiche prevedono esplicitamente il gating; una release S1/S2 parziale mantiene un nome e un ambito distinti dalla V1 completa.
- Installazione e primo avvio su macchina pulita, riavvio/crash/recupero e confini di privacy verificati; firma/pubblicazione usa credenziali e destinazione effettivamente autorizzate.
- Qualificare almeno un ora con budget congelati, piu finestre, pressione variabile e guasti ripetuti; registrare crescita residua e latenza.

**Verifiche del passo:** `matrice di accettazione FRD completa e AT-1–AT-12`; `ripetizione riproducibile dei gate di release su Windows`; `verifica checksum/firma del candidato e note di rilascio`; `planned: soak Windows RES4 di almeno un ora, qualifiche per agente, metriche pre/dopo recupero e verifica storia salvata`.

**Gate collegati, da registrare separatamente:** AT-1–AT-12, X1–X6, tutti i G0/G1 applicabili alla release, RES1, RES2, RES3, RES4.

**Requisiti coperti dal piano:** FRD-0001/R59, FRD-0001/R60, FRD-0001/R61, FRD-0001/R62, FRD-0001/R63.

**Commit proposto:** `feat(v1-acceptance): Chiudere la matrice V1 e pubblicare il candidato completo`.

**Prima azione alla ripresa:** Leggere la scheda e le dipendenze; aprire docs/verification/release-v1/**, docs/releases/**. Preparare la prima prova di accettazione prima di modificare il comportamento.

<a id="d01"></a>
## D01 · Integrare pacchetti e marketplace Daodan

Installazione Daodan, dipendenze marketplace e pacchetti per harness. Rimane separata dal controllo di avvio e dai set generici.

**Ambito:** deferred. **Dipendenze:** C14, C15. **Stima:** 4–8 ore.

**File/componenti:** `crates/daodan/**`; `verify/D19/**`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su richiesta esplicita dell utente del 2026-11-02: ignorare per adesso l integrazione con Daodan. Riprendere solo su nuova indicazione.

- Solo dopo riattivazione: installare e verificare manifest/pacchetti senza modifiche implicite alle home system.
- Versioni e fixture live aggiornate prima di dichiarare i pacchetti supportati.

**Verifiche del passo:** `pianificato dopo riattivazione: fixture sintetiche della relativa integrazione e verifica dei gate applicabili`.

**Gate collegati, da registrare separatamente:** FRD-0008 e gate Daodan pertinenti: rinviati, D4, D20.

**Requisiti coperti dal piano:** FRD-0008/R01, FRD-0008/R02, FRD-0008/R03, FRD-0008/R04, FRD-0008/R05, FRD-0008/R06, FRD-0008/R07, FRD-0008/R08, FRD-0008/R09, FRD-0008/R10, FRD-0008/R41, FRD-0008/R42.

**Commit proposto:** `feat(d01): Integrare pacchetti e marketplace Daodan`.

**Prima azione alla ripresa:** Attendere una nuova richiesta esplicita di riprendere Daodan; poi rivalutare versioni, dipendenze e scheda.

<a id="d02"></a>
## D02 · Collegare i partecipanti bare-model della roundtable a Daodan

Bridge moe/peer-review per modelli senza harness; la roundtable basata su harness resta nel ciclo attivo.

**Ambito:** deferred. **Dipendenze:** C33, D01. **Stima:** 4–8 ore.

**File/componenti:** `crates/roundtable/src/daodan.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su richiesta esplicita dell utente del 2026-11-02: ignorare per adesso l integrazione con Daodan. Riprendere solo su nuova indicazione.

- Nessuna reimplementazione sostitutiva di Ultra Nerd; credenziali e traffico seguono i confini previsti.

**Verifiche del passo:** `pianificato dopo riattivazione: fixture sintetiche della relativa integrazione e verifica dei gate applicabili`.

**Gate collegati, da registrare separatamente:** FRD-0008 e gate Daodan pertinenti: rinviati, R1.

**Requisiti coperti dal piano:** FRD-0007/R29.

**Commit proposto:** `feat(d02): Collegare i partecipanti bare-model della roundtable a Daodan`.

**Prima azione alla ripresa:** Attendere una nuova richiesta esplicita di riprendere Daodan; poi rivalutare versioni, dipendenze e scheda.

<a id="d03"></a>
## D03 · Mostrare gli avvisi dei task Daodan nel widget

Segnalazioni di domande Daodan senza risposta; gli avvisi generici degli agenti restano indipendenti.

**Ambito:** deferred. **Dipendenze:** C34, F30. **Stima:** 4–8 ore.

**File/componenti:** `crates/widget/src/daodan_alerts.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su richiesta esplicita dell utente del 2026-11-02: ignorare per adesso l integrazione con Daodan. Riprendere solo su nuova indicazione.

- Gli avvisi derivano da task realmente letti; nessuna modifica dello stato Daodan.

**Verifiche del passo:** `pianificato dopo riattivazione: fixture sintetiche della relativa integrazione e verifica dei gate applicabili`.

**Gate collegati, da registrare separatamente:** FRD-0008 e gate Daodan pertinenti: rinviati.

**Requisiti coperti dal piano:** FRD-0010/R32.

**Commit proposto:** `feat(d03): Mostrare gli avvisi dei task Daodan nel widget`.

**Prima azione alla ripresa:** Attendere una nuova richiesta esplicita di riprendere Daodan; poi rivalutare versioni, dipendenze e scheda.

<a id="d04"></a>
## D04 · Riprendere le seam Daodan in companion, router e misure token

Contesto task/fase, eventuali tabelle thread_task, signals tramite task_router.py e misure di sovrapposizione con la memoria Daodan.

**Ambito:** deferred. **Dipendenze:** D01, F10, F25, F29. **Stima:** 4–8 ore.

**File/componenti:** `crates/companion/**`; `crates/router/**`; `verify/TE/**`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su richiesta esplicita dell utente del 2026-11-02: ignorare per adesso l integrazione con Daodan. Riprendere solo su nuova indicazione.

- I moduli generici continuano a funzionare con Daodan assente.
- Nessun percorso esegue componenti Daodan finche questa integrazione resta rinviata.

**Verifiche del passo:** `pianificato dopo riattivazione: fixture sintetiche della relativa integrazione e verifica dei gate applicabili`.

**Gate collegati, da registrare separatamente:** FRD-0008 e gate Daodan pertinenti: rinviati.

**Requisiti coperti dal piano:** FRD-0016/R19.

**Commit proposto:** `feat(d04): Riprendere le seam Daodan in companion, router e misure token`.

**Prima azione alla ripresa:** Attendere una nuova richiesta esplicita di riprendere Daodan; poi rivalutare versioni, dipendenze e scheda.

<a id="qa-claude-macos"></a>
## QA-claude-macos · Certificare terminale claude su macos

Evidenza riproducibile per un vero claude su macos, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C15. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/macos/claude/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di claude registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Completare SC16 con cinque istanze e storie lunghe per un ora sulla piattaforma: nessuna connessione rifiutata, profilare i ritardi; usare solo fixture/account e budget autorizzati.

**Verifiche del passo:** `matrice P1–P20 claude/macos applicabile`; `fixture TUI reale claude con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-claude-macos): Certificare terminale claude su macos`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-claude-linux"></a>
## QA-claude-linux · Certificare terminale claude su linux

Evidenza riproducibile per un vero claude su linux, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C15. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/linux/claude/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di claude registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.
- Completare SC16 con cinque istanze e storie lunghe per un ora sulla piattaforma: nessuna connessione rifiutata, profilare i ritardi; usare solo fixture/account e budget autorizzati.

**Verifiche del passo:** `matrice P1–P20 claude/linux applicabile`; `fixture TUI reale claude con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-claude-linux): Certificare terminale claude su linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="f11-macos"></a>
## F11-macos · Confinamento del companion a livello di sistema operativo: macOS

Lista positiva dei permessi di lettura e scrittura e collegamenti alle credenziali per macOS.

**Ambito:** deferred. **Dipendenze:** F10. **Stima:** 6–20 ore.

**File/componenti:** `crates/confinement/macos/`; `docs/verification/`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Consentire scritture soltanto nello scratch, comprese le restrizioni allo stato locale dell’harness.
- Worktree, programma e collegamenti originali alle credenziali devono essere in sola lettura; Shinatama non deve aprire o copiare le credenziali.
- Altre directory personali, progetti e database devono essere inaccessibili; mostrare il badge confinato soltanto dopo la verifica.

**Verifiche del passo:** `planned: Only scratch writable, including harness-local state restrictions.`; `planned: Worktree/program and original credential bindings read-only; Shinatama never opens/copies credentials.`; `planned: Other homes/projects/DB inaccessible; confined badge withheld until pass.`.

**Gate collegati, da registrare separatamente:** CS-2, CS-9.

**Requisiti coperti dal piano:** FRD-0012/R66, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R94.

**Commit proposto:** `feat(f11-macos): Confinamento del companion a livello di sistema operativo: macOS`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### F11-macos.1 · Verifica del meccanismo e inventario dei permessi su macOS

Interfaccia di confinamento disattivata per impostazione iniziale, sonda delle capacita del sistema e manifesto dei percorsi del companion; valutare profilo sandbox e regole esplicite per processo e discendenti, senza modificare i file originali.

Stima: 2–6 ore. File: `crates/confinement/macos/`, `docs/verification/`.

Accettazione: L'app e il companion in anteprima continuano ad avviarsi con il badge di sola politica harness; nessuna esecuzione viene dichiarata confinata. L'inventario distingue scratch scrivibile, worktree/programmi e credenziali originali in sola lettura, e percorsi sempre vietati; non legge o copia credenziali reali.

Verifiche pianificate: `pianificato: Compilare e avviare la versione corrente con il nuovo modulo disattivato.`; `pianificato: Eseguire la sonda su file sintetici e registrare prerequisiti, limiti e disponibilita del meccanismo.`.

Commit: `feat(f11-macos.1): Verifica del meccanismo e inventario dei permessi su macOS`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-macos` → `units` → `F11-macos.1`.

### F11-macos.2 · Applicazione dei permessi positivi al processo companion su macOS

Avvio confinato sperimentale con lo scratch come unica destinazione scrivibile e i soli percorsi autorizzati leggibili; collegamento alle credenziali originali soltanto in lettura, se il sistema lo consente.

Stima: 2–8 ore. File: `crates/confinement/macos/`, `docs/verification/`.

Accettazione: Il processo e i discendenti non possono scrivere nel worktree, nella sua .git o nello stato locale del progetto, ne leggere altre home, progetti o il database. Un meccanismo mancante o incapace di vincolare le credenziali non viene aggirato; la capacita confined resta disabilitata e l'anteprima rimane utilizzabile.

Verifiche pianificate: `pianificato: Provare letture e scritture consentite e vietate con file canarino, collegamenti simbolici e percorsi alternativi.`; `pianificato: Verificare che nessuna API Shinatama apra o copi credenziali e che non siano necessarie modifiche permanenti ai permessi dei file originali.`.

Commit: `feat(f11-macos.2): Applicazione dei permessi positivi al processo companion su macOS`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-macos` → `units` → `F11-macos.2`.

### F11-macos.3 · Integrazione del ciclo di vita e prove prima del badge confined su macOS

Integrazione nell'adattatore companion con pulizia del processo e dello scratch, controllo degli hook di progetto e registrazione CS-2/CS-9; badge confined soltanto per la combinazione harness/versione/piattaforma provata.

Stima: 2–6 ore. File: `crates/confinement/macos/`, `docs/verification/`.

Accettazione: Chiusura, annullamento e fallimento di avvio non lasciano scritture esterne, processi gestiti o canali al worker; la normale UI continua a funzionare. Aggiornamento delle credenziali o dipendenze non compatibili rendono l'host non idoneo al confinamento; ogni caso non provato conserva il badge di anteprima esplicito.

Verifiche pianificate: `pianificato: Eseguire i casi negativi CS-2/CS-9 e il confronto dei byte/input del worker con e senza companion.`; `pianificato: Rieseguire avvio/chiusura e prova di confinamento della versione installata, allegando record; segnare il padre completo soltanto dopo tutte le unita e le prove richieste.`.

Commit: `feat(f11-macos.3): Integrazione del ciclo di vita e prove prima del badge confined su macOS`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-macos` → `units` → `F11-macos.3`.

<a id="f11-linux"></a>
## F11-linux · Confinamento del companion a livello di sistema operativo: Linux

Lista positiva dei permessi di lettura e scrittura e collegamenti alle credenziali per Linux.

**Ambito:** deferred. **Dipendenze:** F10. **Stima:** 6–20 ore.

**File/componenti:** `crates/confinement/linux/`; `docs/verification/`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Consentire scritture soltanto nello scratch, comprese le restrizioni allo stato locale dell’harness.
- Worktree, programma e collegamenti originali alle credenziali devono essere in sola lettura; Shinatama non deve aprire o copiare le credenziali.
- Altre directory personali, progetti e database devono essere inaccessibili; mostrare il badge confinato soltanto dopo la verifica.

**Verifiche del passo:** `planned: Only scratch writable, including harness-local state restrictions.`; `planned: Worktree/program and original credential bindings read-only; Shinatama never opens/copies credentials.`; `planned: Other homes/projects/DB inaccessible; confined badge withheld until pass.`.

**Gate collegati, da registrare separatamente:** CS-2, CS-9.

**Requisiti coperti dal piano:** FRD-0012/R66, FRD-0012/R67, FRD-0012/R68, FRD-0012/R69, FRD-0012/R70, FRD-0012/R71, FRD-0012/R72, FRD-0012/R73, FRD-0012/R94.

**Commit proposto:** `feat(f11-linux): Confinamento del companion a livello di sistema operativo: Linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

**Unita di commit sequenziali (ognuna lascia l’app avviabile):**

### F11-linux.1 · Verifica del meccanismo e inventario dei permessi su Linux

Interfaccia di confinamento disattivata per impostazione iniziale, sonda delle capacita del sistema e manifesto dei percorsi del companion; valutare Landlock o bubblewrap, con verifica delle capacita disponibili e dei vincoli del sistema.

Stima: 2–6 ore. File: `crates/confinement/linux/`, `docs/verification/`.

Accettazione: L'app e il companion in anteprima continuano ad avviarsi con il badge di sola politica harness; nessuna esecuzione viene dichiarata confinata. L'inventario distingue scratch scrivibile, worktree/programmi e credenziali originali in sola lettura, e percorsi sempre vietati; non legge o copia credenziali reali.

Verifiche pianificate: `pianificato: Compilare e avviare la versione corrente con il nuovo modulo disattivato.`; `pianificato: Eseguire la sonda su file sintetici e registrare prerequisiti, limiti e disponibilita del meccanismo.`.

Commit: `feat(f11-linux.1): Verifica del meccanismo e inventario dei permessi su Linux`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-linux` → `units` → `F11-linux.1`.

### F11-linux.2 · Applicazione dei permessi positivi al processo companion su Linux

Avvio confinato sperimentale con lo scratch come unica destinazione scrivibile e i soli percorsi autorizzati leggibili; collegamento alle credenziali originali soltanto in lettura, se il sistema lo consente.

Stima: 2–8 ore. File: `crates/confinement/linux/`, `docs/verification/`.

Accettazione: Il processo e i discendenti non possono scrivere nel worktree, nella sua .git o nello stato locale del progetto, ne leggere altre home, progetti o il database. Un meccanismo mancante o incapace di vincolare le credenziali non viene aggirato; la capacita confined resta disabilitata e l'anteprima rimane utilizzabile.

Verifiche pianificate: `pianificato: Provare letture e scritture consentite e vietate con file canarino, collegamenti simbolici e percorsi alternativi.`; `pianificato: Verificare che nessuna API Shinatama apra o copi credenziali e che non siano necessarie modifiche permanenti ai permessi dei file originali.`.

Commit: `feat(f11-linux.2): Applicazione dei permessi positivi al processo companion su Linux`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-linux` → `units` → `F11-linux.2`.

### F11-linux.3 · Integrazione del ciclo di vita e prove prima del badge confined su Linux

Integrazione nell'adattatore companion con pulizia del processo e dello scratch, controllo degli hook di progetto e registrazione CS-2/CS-9; badge confined soltanto per la combinazione harness/versione/piattaforma provata.

Stima: 2–6 ore. File: `crates/confinement/linux/`, `docs/verification/`.

Accettazione: Chiusura, annullamento e fallimento di avvio non lasciano scritture esterne, processi gestiti o canali al worker; la normale UI continua a funzionare. Aggiornamento delle credenziali o dipendenze non compatibili rendono l'host non idoneo al confinamento; ogni caso non provato conserva il badge di anteprima esplicito.

Verifiche pianificate: `pianificato: Eseguire i casi negativi CS-2/CS-9 e il confronto dei byte/input del worker con e senza companion.`; `pianificato: Rieseguire avvio/chiusura e prova di confinamento della versione installata, allegando record; segnare il padre completo soltanto dopo tutte le unita e le prove richieste.`.

Commit: `feat(f11-linux.3): Integrazione del ciclo di vita e prove prima del badge confined su Linux`. Registrare stato e prove sotto `progress.json` → `steps` → `F11-linux` → `units` → `F11-linux.3`.

<a id="qa-codex-macos"></a>
## QA-codex-macos · Certificare terminale codex su macos

Evidenza riproducibile per un vero codex su macos, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C16. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/macos/codex/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di codex registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 codex/macos applicabile`; `fixture TUI reale codex con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-codex-macos): Certificare terminale codex su macos`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-codex-linux"></a>
## QA-codex-linux · Certificare terminale codex su linux

Evidenza riproducibile per un vero codex su linux, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C16. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/linux/codex/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di codex registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 codex/linux applicabile`; `fixture TUI reale codex con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-codex-linux): Certificare terminale codex su linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-opencode-macos"></a>
## QA-opencode-macos · Certificare terminale opencode su macos

Evidenza riproducibile per un vero opencode su macos, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C17. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/macos/opencode/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di opencode registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 opencode/macos applicabile`; `fixture TUI reale opencode con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-opencode-macos): Certificare terminale opencode su macos`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-opencode-linux"></a>
## QA-opencode-linux · Certificare terminale opencode su linux

Evidenza riproducibile per un vero opencode su linux, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C17. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/linux/opencode/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di opencode registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 opencode/linux applicabile`; `fixture TUI reale opencode con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-opencode-linux): Certificare terminale opencode su linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-pi-macos"></a>
## QA-pi-macos · Certificare terminale pi su macos

Evidenza riproducibile per un vero pi su macos, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C18. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/macos/pi/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di pi registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 pi/macos applicabile`; `fixture TUI reale pi con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-pi-macos): Certificare terminale pi su macos`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-pi-linux"></a>
## QA-pi-linux · Certificare terminale pi su linux

Evidenza riproducibile per un vero pi su linux, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C18. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/linux/pi/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di pi registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 pi/linux applicabile`; `fixture TUI reale pi con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-pi-linux): Certificare terminale pi su linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-hermes-macos"></a>
## QA-hermes-macos · Certificare terminale hermes su macos

Evidenza riproducibile per un vero hermes su macos, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C19. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/macos/hermes/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di hermes registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 hermes/macos applicabile`; `fixture TUI reale hermes con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-hermes-macos): Certificare terminale hermes su macos`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-hermes-linux"></a>
## QA-hermes-linux · Certificare terminale hermes su linux

Evidenza riproducibile per un vero hermes su linux, separata dalle altre combinazioni e dalle sole fixture del meccanismo.

**Ambito:** deferred. **Dipendenze:** C09, C19. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/linux/hermes/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Input, resize, focus/modal, clipboard/escape negativi e stream reale verificati per la versione di hermes registrata.
- Riavvio, chiusura, crash e processo figlio non perdono sessioni o lasciano processi non dichiarati; misure e limiti registrati.
- Nessuna equivalenza implicita fra sistemi: la capacità è promossa solo per questa combinazione se passa; altrimenti versione o funzionalità resta indisponibile.

**Verifiche del passo:** `matrice P1–P20 hermes/linux applicabile`; `fixture TUI reale hermes con account/test sintetici autorizzati`.

**Gate collegati, da registrare separatamente:** P1–P20 per combinazione, TL2, TL5, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-hermes-linux): Certificare terminale hermes su linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-deepseek-macos"></a>
## QA-deepseek-macos · Certificare il pannello ACP DeepSeek su macos

Prova del vero client ACP Shinatama/DeepSeek su macos: messaggi, tool, permessi e chiusura del processo, senza inventare una TUI o un PTY DeepSeek.

**Ambito:** deferred. **Dipendenze:** C09, C20. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/macos/deepseek/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Handshake ACP, stream messaggi/tool e richieste di permesso correlati per versione e request ID.
- Focus/modal, Unicode, backpressure e cleanup del processo verificati; nessuna risposta a una richiesta scaduta.
- La promozione riguarda questa combinazione; restano indisponibili le capacità non provate.

**Verifiche del passo:** `fixture DS1–DS15/ACP applicabili su macos`; `request/permission correlation e crash cleanup`.

**Gate collegati, da registrare separatamente:** DS1–DS15 applicabili, P14, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-deepseek-macos): Certificare il pannello ACP DeepSeek su macos`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

<a id="qa-deepseek-linux"></a>
## QA-deepseek-linux · Certificare il pannello ACP DeepSeek su linux

Prova del vero client ACP Shinatama/DeepSeek su linux: messaggi, tool, permessi e chiusura del processo, senza inventare una TUI o un PTY DeepSeek.

**Ambito:** deferred. **Dipendenze:** C09, C20. **Stima:** 4–8 ore.

**File/componenti:** `verify/platform/linux/deepseek/**`; `docs/verification/`; `crates/adapter/src/capabilities.rs`.

**Accettazione osservabile:**

**Rinvio:** Rinviato su decisione esplicita dell utente del 2026-11-02: solo Windows e supportato ufficialmente e non ci sono macchine macOS o Linux. Riprendere solo su nuova indicazione.

- Handshake ACP, stream messaggi/tool e richieste di permesso correlati per versione e request ID.
- Focus/modal, Unicode, backpressure e cleanup del processo verificati; nessuna risposta a una richiesta scaduta.
- La promozione riguarda questa combinazione; restano indisponibili le capacità non provate.

**Verifiche del passo:** `fixture DS1–DS15/ACP applicabili su linux`; `request/permission correlation e crash cleanup`.

**Gate collegati, da registrare separatamente:** DS1–DS15 applicabili, P14, U3, U7.

**Requisiti coperti dal piano:** FRD-0001/R09, FRD-0001/R10, FRD-0001/R13, FRD-0001/R15, FRD-0001/R16, FRD-0001/R17, FRD-0001/R18, FRD-0001/R19, FRD-0001/R20, FRD-0001/R21, FRD-0001/R33, FRD-0001/R50, FRD-0001/R51, FRD-0001/R52, FRD-0001/R53, FRD-0001/R54, FRD-0001/R55, FRD-0001/R57, FRD-0001/R58.

**Commit proposto:** `feat(qa-deepseek-linux): Certificare il pannello ACP DeepSeek su linux`.

**Prima azione alla ripresa:** Attendere una nuova indicazione esplicita di riprendere questo sistema; poi aggiornare ADR-0047 e provare TL1.

## Copertura e confini

Ogni requisito Markdown FRD-0001–FRD-0017 è assegnato ad almeno un passo, compresi invarianti, esclusioni e prove. Assegnazione non equivale a implementazione.
Gli incrementi `evaluation` possono concludersi con una decisione o una capacità ancora indisponibile; non dichiarano consegnata la funzionalità. Gli incrementi `post-v1` sono esclusi dal prossimo passo automatico.
Hop V1 solo seed/resume; nessuna scrittura di sessioni native in V1, nessun Git Diff dedicato, Grok o fallback browser automatico. Widget ultimo nella propria sequenza V1; browser/roundtable/TokenEconomizer non bloccano S1 e S2.
Tauri è la shell V1 definitiva (ADR-0005 v2, ADR-0047 v2); TL1-proof la prova su Windows, unico sistema supportato ufficialmente. Le revisioni dipendenti di ADR-0002 (Mermaid), ADR-0040 e ADR-0048 restano da approvare.

I rinvii espliciti dell’utente prevalgono sullo scope storico: le parti rinviate restano tracciate, escluse dalla selezione automatica e dall’accettazione del ciclo attivo.
