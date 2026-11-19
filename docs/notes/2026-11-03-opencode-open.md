# OpenCode dal progetto selezionato

Incremento C17: OpenCode è disponibile nel dialogo New session accanto a
Claude Code, Codex e PowerShell. Open prepara automaticamente i prerequisiti,
salva il thread del progetto e apre il terminale originale.

L'installazione npm viene risolta leggendo il wrapper riconosciuto, senza
eseguirlo. Parte direttamente `opencode.exe` con `--standalone`: nessuna
opzione che disabiliti le richieste di permesso native. La ricerca conserva
l'ordine PATH, controllando al massimo 128 cartelle entro 10 secondi. Il limite
precedente di 32 non raggiungeva la cartella npm di questo computer.

Programma, versione, home e profilo sono verificati separatamente da Claude e
Codex. L'installer npm può creare più hardlink allo stesso eseguibile: il pin
impedisce modifiche ai byte e al nome scelto. Windows può rimuovere l'altro
alias; il cambiamento del numero di link invalida la preparazione. L'installazione
locale ha tre collegamenti, coperti dalla prova mirata. Configurazioni
e dati privati continuano a rifiutare hardlink.

Database, XDG, cache e configurazione OpenCode sono privati. Il plugin di
ripristino dell'ambiente dei programmi figli è predisposto. Limiti fissi per
la sessione: 1 GiB aggregato, 768 MiB per processo, 8 processi, CPU 10%, coda
4 MiB/512 elementi. Il servizio nativo resta nel job della sua sessione.

Sette controlli engine mirati sono passati, compresi il caso PATH reale e la
modifica di un alias. Compilazione frontend, desktop e helper riuscita.

## Apertura nativa verificata nel servizio comune

La prova sul nuovo servizio harness è **superata con OpenCode 2.2.5** su Windows.
Con dati applicativi privati nuovi e progetti sintetici, il percorso
**New session → progetto → OpenCode → Open** prepara automaticamente il servizio
locale, apre la TUI originale e salva progetto, thread e vista. Non compaiono
controlli preparatori obbligatori e l'apertura non avviene automaticamente.
Arresto della sola sessione posseduta e chiusura del frame sono riusciti.

La ricevuta locale `.prototype-evidence/new-session-opencode.json` riporta
`status: passed` e quattro controlli riusciti: assenza di avvio automatico o
controlli preparatori, singolo Open con persistenza, preparazione interna del
servizio e chiusura posseduta. Il file è ignorato da Git; questa nota conserva
versione, risultati e perimetro della prova. Il servizio comune e la regressione
Codex sono documentati in [harness-services](2026-11-03-harness-services.md).

La richiesta dei percorsi privati ora comprende il file database e i quattro
percorsi XDG, oltre alla home degli strumenti per istanza. Era il requisito
mancante dietro il precedente rifiuto di composizione.

Autenticazione, strumenti reali, traffico provider, esportazione dell'archivio
OpenCode e ripresa nativa restano da dimostrare. Nessun account viene interrogato
o copiato da altre installazioni; la prova non invia risposte di login o prompt
al provider. L'apertura riuscita non completa da sola C17.
