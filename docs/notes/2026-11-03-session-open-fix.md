# Apertura della sessione con una sola azione

Il flusso prodotto richiede progetto, agente e **Open**. `Controller::open`
prepara internamente il servizio locale e qualifica Claude quando necessario;
non richiede pulsanti Check o Prepare nella UI. `Controller::start` resta
disponibile per le prove diagnostiche esistenti.

La preparazione usa un solo worker posseduto. Il servizio esistente viene
riutilizzato oppure avviato con il suo supervisore attuale; l'attesa del client
è limitata a 30 secondi. Un'attesa scaduta non libera risorse ancora possedute e
non crea un supervisore sostitutivo. Admission, budget e interventi C09R restano
invariati.

L'osservazione della versione e la ricevuta TUI reale vengono riutilizzate nella
stessa esecuzione di Shinatama. Ogni apertura ricontrolla il candidato e il
qualificatore ripete il pin/hash effettivo del binario e della home. Se cambiano
binario, root o profilo, l'apertura esegue una sola nuova qualificazione prima di
un nuovo tentativo. Non vengono fabbricate ricevute e un cleanup incerto conserva
il suo proprietario. Un riavvio dell'app non recupera ricevute serializzate.

## Correzione del tempo della prova versione

Il limite precedente di 10 secondi iniziava prima dell'hash del binario (circa
335 MiB nella build locale), della creazione delle directory private e
dell'admission. Il tempo speso prima del processo riduceva quindi il tempo
disponibile a `claude --version`. Questo difetto è verificabile nel percorso di
codice; il tempo di ogni fase del fallimento segnalato dall'utente non era stato
registrato e non viene ricostruito retroattivamente.

Ora le fasi hanno limiti separati: preparazione 10 secondi, admission 5 secondi,
osservazione del processo realmente avviato 10 secondi e cleanup 1 secondo.
I limiti di memoria, CPU, processi e output non sono aumentati. La diagnostica
conserva i tempi delle tre fasi e il codice d'esito, senza leggere credenziali,
risposte di autenticazione o contenuti delle conversazioni.

La modifica non qualifica altri agenti, login o traffico reale del provider.
La prova nativa del nuovo percorso ha superato l'apertura effettiva il 2026-11-03:
**New session → progetto sintetico → Claude Code → Open**. Il servizio è partito
automaticamente, il thread è stato salvato e la sua vista si è aperta. Nessun
pulsante Check/Prepare è stato usato. Arresto e chiusura dei soli processi della
prova sono riusciti. Ricevuta: `.prototype-evidence/new-session-claude_code.json`.

Anche il flusso PowerShell ha superato la prova con due progetti: un thread per
progetto, entrambe le viste nello stesso workspace, cambio del filtro progetto
senza riavvii o cambio layout e risposta della shell sana. Ricevuta:
`.prototype-evidence/new-session-powershell.json`.

La prova dello stato globale con due connessioni SQLite e quella delle soglie
effettive del desktop sono passate. Il frontend e il desktop sono compilati.
Le prime prove hanno distinto rifiuti reali per memoria, timeout prima del
processo e un problema del selettore della finestra nella prova automatica;
questi fallimenti non sono promossi a successi. La verifica finale riguarda
questo ingresso concreto, non l'intero prodotto o i provider.

## Misura successiva e salute transitoria del servizio

La prova nativa successiva, con `sha2` ottimizzato, ha superato la lettura della
versione: 1.031 ms complessivi, di cui 875 ms di preparazione, 7 ms di admission
e 109 ms di esecuzione. Questa misura riguarda la versione, non il completamento
dell'apertura interattiva. Il nuovo arresto osservato avveniva dopo la versione:
il servizio locale risultava temporaneamente `busy`, con `healthy=false` e
`health_unavailable`.

Prima e dopo la preparazione di trial e avvio, il controller ora attende fino a
5 secondi, a intervalli di 100 ms, che lo stesso proprietario locale torni
qualificato. L'attesa non avvia o sostituisce servizi, non ripete un canary e non
invia input al provider. I controlli interni del qualificatore e i vincoli di
identità/profilo restano attivi. Questa correzione richiede ancora la prova
nativa del percorso completo.

## Calibrazione della pressione globale

Anche l'apertura di PowerShell aveva rifiutato l'avvio con telemetria completa,
4.965.691.392 byte di commit residuo, 8.330.764.288 byte di RAM fisica disponibile,
CPU al 21%, campione del chiamante vecchio di 475 ms e ciclo di campionamento
vecchio di 99 ms. Le soglie globali precedenti (ingresso 85%, uscita 75%) potevano
mantenere il blocco dopo un picco nonostante il margine misurato sufficiente per
un job e la riserva del sistema.

La calibrazione desktop porta l'ingresso al 90% e l'uscita all'87,5%; l'emergenza
resta al 95%. La stabilizzazione di 3 secondi, i controlli su telemetria completa
e recente, i limiti dei job e le riserve in byte restano invariati. Questa soglia
non concede un avvio da sola: ogni lancio continua a richiedere un budget fresco,
RAM e commit sufficienti per i budget già riservati più quello nuovo, oltre a
512 MiB di RAM e 1 GiB di commit per il sistema. Il tetto aggregato iniziale resta
`min(RAM disponibile / 2, commit disponibile / 2, 4 GiB)`; non cresce per adattarsi
a un processo che perde memoria.

Una regressione usa la configurazione desktop effettiva: 40 GiB di commit con
5 GiB liberi e 32 GiB fisici con 8 GiB liberi devono permettere i successivi
controlli di admission dopo la stabilizzazione. Il calo a 1 GiB di commit residuo
deve rinviare nuovi avvii; il recupero a 5 GiB deve consentirli nuovamente solo
dopo la finestra stabile. La prova è sorgente pronta, ancora da eseguire da root.

## Progetto già selezionato: nessuna seconda scelta

`New session` eredita il progetto selezionato nella sidebar o nella barra
superiore. Il dialogo mostra il percorso come contesto e porta subito il focus
sull'harness; campo cartella e progetti recenti compaiono solo senza progetto
oppure premendo esplicitamente `Change project`. Non ricarica i recenti quando
il progetto è già noto. Il percorso scelto nella barra è disponibile subito,
anche prima del completamento asincrono della sidebar. La cartella iniziale
del processo non viene considerata una scelta dell'utente.

Build frontend e Windows riuscite. La prova nativa PowerShell aggiornata ha
verificato prima apertura senza progetto, apertura nel progetto già scelto nella
barra, scelta dalla sidebar fra due progetti senza nuova richiesta di cartella,
focus sull'harness, thread e viste conservati, risposta della shell e chiusura
delle sole sessioni di prova. Ricevuta:
`.prototype-evidence/new-session-powershell.json`. Nessuna ripetizione delle
qualifiche Claude/provider, non coinvolte nella modifica.
