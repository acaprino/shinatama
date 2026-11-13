# Skill per sessioni comunicanti

La richiesta e': dalla sessione corrente poter dire «apri un'altra sessione e falle implementare il piano», continuando a supervisionarla attraverso Mukade. **Claude e' il predefinito; una richiesta esplicita di Codex seleziona Codex.** Il modello e' una scelta distinta dall'harness.

Questo documento completa lo [studio di UI, codice e logica](2026-11-13-mukade-collaboration-study.md). La skill di progetto e' in `.claude/skills/mukade-collaborate/`. Il suo formato segue la [documentazione ufficiale di Claude Code](https://code.claude.com/docs/en/skills).

## Comportamento richiesto

| Richiesta | Selezione | Modalita' |
|---|---|---|
| «Apri un'altra sessione e falle implementare questo piano» | `claude_code` | Delega interattiva |
| «Apri Codex e fagli implementare il piano» | `codex` | Delega interattiva |
| «Apri Claude per discutere le alternative» | `claude_code` | Dialogo, senza modifiche al codice |
| «Fai rivedere il diff a Codex» | `codex` | Revisione, senza implementazione implicita |
| «Osserva quella sessione» | Sessione esistente identificata esattamente | Osservazione ridotta |
| «Se gli dico apri Codex, lo fa?» | Nessun avvio | Spiegazione del comportamento |

Un harness esplicito non disponibile produce un blocco spiegato; non viene sostituito automaticamente. Un nome di modello o profilo come Qwen o Fable non diventa automaticamente un eseguibile.

## Tre livelli distinti

1. **Skill:** interpreta l'intento, prepara il contesto e segue il protocollo.
2. **Mukade:** conserva stanza, membri, messaggi, ricevute e associazioni alle sessioni native.
3. **Host:** apre e controlla il processo reale con cwd, identita', permessi e terminale corretti.

Il codice esaminato espone i tool MCP `chat_*` per il secondo livello. `chat_join` registra un membro; non apre un processo. La CLI `mukade chat launch` apre un harness e resta bloccante nel suo terminale fino all'uscita. Non crea da sola una scheda Shinatama.

## Flusso della delega

La sessione iniziale seleziona harness e modalita', verifica MCP e launcher, prepara un incarico con obiettivo, piano autorizzato, directory, ambito e criteri di accettazione. Un avvio reale riceve un bootstrap breve con stanza, nome del coordinatore e identificativo univoco dell'incarico.

Il nuovo partecipante usa il ruolo Mukade **peer**: puo' chiedere chiarimenti, comunicare progressi e inviare il resoconto. Il ruolo `worker` e' piu' restrittivo: non puo' mandare messaggi liberi e risponde soltanto con `done` o `blocked` a un'istruzione indirizzata da un `lead`. Non e' quindi il predefinito per la delega interattiva.

Prima dell'assegnazione, il coordinatore richiede un messaggio READY correlato all'avvio e verifica l'identita' del partecipante. La sola presenza in `chat_members` non attesta che un processo sia vivo. Il messaggio di ritorno contiene riferimenti ai file, verifiche eseguite, esiti e blocchi. La verifica del risultato resta distinta dalla ricezione del messaggio.

Gli hook Mukade gestiscono gia' lettura e conferma dei messaggi per le sessioni avviate con il relativo adapter. La skill non deve creare un secondo consumatore con `chat_read`/`chat_wait` sulla stessa membership. In modalita' manuale, invece, legge, elabora e conferma una sola volta il batch. Le attese sono limitate e la scadenza non provoca un secondo avvio automatico.

## Stato e collegamento mancante

La skill e' una procedura di progetto, non un nuovo servizio di lancio. L'integrazione nativa richiede ancora un'operazione dell'host, accessibile all'agente, che:

- riusi il launcher di Shinatama e apra una vera scheda;
- componga gli hook Mukade con quelli gia' presenti;
- restituisca un riferimento verificabile all'avvio e alla sessione;
- propaghi stanza, profilo, cwd e bootstrap senza duplicare l'identita' della sessione madre;
- esponga gli errori e lo stato senza rendere la membership una prova di vitalita'.

La skill deve rilevare questa capacita' quando disponibile, senza inventare un tool. Una console esterna e' utilizzabile solo se e' compatibile con il contesto richiesto e il relativo launcher esiste: non equivale a una scheda nativa. Se manca una capacita' necessaria, la skill prepara l'incarico e indica precisamente il blocco.

Per Codex gli hook del kit Mukade devono essere riconosciuti come attendibili dal suo sistema di trust. Non si disabilitano i controlli per aggirare questo passaggio. Un errore di trust non autorizza il ripiego su Claude.

## Confini di verifica

La validazione della skill controlla struttura, riferimenti e applicazione del protocollo a scenari. Non equivale a un test end-to-end Claude → Mukade → Codex. La futura integrazione va provata con un incarico piccolo, READY correlato, domanda di ritorno, modifica isolata, test, timeout senza duplicazione e risultato verificato dalla sessione iniziale.

Verifiche eseguite sulla versione iniziale:

- Validatore della struttura skill: superato; corpo principale di 171 righe.
- Riferimenti locali, metadati, assenza di segnaposto TODO e spazi finali: superati.
- `mukade chat tools` sul binario locale release: 21 tool, coerenti con il contratto documentato; nessun tool di avvio sessione.
- Valutazione indipendente delle istruzioni su quattro scenari: predefinito Claude da una sessione Codex, Codex esplicito senza READY, dialogo con adapter attivo, domanda ipotetica senza avvio. Comportamento coerente; precisati `request_id` e `reason` nel riferimento MCP dopo la revisione.
- Nessun avvio reale di Claude/Codex o modifica al trust durante queste verifiche.

I file sono presenti nel progetto. La registrazione in Git non e' riuscita: la creazione di `.git/index.lock` e' stata rifiutata per permessi. Non e' quindi attestata una distribuzione o sincronizzazione della skill, ne' il suo caricamento in una sessione Claude gia' aperta.
