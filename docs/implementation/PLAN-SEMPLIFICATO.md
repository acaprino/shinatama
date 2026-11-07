# Piano semplificato (2026-11-06)

Sostituisce, come vista unica, le 51 voci sparse del register. Il register
(`progress.json`) resta la fonte dei dettagli; qui c'è solo ciò che conta.

## Regola generale

- Una prova per comportamento. Se un comportamento è già provato, non si riprova
  senza una modifica di codice che lo tocchi.
- Niente cicli automatici di run. Una finestra di macchina quieta = un run; dopo
  due fallimenti ambientali la voce va in parcheggio, non si ripete.
- Preferire test unitari/engine alle smoke UI per tutto ciò che non richiede
  ConPTY, WebView o crash reali.
- Un parent si chiude solo con l'accettazione esplicita del proprietario.

## A. Backlog di verifica (solo a macchina libera — non sono blocchi)

| Voce | Prova | Stato |
| --- | --- | --- |
| C13 | `desktop-smoke.mjs` completo, una volta, con commit libero sufficiente | test riallineato il 2026-11-07 (usava "Session index" e "Agents", non più montati dopo `dffb269`: ora New session → Claude Code → Open); non ancora verde: il governatore era in "reduced activity" (commit libero 4,1–4,6 GiB, uscita a 5,17) |
| C25 | `forced-crash-recovery-smoke.mjs` (minima: 1 crash, 1 riavvio, 1 recupero) | **crash vero recuperato dal certificato ADR-0051** (nel database della fixture: attivazione chiusa rev 2, journal vuoto); asserzione del test aggiornata e retry in lettura; run verde ancora da avere con commit libero ≥ 8 GiB |
| C46 | `attention-smoke.mjs` (PTY + attenzione UI) | test corretto (campo progetto nascosto); non ancora verde: "New terminals are waiting for healthy resource measurements" |
| F02 | prova UI Claude guardata + crash forzato nativo | prova UI già superata (resume UI 3/11 + race isolata); il crash forzato è C25; **pronta per la tua accettazione del padre** |

Regola: un run ciascuna, quando la macchina è ferma; nessun retry automatico.
Una voce fallita per memoria resta parcheggiata finché non dici "vai" a macchina libera.

## B. Backlog di capacità (funzioni, non blocchi di prodotto)

| Voce | Cosa resta | Quando vale la pena |
| --- | --- | --- |
| C14 | set generici e stati di capability qualificati | quando serve un nuovo set |
| C15.4 | autenticazione/provider reale per harness | quando si vuole usare l'account vero |
| C16 | binding nativo Codex oltre l'Open già funzionante | Richiede il prossimo harness installato |
| C17 | capability OpenCode residue | se serve di più dell'avvio verificato |
| C22/C23 | archivio e dettaglio conversazione oltre il flusso funzionante | se serve navigazione avanzata |
| C24 | ricerca globale persistente | se serve oltre la ricerca per progetto |
| C12.5 | **FATTO 2026-11-07**: `targetRatio`/`growthWindow` tolti dalla base di prodotto (nessun codice li consuma; i default canonici 0.75/20 sono identici ai valori di prima, comportamento invariato) | chiusa |
| ADR-0051 | **FATTO**: certificato di terminazione, recupero crash pre-ricevuta e ripristino capacità ledger (`85544c3`) | chiusa, resta solo il run reale in C25 |

## C. Piattaforme non coperte (ferme)

QA Claude/Codex/OpenCode/Pi/Hermes/DeepSeek su macOS e Linux: 22 voci deferred.
Non si toccano finché non esiste una macchina di prova.

## Cosa è già chiuso e non va riaperto

Claude/Codex open e resume nativi (stesso UUID, contesto), race duplicate-resume,
supervisore sidecar, storage/threads/search, cleanup con ricevuta + Recover UI,
C12.4 effective-config live (PASS dall'altra sessione), pacchetto portable
`837f4345`. Prodotto invariato da `c9deab5`: i binari 10:42 sono validi.

## Limite di prodotto noto (da ADR-0051)

Un crash pre-ricevuta trattiene 1 GiB nel ledger (`MAX_TOTAL` 4 GiB): finché il
thread non è recuperabile, nuove sessioni possono essere rifiutate con
`Ledger(Capacity)`. Il recupero esiste solo con ADR-0051 approvato.
