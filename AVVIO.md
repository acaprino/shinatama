# Avviare Shinatama

1. Fai doppio clic su `Shinatama.cmd` nella cartella del progetto.
2. Premi **New session**, scegli la cartella del progetto e l'harness.
3. Premi **Open**. Se Claude richiede l'accesso, completalo nel suo terminale.

Preparazione e servizio locale partono automaticamente con **Open**. Non serve
eseguire un controllo separato. La scelta attuale comprende Claude Code e PowerShell.

**Split** divide i pannelli; **Find** cerca nel
testo del terminale. La scelta del tema e il layout vengono salvati automaticamente.
Chiudendo la finestra con terminali aperti puoi annullare oppure confermare l'arresto.

Il progetto contiene i thread. Nel pannello **Threads**, **New session** apre lo
stesso dialogo con il progetto selezionato. **Reload threads** rilegge quelli salvati. Il bordo destro
ridimensiona il pannello; la freccia lo comprime. Dimensioni e stato sono salvati.
Il workspace può mostrare viste di progetti diversi: cambiare il progetto nel
pannello Threads non cambia la disposizione. Chiudere una vista conserva il thread.
La riapertura nativa di una sessione terminata e le finestre flottanti restano da implementare.

Per consultare un archivio esistente, apri **Sessions** e **Add a read-only archive**.
Scegli Claude Code, Codex o Pi e indica la cartella archivio dell'agente; premi
**Add archive**, poi **Reload sessions**. Le pagine contengono fino a 50 sessioni;
il filtro cerca nella pagina corrente. La consultazione non modifica l'archivio.
Seleziona un ID e premi **Open detail** per leggere la conversazione. Il dettaglio
attuale accetta archivi fino a 16 MiB; quelli più grandi vengono rifiutati prima
della lettura. **Close session detail** torna all'elenco.

Questa build locale abilita il primo avvio Claude in modalità safe con set vuoto.
Autenticazione, copertura dei provider, avvio degli altri agenti, ricerca globale e hop
restano distinti: non sono certificati dalla sola apertura della TUI.
