# Avviare Shinatama

1. Fai doppio clic su `Shinatama.cmd` nella cartella del progetto.
2. Premi **New session** e scegli l'harness. Il progetto selezionato è già impostato; puoi cambiarlo.
3. Premi **Open**. Se Claude richiede l'accesso, completalo nel suo terminale.

Preparazione e servizio locale partono automaticamente con **Open**. Non serve
eseguire un controllo separato. Sono disponibili Claude Code, Codex, OpenCode e PowerShell.

**Split** divide i pannelli; **Find** cerca nel
testo del terminale. La scelta del tema e il layout vengono salvati automaticamente.
Chiudendo la finestra con terminali aperti puoi annullare oppure confermare l'arresto.

**Shinatama alerts** → **Prova viso e suono** mostra il viso in sovraimpressione.
Nello stesso pannello puoi disattivare il suono o gli avvisi. La comparsa lascia
il focus nella finestra in uso; **Apri sessione** torna al terminale associato.
Questa prima versione riceve i segnali di attenzione nativi del terminale e la
fine del processo mentre Shinatama è aperta. La disponibilità delle notifiche
dipende dall'harness; la domanda completa si legge e si risponde nel terminale.

Il progetto contiene i thread, caricati automaticamente quando lo selezioni.
Nel pannello **Threads**, **New session** apre lo stesso dialogo con il progetto
selezionato. Seleziona un thread e premi **Rename** per cambiarne il nome:
Invio salva, Esc annulla. **Reload threads** rilegge quelli salvati. Il bordo destro
ridimensiona il pannello; la freccia lo comprime. Dimensioni e stato sono salvati.
Il workspace può mostrare viste di progetti diversi: cambiare il progetto nel
pannello Threads non cambia la disposizione. Chiudere una vista conserva il thread.
La ripresa delle conversazioni Claude è collegata al comando **Resume**, quando disponibile,
ma la prova completa con una conversazione reale resta aperta. Le finestre flottanti restano da implementare.

Se dopo un arresto dell'app un thread risulta ancora aperto senza terminale,
**Recover session** tenta di recuperarlo. Quando riesce, premi **Resume**.
Se la fine della sessione precedente non può essere verificata, il recupero
viene rifiutato e la conversazione salvata viene conservata.

Per consultare un archivio esistente, apri **Sessions** e **Add a read-only archive**.
Scegli Claude Code, Codex o Pi e indica la cartella archivio dell'agente; premi
**Add archive**, poi **Reload sessions**. Le pagine contengono fino a 50 sessioni;
il filtro cerca nella pagina corrente. La consultazione non modifica l'archivio.
Seleziona un ID e premi **Open detail** per leggere la conversazione. Il dettaglio
attuale accetta archivi fino a 16 MiB; quelli più grandi vengono rifiutati prima
della lettura. **Close session detail** torna all'elenco.

Claude conserva i propri strumenti e le richieste di permesso nel terminale.
Autenticazione, copertura dei provider, ripresa per ogni harness, ricerca globale
e hop restano distinti: non sono certificati dalla sola apertura della TUI.
