# Rinomina e caricamento automatico dei thread

Stato: **PASS il 3 novembre 2026**, con una nuova istanza privata dell'app e
una sessione PowerShell sintetica. Ricevuta locale ignorata da Git:
`.prototype-evidence/thread-rename.json`.

Nel pannello Threads, selezionare un thread e premere **Rename** apre un campo
inline. Invio salva, Esc annulla. Il nome salvato sopravvive al caricamento
della pagina; selezionare un progetto carica automaticamente i suoi thread.
Il caricamento riguarda solo metadati: non riapre processi o conversazioni.

Il comando `thread_rename` controlla progetto e revisione, limita il titolo a
256 byte UTF-8, rifiuta stringhe vuote e caratteri di controllo. Il confronto
atomico della revisione impedisce di sovrascrivere un aggiornamento concorrente.
Un conflitto richiede **Reload threads** prima di un nuovo tentativo. Il comando
preserva archivio, identità, associazione nativa e attivazione del thread.

La UI serializza le operazioni e accorpa i cambi di progetto pendenti: al termine
dell'operazione corrente legge solo l'ultimo progetto selezionato. Un errore di
lettura non avvia tentativi automatici. Dopo una rinomina confermata, un eventuale
errore nel ricaricare l'elenco non nasconde il nome già salvato.

## Verifica essenziale

1. `npm run build` — PASS.
2. `cargo build --offline --locked -p shinatama-desktop --features custom-protocol -j1`
   — PASS, 4,80 secondi per l'ultima compilazione incrementale.
3. `verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/thread-rename-smoke.mjs`
   — PASS: annullamento, salvataggio con Invio, conflitto concorrente, rifiuto di
   progetto errato/campi non ammessi/titolo eccessivo, persistenza dopo reload,
   identità e sessione conservate, risposta della stessa shell e cleanup posseduto.

Il confronto della disposizione verifica gruppi, tab, focus e risorse; esclude
solo `lastUsableSize`, cache delle dimensioni misurate che viene aggiornata
legittimamente dopo un reload. Durante la prova è emerso e stato corretto il
mancato caricamento automatico dei thread salvati. Nessuna sessione personale,
credenziale, archivio utente o chiamata a un provider è stata usata.

Questo incremento aggiunge la modifica del nome e la lettura automatica, senza
chiudere l'intero C25: ripresa completa, concorrenza tra processi e recupero dopo
crash conservano i rispettivi requisiti e prove.
