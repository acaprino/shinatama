# B02 — Cronologia progetti persistente

Data: 2026-11-02. Piattaforma eseguita: Windows 10.0.26200 x86_64,
Rust 1.99.1, Node 24.14.1, Tauri 2.12.5. Incremento locale, non chiusura
completa dei gate PM1/PM7/PM8/S9 o del project manager.

## Comportamento consegnato

Il selettore ricorda dieci cartelle recenti, ordinate per ultima selezione,
con filtro incrementale. Valida e canonicalizza una cartella esistente prima
di salvarla. Ripristina l'ultima cartella disponibile senza avviare processi.
Le cartelle mancanti restano visibili come indisponibili. La selezione modifica
la directory dei terminali futuri; quelli aperti conservano directory e identità.

La configurazione è `projects.json` nella directory configurazione applicativa
fornita da Tauri; `SHINATAMA_HOME` assoluto permette una posizione esplicita.
FRD-0004/R22 consente questa collocazione. Non è il database autorevole di
sessioni/thread; C01–C03 ne preparano l'integrazione con SQLite.

Formato v1, massimo 256 KiB; scrittura atomica con file temporaneo nella stessa
directory, lock OS e verifica dei byte originali. Un'altra finestra può aggiornare
il file: la cronologia viene riletta; una scrittura basata su dati vecchi è rifiutata.
File corrotti, troppo grandi o di versione futura sono preservati. Nessuna
creazione/scrittura viene eseguita nella cartella progetto scelta.

## Prove eseguite

- `cargo test -p shinatama-projects`: nove test passati su Windows, comprendenti
  persistenza/limite/ordine, alias, invalidi, cartella scomparsa, corruzione/versione
  e conflitto fra store. Le prove Unix sui permessi non sono state eseguite qui.
- `npm run build`: controllo tipi e bundle passati.
- `npm run test:projects`: tre avvii dell'eseguibile, stato e cronologia recuperati,
  zero terminali automatici, terminale precedente invariato, filtro e cartella
  mancante verificati. Configurazione e cartelle esclusivamente sintetiche.
  Se l'ultima cartella sparisce, resta selezionata come indisponibile: nessun
  passaggio silenzioso a un'altra cartella; aprire un terminale richiede una nuova
  scelta esplicita. Durante il salvataggio non si può annullare/editare la selezione.
- `npm run test:desktop`: regressione passata dei comandi borderless, terminali,
  spostamento/nascondi/ripristina, output continuo e chiusura confermata.

Script: `apps/desktop/tests/projects-smoke.mjs`. Screenshot locale ignorato:
`.prototype-evidence/projects-picker.png`. Nessun agente, modello o account reale
è stato usato. I test non scrivono nelle preferenze personali dell'app.

## Limiti espliciti

Dieci cartelle recenti totali per ora; il requisito delle dieci cartelle esterne
alle radici configurate si completa in C04. Root multipli, F3/picker nativo,
default root e creazione progetti sono pianificati. ACL Windows owner-only,
case sensitivity per volume macOS, scansioni di root e gate su tre OS rimangono
aperti. Il filtro attuale riguarda la cronologia, non un indice globale dei progetti.
