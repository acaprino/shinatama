# Brief e scambi tra sessioni

## Incarico

Compila con il contesto disponibile, senza inventare decisioni dell'utente.
Mantieni il messaggio breve e rinvia ai documenti accessibili da entrambe le
sessioni. Non includere credenziali, corpi di file o la chat integrale.

```text
TASK <task_id> REV <revision>
Obiettivo: risultato concreto richiesto dall'utente.
Modalita': delega / dialogo / revisione.
Piano autorizzato: percorso assoluto e sezione/revisione, oppure breve sintesi.
Directory effettiva: checkout/worktree e stato iniziale rilevante.
Ambito: file/aree modificabili e vincoli espliciti.
Input: documenti e decisioni necessari; verificare che siano accessibili.
Accettazione: comportamento da ottenere e controlli pertinenti.
Coordinatore: nome verificato del membro destinatario.
Budget: massimo cicli, scadenza e condizioni di arresto concordati.
Blocco: fare una domanda indirizzata prima di assumere una decisione irreversibile.
Risultato: file, verifiche con esito, limiti e prossima decisione necessaria.
```

Un piano approvato per implementazione autorizza le modifiche nel suo ambito.
Un confronto di alternative o una revisione non li autorizza implicitamente.
Se il piano manca, non inventarlo: prepara un incarico di analisi o chiarisci
l'obiettivo, secondo la richiesta effettiva.

## Bootstrap minimo

Consegnare attraverso il launcher come istruzione iniziale della nuova sessione:

```text
Collabori per conto dell'utente al task <task_id>, run <run_id>.
Modalita' e ambito autorizzati: <sintesi>. Directory: <percorso assoluto>.
Stanza: <nome e ID verificati>. Coordinatore: <nome verificato>.
Usa l'identita' Mukade fornita dall'adapter; non creare un secondo membro.
Se non ricevi un'identita' valida o non hai i tool MCP, segnala il blocco.
Invia al coordinatore READY <run_id> <task_id>, con ruolo e cwd effettivi.
Attendi il TASK della revisione corrente prima di modificare file.
Usa chat_send per domande e risultati. Lascia agli hook read/wait/ack.
Non delegare ulteriormente e non ampliare il mandato dell'utente.
```

Questo modello presume un child con adapter configurato dal launcher. Per un
client manuale, il launcher deve fornire un percorso esplicito di join e un solo
consumatore: non usare contemporaneamente le due varianti. La verifica di
READY non sostituisce l'autenticazione del server o l'identita' del processo.

## Messaggi del peer

| Convenzione testuale | Contenuto |
|---|---|
| `READY run task` | Sono collegato; ruolo, directory, identita' verificabili. |
| `ACCEPT task rev` | Ho letto il brief e verificato gli input; ambito compreso. |
| `QUESTION task rev` | Domanda concreta e conseguenza della scelta. |
| `PROGRESS task rev` | Evidenza nuova, passo concluso o rischio emerso. |
| `BLOCKED task rev` | Motivo, verifiche gia' fatte e decisione necessaria. |
| `RESULT task rev` | Modifiche, file, test e relativi esiti, limiti. |
| `SUPERSEDE task rev` | Nuovo brief che sostituisce quello precedente. |

Queste stringhe sono testo in `chat_send`, non operazioni del server. Non sono
ACK di trasporto e non servono a modificare il ruolo Mukade. Il coordinatore
ignora come istruzioni correnti le revisioni superate, conservandole come
storia; chiede conferma del nuovo ambito quando un esecutore stava gia' lavorando.

## Esempi

- Da Codex: «apri un'altra sessione per implementare il filtro» → nuova
  sessione Claude, delega con peer; la scelta non eredita l'harness del padre.
- Da Claude: «apri Codex e fagli implementare il piano» → Codex, peer;
  un fallimento di avvio viene segnalato, senza fallback.
- «Confrontate due soluzioni prima di scrivere codice» → dialogo tra peer;
  proposte motivate e decisione, nessuna modifica durante il confronto.
- «Fammi vedere cosa sta facendo quella sessione» → identificazione esatta e
  osservazione ridotta; distinguere le evidenze dalle inferenze.
