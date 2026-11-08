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
| 0.2 | Rigenero il pacchetto dal HEAD pulito: `scripts/package-local.ps1` (include C12.5 e ADR-0051) | ~10 min |
| 0.3 | Verifica del pacchetto: `scripts/package-local.ps1 -Verify -Package <id>` | ~5 min |
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
| C25 | crash forzato → riavvio → Recover | **VERDE 2026-11-08** (crash vero, recupero via certificato ADR-0051); pronta per la tua accettazione |
| C13 | `desktop-smoke.mjs` completo | parcheggiata: arriva fino ad aprire Claude, poi il governatore risorse rifiuta; un solo run in più a macchina ferma |
| C46 | `attention-smoke.mjs` | **VERDE 2026-11-08**; pronta per la tua accettazione |
| F02 | prova UI Claude + crash forzato | pronta per la tua accettazione del padre |

Nota ambiente: i proof vanno lanciati con `TEMP` fuori da qualsiasi repo Git
(altrimenti "Storage unavailable"). Fatto nel run attuale.

## Wave 2 — Uso quotidiano (solo se servono)

| Voce | Cosa | Quando |
| --- | --- | --- |
| C15.4 | autenticazione/provider reale per harness | se vuoi usare gli account veri nell'app |
| C22/C23 | archivio e dettaglio conversazione | se serve navigare le vecchie sessioni |
| C24 | ricerca globale persistente | se serve oltre la ricerca per progetto |

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
