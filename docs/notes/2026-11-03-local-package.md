# Pacchetto Windows locale

Da PowerShell 7.4+ nella cartella del progetto:

```powershell
pwsh -NoProfile -File scripts/package-local.ps1
pwsh -NoProfile -File scripts/package-local.ps1 -Verify -Package 'D:\Projects\shinatama\.verification\portable\UUID-STAMPATO'
```

Il comando usa esclusivamente la build già presente in `target/debug`: desktop,
helper terminale, helper sessioni e helper hook. Il desktop deve avere il sottosistema Windows
GUI. Non compila, scarica, ripristina dipendenze, esegue Node o avvia l'app.
WebView2 deve essere già disponibile su Windows: il pacchetto non lo installa.

Ogni esecuzione crea una nuova cartella UUID sotto `.verification/portable`;
nessun risultato precedente viene sovrascritto o cancellato. I percorsi collegati
o con attraversamento sono rifiutati. Restano fissi i limiti di 1.024 file,
4.096 voci visitate, profondità 32, 512 MiB per file e 1 GiB complessivo.

Il pacchetto contiene i quattro eseguibili, `AVVIO.md`, un `Shinatama.cmd` che avvia
l'eseguibile accanto a sé e le risorse in `resources/sidecar`: i 609 file preparati
fissati dal pin più il file di identità generato (610 file preparati), la provenienza,
Node fissato, la sua identità e il LICENSE completo.
I verificatori statici esistenti sono obbligatori. Ogni file copiato deve avere
gli stessi byte e SHA-256 dell'input; i pin di Node, licenza e sidecar restano attivi.
Nessuna home, configurazione, credenziale o cronologia dell'utente viene inclusa.

`resources/agent-plugins` contiene le copie locali di Ponytail e i-have-adhd,
il catalogo Codex e le licenze MIT. `plugins.lock.json` fissa le revisioni
upstream e i digest dei file. Il packager verifica i digest in copia e lega
il lock alla ricevuta di staging. L'app li attiva a ogni avvio di sessione (Claude `--plugin-dir`; Codex cache privata più `-c`; Pi `-e` e `--skill`; OpenCode shim nella config privata). Vedi [Plugin inclusi](../../AVVIO.md#plugin-inclusi).

`package-manifest.json` registra commit corrente, modalità debug e hash dei file.
Non attribuisce automaticamente quel commit ai binari già compilati. La ricevuta
separata `UUID.receipt.json` resta nella cartella di staging del progetto e lega
il digest del manifest: `-Verify` rifiuta file mancanti, aggiunti o modificati e un
manifest manomesso. Non modificare o eliminare questa ricevuta per verificare lo stage.

La licenza completa del publisher è una verifica statica; non equivale a una
revisione legale o alla firma della distribuzione. Questo è un pacchetto debug
locale, non il superamento dei gate pubblici di rilascio/X3.

## Verifica locale eseguita

Il 2026-11-03 il verificatore ha accettato lo stage
`.verification/portable/6a60b263-9798-4a37-91c0-e955cfce09ee`:
620 file di payload, 220.644.742 byte, HEAD registrato
`7e5a2030a3adbddfa85f8171b421b5766999d847`. Manifest e ricevuta indipendente
sono rimasti invariati durante la verifica. I binari sono copie della build
debug esistente; il manifest non certifica che siano stati prodotti dal commit
registrato.

La prova nativa della copia ha superato il flusso DB-only: progetto sintetico,
creazione e riapertura dello stesso thread, dimensione e compressione della barra
salvate, conflitto su salvataggio obsoleto rifiutato, nessun terminale avviato e
chiusura della sola app della prova. Comando eseguito tramite il wrapper P14:
`SHINATAMA_SMOKE_APP_EXE=<stage>/shinatama-desktop.exe` con
`node apps/desktop/tests/threads-smoke.mjs`. Esito PASS in circa 7 secondi;
ricevuta `.prototype-evidence/package-threads-sidebar.json` e immagine omonima.

Una seconda istanza della copia è stata aperta visibilmente per l'accesso manuale
scelto dall'utente. Non sono stati letti credenziali o risultati di autenticazione.
Questa prova non certifica accesso, traffico dei provider o nuovi avvii sotto
pressione sulle risorse; la politica C09R resta attiva.

La lettura JSON conserva esplicitamente le stringhe al livello principale
mediante `JsonDocument`: PowerShell converte altrimenti `createdUtc` in una data
e rifiuta erroneamente il manifest valido. Il controllo dello schema resta
rigoroso e il lettore continua a supportare PowerShell 7.4.

## Copia aggiornata con apertura unica

Lo stage `.verification/portable/ab7f42ce-5198-4c86-b8b7-c3bdf4c122c0` è stato
creato e verificato dopo il checkpoint
`dffb2695239776fbfbb6d552e725c3c010bd9b0c`: 620 file, 221.661.788 byte.
La verifica interna alla creazione è passata; nessuna seconda scansione identica
è stata aggiunta. I file copiati corrispondono ai byte e alle impronte degli
input verificati. Rimangono vere le distinzioni del manifest fra build debug
esistente, HEAD registrato e revisione dei binari non certificata.

Le prove del percorso utente sulla build Windows sono passate per Claude Code
e per due progetti PowerShell nello stesso workspace, come registrato in
`docs/notes/2026-11-03-session-open-fix.md`. Una copia è stata aperta visibilmente
per l'utente e risultava in esecuzione e responsiva. Nessun contenuto del suo
terminale o accesso è stato letto. Il percorso normale ora è **New session →
progetto e harness → Open**. L'eventuale accesso resta nella TUI originale.
