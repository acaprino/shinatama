# Prima versione del widget desktop su Windows

Data: 2026-11-03. Richiesta utente: «Anticipa una prima versione».
Passo: C46. Stato iniziale: implementazione autorizzata, verifiche del prodotto da registrare.

## Risultato da consegnare

Il viso di Shinatama compare sopra il desktop con un breve fumetto quando un
harness richiede attenzione o arriva un aggiornamento significativo. Un suono
breve è disattivabile; il clic esplicito apre la sessione associata. La comparsa
lascia il focus e la digitazione nella finestra che l'utente sta usando.

Questa prima versione è una seconda finestra dell'app Tauri Windows. Usa avvisi
locali transitori, con testo e coda limitati e deduplicazione del suono. Termina
quando si chiude Shinatama. Solo i segnali nativi realmente collegati e provati
possono dichiarare richieste di input e fine turno; l'output del terminale non
diventa una richiesta di approvazione. Le azioni della preview aprono o chiudono
l'avviso: le risposte rapide ai permessi appartengono a F31.

## Rapporto con il widget finale

L'anticipazione è autorizzata dall'utente e modifica soltanto l'ordine di consegna
della prima superficie utilizzabile. F30/F31 mantengono il widget in un processo
separato, client del feed persistente condiviso, funzionamento senza finestra
principale, riconciliazione tra finestre, animazioni definitive e risposte rapide
legate all'identità della richiesta. C46 copre una parte di FRD-0010/R24, R26,
R28, R29 e R30; non soddisfa R25 né promuove i gate W1–W10.

## Checkpoint e dipendenze

B00, B02, C01, C07 e C08 sono `committed` nel checkpoint al momento della
preparazione. C46 non dipende dal futuro feed F02 o dalle estensioni V1 finali.
C12 è già il passo `in_progress`: resta intatto. C46 è registrato `pending`
mentre la preview viene preparata e diventerà `verified` solo dopo prove reali
documentate. Questo stato conserva il vincolo del piano su un solo passo attivo;
non revoca l'autorizzazione esplicita a lavorare sulla preview indipendente.

## Verifiche richieste e limiti

1. Compilare il frontend e il desktop modificati.
2. Provare limiti, duplicati, richieste obsolete e privilegi dei comandi IPC.
3. Provare nell'app Windows comparsa senza focus, clic sulla sessione corretta,
   suono/silenziamento, disattivazione e chiusura con l'app.
4. Registrare separatamente fixture sintetiche e segnali reali, con la copertura
   effettiva per harness. Una dimostrazione non certifica tutti gli harness.

La superficie e implementata e verificata su Windows: build frontend/desktop,
otto test Rust e prova nativa della comparsa senza attivazione, controlli,
preferenze e chiusura con l'app passano. La prova completa ha incontrato il
rifiuto del controllo risorse prima dell'avvio PowerShell; non e stata aggirata.
C46 resta pending per questa prova. Esiti e limiti effettivi:
[record C46](../verification/2026-11-03-C46-widget-preview.md).
