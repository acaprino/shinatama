# Progetto, thread e workspace: correzione vincolante

**Direttiva esplicita dell'utente, 2026-11-03.** Prima delle funzioni avanzate,
rendere utilizzabile il flusso di apertura delle sessioni descritto qui. Questa
nota corregge il comportamento da implementare; non dichiara completata la UI.

## Flusso unico di apertura

1. **New session** apre una sola scelta di **Project** e **Harness**.
2. **Open** crea o apre il thread appartenente a quel progetto e mostra il
   terminale originale dell'harness scelto.
3. Preparazione, controllo dei prerequisiti e avvio necessari avvengono
   internamente in conseguenza dell'Open esplicito. Un errore resta visibile nello
   stesso flusso, con possibilità concreta di riprovare.
4. Se l'harness richiede autenticazione, l'utente la esegue nel suo terminale
   originale. Shinatama non introduce un ulteriore wizard di accesso.

Non richiedere all'utente passaggi di prodotto separati **New agent**, **Check**
o **Session index** per arrivare alla sessione. Queste responsabilità interne non
devono trasformarsi in attività preparatorie obbligatorie della UI. Non aprire
una PowerShell generica al posto del thread dell'harness selezionato.

## Proprietà e disposizione visiva

Il **progetto possiede i thread**. Il thread conserva identità e collegamento
alla propria sessione; scegliere un altro progetto non cambia la sua proprietà.

Il **workspace visivo è indipendente dal progetto**: dispone viste di thread e
può mostrare contemporaneamente thread provenienti da progetti diversi. Il
progetto selezionato serve a scegliere il contenuto o il proprietario di un nuovo
thread, non a sostituire implicitamente tutta la disposizione corrente.

Tiling e finestre floating sono modalità distinte della disposizione delle viste.
Una vista è una presentazione del thread, non il thread stesso. **Chiudere una
vista non elimina il thread** né il lavoro salvato. Eventuali azioni esplicite
sulla sessione o sul processo devono mantenere questa distinzione.

## Scostamenti attuali da correggere

- **C08 e persistenza del layout:** il frontend usa uno scope
  `{ projectId, workspaceId: 'main' }`. Legare la disposizione al cambio progetto
  contrasta con il workspace che contiene thread di più progetti. Integrare nel
  piano il nuovo scope e il recupero dei layout già salvati senza perdita dati.
- **Apertura predefinita:** il percorso corrente può presentare una PowerShell
  senza un thread. Sostituire tale ingresso con **New session → Project + Harness
  → Open**, mantenendo il legame thread/progetto durante avvio e visualizzazione.
- **Preparazione esposta:** ricondurre gli ingressi separati dell'agente al solo
  Open richiesto dall'utente; gli strumenti diagnostici restano un supporto,
  non un prerequisito manuale del normale utilizzo.

**Prossimo incremento funzionante:** un Open crea un thread sotto il progetto
scelto, avvia l'harness scelto e ne mostra il terminale; chiusa la vista, il thread
resta selezionabile. Aggiornare piano e requisiti da questa direttiva prima di
riprendere le funzioni avanzate. I file condivisi del piano e gli FRD non sono
modificati da questa nota: il coordinatore li riallinea con il lavoro UI in corso.

## Riallineamento del piano

C15.3 traccia ora **Aprire una sessione scegliendo progetto e harness**, dopo
C15.2. La precedente unità di login/capacità, ancora da completare, è conservata
come C15.4. Il padre C15 resta pending: il nuovo ingresso non certifica accesso,
provider, resume o altri harness. Daodan resta rinviato.

Per il workspace normale, la nuova impostazione globale `ui.workspace.main.v1`
supera l'accoppiamento fra layout e progetto. Le API e i record legacy per
progetto restano conservati; il collegamento all'avvio e la prova del flusso
globale appartengono al nuovo incremento. C08 conserva il proprio checkpoint
storico con questo limite esplicito. C25.1 è registrato al commit reale
`7e5a2030a3adbddfa85f8171b421b5766999d847`.

La prima prova del nuovo Open è stata fermata dalle risorse prima di avviare
Claude. Le correzioni prevedono attesa della telemetria fino a 8 secondi, timer
del probe separato dalla preparazione e messaggi comprensibili nel dialogo.
Le prove finali del nuovo Open sono passate su Windows: Claude Code viene aperto
dal solo dialogo; due progetti PowerShell conservano un thread ciascuno nello
stesso workspace, senza riavvii al cambio progetto. Anche il salvataggio globale
con due connessioni e la regressione sulle soglie reali sono passati. C15.3 è
`verified`; il padre C15 e i gate account/provider restano aperti. Dettagli:
`docs/notes/2026-11-03-session-open-fix.md`.
