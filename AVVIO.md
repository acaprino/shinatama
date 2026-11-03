# Avviare Shinatama

1. Fai doppio clic su `Shinatama.cmd` nella cartella del progetto.
2. Scegli il progetto con **Project**.
3. Apri **Session index**, premi **Start** e attendi **Ready**.
4. Apri **Agents**, scegli **Claude Code**, premi **Check**, poi **Open agent**.
5. Segui i comandi originali di Claude nel terminale per configurare aspetto e accesso.

**New terminal** apre PowerShell. **Split** divide i pannelli; **Find** cerca nel
testo del terminale. La scelta del tema e il layout vengono salvati automaticamente.
Chiudendo la finestra con terminali aperti puoi annullare oppure confermare l'arresto.

Nel pannello **Threads**, **New thread** salva un nuovo thread nel progetto senza
avviare un agente. **Reload threads** rilegge quelli salvati. Il bordo destro
ridimensiona il pannello; la freccia lo comprime. Dimensioni e stato sono salvati.
La ripresa nativa dai thread resta da implementare.

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
