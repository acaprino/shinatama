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
| C13 | `desktop-smoke.mjs` completo, una volta, con commit libero sufficiente | unità tutte committate; prova riparata in `77934cc`, run finale mancante |
| C25 | `forced-crash-recovery-smoke.mjs` (minima: 1 crash, 1 riavvio, 1 rifiuto) | comportamenti provati per-run; run unico mancante |
| C46 | `attention-smoke.mjs` (PTY + attenzione UI) | mai eseguita: serve memoria |
| F02 | prova UI Claude guardata + crash forzato nativo | bloccata da headroom |

Regola: un run ciascuna, quando la macchina è ferma; nessun retry automatico.

## B. Backlog di capacità (funzioni, non blocchi di prodotto)

| Voce | Cosa resta | Quando vale la pena |
| --- | --- | --- |
| C14 | set generici e stati di capability qualificati | quando serve un nuovo set |
| C15.4 | autenticazione/provider reale per harness | quando si vuole usare l'account vero |
| C16 | binding nativo Codex oltre l'Open già funzionante | Richiede il prossimo harness installato |
| C17 | capability OpenCode residue | se serve di più dell'avvio verificato |
| C22/C23 | archivio e dettaglio conversazione oltre il flusso funzionante | se serve navigazione avanzata |
| C24 | ricerca globale persistente | se serve oltre la ricerca per progetto |
| C12.5 | decisione targetRatio/growthWindow (precedenza o deprecazione) | decisione di prodotto, non tecnica |
| ADR-0051 | implementazione del certificato di terminazione (recupero crash pre-ricevuta, ripristino capacità ledger) | dopo approvazione dell'ADR |

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
