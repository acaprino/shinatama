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
Aggiornamenti e notifiche di completamento non scadono dopo pochi secondi:
puoi chiuderli con la **X**, con **Esc** quando il widget ha il focus, oppure
aprendo la sessione. Gli avvisi vengono aggiornati quando cambia la situazione.
Questa prima versione riceve i segnali di attenzione nativi del terminale e la
fine del processo mentre Shinatama è aperta. La disponibilità delle notifiche
dipende dall'harness; la domanda completa si legge e si risponde nel terminale.

Quando la memoria disponibile scende, il widget mostra un avviso e indica il
terminale che consuma di più tra quelli misurati, con il suo consumo in MiB.
Shinatama non chiude né sospende i terminali già aperti per la memoria. Può
rifiutare l'apertura di un nuovo terminale quando manca il piccolo margine
necessario all'avvio: non richiede gigabyte liberi per aprire una shell.
L'avviso non prende il focus; **Apri sessione** porta al terminale indicato.
I dettagli sono nella [politica di memoria](docs/notes/2026-11-12-interactive-memory.md).

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

## Novità (novembre 2026)

- **Projects** (barra in alto): progetti preferiti con la stella, branch e file modificati,
  ultimo thread con **Resume last**, **New session**, **New in worktree…** (crea un worktree
  Git solo dopo la tua conferma).
- **Files**: albero e ricerca dei file del progetto. Aprire un file non lo esegue mai.
  Markdown in Source, Preview, Split o **Visual** (titoli, paragrafi e codice; il resto resta
  com'è). **Save** non sovrascrive mai in silenzio un file cambiato da un agente.
- **Instances**: ogni terminale aperto con lo stato osservato e la sua fonte (gli stati in
  corso sono incerti finché non sono collegati gli hook nativi), cosa sta facendo, token dai
  record dell'agente, avvisi di collisione sui file, hop recenti. Unica azione: **Focus**.
- Nel dettaglio di una sessione: **Export…** (STX, Mukade JSON, Markdown; i segreti sono
  oscurati, il ragionamento non è mai esportato) e **Hop…** (anteprima esatta di cosa riceve
  l'altro agente, poi **Hop** apre la destinazione e le consegna il riepilogo).
- Riga di comando in `cli\shinatama.exe`: `sessions`, `find`, `export`, `forget`,
  `check-state` (integrità e ricerca di credenziali nei dati di Shinatama, senza mai stamparle)
  e `mcp` (gli strumenti di messaggistica tra istanze).

## Marionette

Ogni sessione di Claude Code, Codex, Pi e OpenCode aperta da Shinatama parte
con **Marionette**, un esempio costruito con [Muro](https://github.com/acaprino/muro):
include anche le regole di lavoro e di comunicazione di Ponytail e i-have-adhd.
Non c'è nulla da installare e la configurazione dei tuoi agenti non cambia.

A ogni avvio Shinatama controlla l'ultima release pubblicata su
[acaprino/marionette](https://github.com/acaprino/marionette) e, se è
nuova, la scarica in `marionette\` dentro la cartella dati, ne verifica lo
SHA-256 e la usa per le sessioni aperte da quel momento. Senza rete resta in uso l'ultima
copia scaricata; al primo avvio senza rete le sessioni partono senza Marionette.
Prima di ogni sessione Shinatama verifica i file dell'adapter di quell'agente:
se uno è stato modificato, quella sessione parte senza Marionette e il
controllo successivo ripara la copia. Per disattivarla imposta la variabile
d'ambiente `SHINATAMA_MARIONETTE=off` prima di aprire Shinatama.

In Codex le istruzioni di sessione arrivano dagli hook di Marionette, che
richiedono la fiducia: apri `/hooks`, accorda la fiducia e avvia un nuovo
thread. Le skill di Marionette usano `python` (e per alcuni helper `node`)
dal `PATH`.

### Formule e diagrammi in Claude Code

Solo per Claude Code, insieme a Marionette, Shinatama carica la mod
**shn-render** (`resources/claude-mods`): mostra le formule LaTeX e i diagrammi
Mermaid semplici come testo Unicode. Cambia solo ciò che si vede nel terminale:
la risposta salvata resta identica, e `ctrl+o` mostra l'originale. Copiando una
formula dal pannello si copia la versione Unicode. Codex, Pi e OpenCode
disegnano già formule e diagrammi da soli e non ricevono nulla.

La mod ha una verifica sua (`mods.lock.json`): se un file non torna, viene
tolta solo lei e Marionette resta attiva. Parte solo con le
versioni di Claude Code con cui è stata provata (oggi 2.1.322); con le altre la
sessione parte senza. Per disattivarla imposta `SHINATAMA_CLAUDE_MODS=off`.
Per ora è presente nella build locale (`Shinatama.cmd`), non ancora nel
pacchetto portabile.

### Fuori da Shinatama

Per le CLI che apri da PowerShell per conto tuo, usa la copia che Shinatama ha
scaricato. Parti dalla cartella dati di Shinatama (nel pacchetto portabile è
`data\` accanto a `Shinatama.exe`).

```powershell
$tag = (Get-Content './marionette/current.json' | ConvertFrom-Json).tag
$adapters = (Resolve-Path "./marionette/releases/$tag/marionette/adapters").Path
```

- Claude Code: `claude --plugin-dir "$adapters/claude-code"`.
- Codex: `codex plugin marketplace add "$(Resolve-Path "./marionette/releases/$tag")"`, poi
  `codex plugin add marionette@shinatama`.
- Pi: `pi -e "$adapters/pi/extensions/policy.js" --skill "$adapters/pi/skills"`.
- OpenCode e gli altri host: segui `INSTALL.md` nella cartella dell'adapter.
