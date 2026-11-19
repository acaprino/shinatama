# Piano a wave (2026-11-08)

Vista unica. `progress.json` resta il registro dei dettagli.
Ogni wave è utilizzabile da sola: si passa alla successiva solo se serve.

## Regole

- Una prova per comportamento, un run per finestra di macchina quieta, niente retry automatici.
- Un parent si chiude solo con la tua accettazione esplicita.
- Windows soltanto. Nessuna prova su macOS/Linux finché non c'è una macchina.

## Wave 0 — Prima versione usabile OGGI (v0.1 portable, Windows)

Obiettivo: avere un pacchetto che apri e usi, con i limiti dichiarati.

| # | Azione | Tempo |
| --- | --- | --- |
| 0.1 | Run C25 e C46 verdi, C13 parcheggiata. **Non blocca** il pacchetto | fatto |
| 0.2 | Pacchetto rigenerato da `5d17449`: `a398b73b-7b57-4036-8583-ec464eb5fa8e` (621 file, 219 MB) | fatto |
| 0.3 | Verifica del pacchetto: `verified` (app non eseguita dallo script) | fatto |
| 0.4 | Tu apri il pacchetto: New session → Claude / Codex / OpenCode → Open | ~5 min |

**Cosa funziona in v0.1** (già provato): apertura e resume nativi Claude e Codex (stesso UUID),
OpenCode avviato, progetti, storage, ricerca per progetto, cleanup con ricevuta + Recover,
recupero crash pre-ricevuta (ADR-0051), supervisore sidecar, overlay attenzione.

**Limiti noti di v0.1** (da dichiarare, non da risolvere oggi):
- sessioni Codex non completano un turno in test (provider sintetico), ma funzionano con account reale;
- C25/C13/C46 come prove complete su macchina ferma potrebbero restare "parcheggiate";
- un crash pre-ricevuta ora è recuperabile con Recover; fino al recupero trattiene 1 GiB nel ledger;
- governatore risorse: con poca memoria libera (< ~5 GiB di commit) rifiuta nuove sessioni.

Uscita Wave 0: pacchetto id registrato in `progress.json` + tuo OK di prova.

## Wave 1 — Fiducia (prove rimaste, solo a macchina libera)

| Voce | Prova | Stato |
| --- | --- | --- |
| C25 | crash forzato → riavvio → Recover | **VERDE 2026-11-08** (crash vero, recupero via certificato ADR-0051); accettata dal proprietario 2026-11-08, chiusa |
| C13 | `desktop-smoke.mjs` completo | parcheggiata: arriva fino ad aprire Claude, poi il governatore risorse rifiuta; un solo run in più a macchina ferma |
| C46 | `attention-smoke.mjs` | **VERDE 2026-11-08**; accettata dal proprietario 2026-11-08, chiusa |
| F02 | prova UI Claude + crash forzato | pronta per la tua accettazione del padre |

Nota ambiente: i proof vanno lanciati con `TEMP` fuori da qualsiasi repo Git
(altrimenti "Storage unavailable"). Fatto nel run attuale.

## Wave 2 — Uso quotidiano (solo se servono)

| Voce | Stato | Serve da te |
| --- | --- | --- |
| C15.4 | pacchetto pronto in `release\`; manca il login reale per harness | apri `release\Shinatama.cmd`, fai login in Claude/Codex/OpenCode, dimmi cosa non va |
| C22/C23 | tutte le unità committate (archivio, dettaglio, navigazione) | accettazione dei padri, dopo che li hai provati in app |
| C24 | **FATTO 2026-11-08**: ricerca globale persistente (pulsante Search): indice FTS5 incrementale in file separato, filtri agente/cartella/data, sorgente cancellata segnalata, forget/disattiva/ricostruisci; prova nativa `session-search-smoke.mjs` verde | provala in app: Search → Index sessions |

Si apre una voce solo dopo l'uso reale della v0.1: i difetti trovati vincono sulle funzioni nuove.

## Wave 2b — Implementato 2026-11-09/10 (da provare in app)

| Voce | Cosa c'è | Cosa resta |
| --- | --- | --- |
| C26 | **Export…** nel dettaglio sessione (STX, Mukade JSON, Markdown, segreti oscurati, scrittura atomica); CLI `sessions/find/export/forget` | export nativo per agente |
| C27-C29 | **Hop…**: anteprima esatta del seed, file privati in `.git/shinatama/handoffs`, hop da archivio con registro `hops.sqlite`, recupero dopo crash, catch-up, UNDO, standby (logica) | un hop vero con il tuo account; close da terminale vivo |
| C30.1 | matrice delle 36 coppie: offerte solo quelle con lettore e avvio qualificati | canary reali per coppia |
| C31-C32 | API messaggi tra istanze, `shinatama mcp` con 4 strumenti e token per istanza | iniezione negli agenti (C32.3) |
| C33 | coordinatore roundtable con limiti, stop e note | partecipanti headless reali, UI |
| C36 | **Projects**: preferiti, branch e modifiche, ultimo thread, nuovo worktree confermato | — |
| C41-C44 | **Files**: albero, ricerca, anteprima Markdown inerte, modalità Visual, salvataggio con controllo versione, immagini | lettore DOCX |
| F04/F06 | **Instances**: stato osservato con la sua fonte, titoli verbatim, pausa riepiloghi | hook nativi per agente |
| F09, F20, F03P | protocollo schede companion; registro token per sessione; `shinatama check-state` | pannello companion; 10 sessioni reali per agente |

## Wave 3 — Capacità

| Voce | Cosa |
| --- | --- |
| C14 | set generici e stati di capability |
| C16 | binding nativo Codex oltre Open |
| C17 | capability OpenCode residue |

## Wave 4 — Piattaforme (ferme)

macOS/Linux: 22 voci deferred. Non si toccano senza una macchina di prova.

## Già chiuso, non riaprire

Claude/Codex open+resume nativi, race duplicate-resume, supervisore sidecar,
storage/threads/search, cleanup con ricevuta + Recover UI, C12.4, C12.5, ADR-0051.
