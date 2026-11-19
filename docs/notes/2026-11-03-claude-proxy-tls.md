# Claude/Bun: prova TLS locale separata

Stato: **prova locale Claude/Bun superata**, 1/1 in 9,27 secondi il 3 novembre
2026. Senza la CA privata Claude termina con un errore di certificato e zero
richieste Messages all'origine. Con la CA soltanto nel figlio, Claude termina
correttamente e riceve il marker da una richiesta Messages attraversata dal
sidecar canonico. La prova è ignorata nella suite ordinaria.
Richiede la lettura e la programmazione esplicita del coordinatore. Non abilita
capacità del prodotto o autorità C11.

La baseline superata è conservata nel commit `885db6d`. Il supporto di avvio,
pin, budget, lettura limitata e cleanup è ora estratto in
`crates/engine/tests/support/claude_native.rs`: la ricetta TLS mantiene argv,
ambiente e condizioni negative/positive. La regressione dopo l'estrazione è
**superata**, 1/1 in 9,18 secondi, nella stessa esecuzione sequenziale della
prova nativa di ripresa. La ricevuta locale è aggiornata al supporto condiviso.

La fixture ha anche una modalità esplicita `native-resume`, distinta dalla prova
TLS: due prompt e due risposte sintetiche diversi, strumenti e MCP disabilitati,
CA privata in entrambe le invocazioni. Il server accetta la seconda richiesta
solo se contiene la prima risposta assistant e il secondo prompt user; registra
soltanto il booleano `resumeHistoryObserved`. Gli eventi autenticati del comando
hook vengono verificati dal test Rust dedicato, non dedotti dal server finto.
La prova dedicata di hook, persistenza e ripresa è superata in 9,78 secondi;
per il suo perimetro headless vedere [la nota](2026-11-03-claude-native-resume.md).

La ricetta TUI C11 resta chiusa: `claude_launch::prepare_trial` fissa argv,
home vuota e CA obbligatoria. Questa prova usa invece il launcher C09 già
contenuto per una ricetta headless distinta; non sostituisce la qualifica del
percorso interattivo e non legge l'accesso manuale dell'utente.

1. Fissa Claude 2.1.317 al digest
   `26b01ec22df2c17ac7f5703aa547d36706e8d58412136ced0344a8ced4d86038`
   riusando il pin eseguibile/antenati di `PreparedClaudeProbe`, senza eseguirlo.
   Node e tutti i file del sidecar devono superare i verificatori esistenti.
2. Crea un archivio C01 UUID nuovo e una CA RSA sintetica valida un giorno con
   `node:crypto`. Nessuna installazione, esportazione o modifica della fiducia Windows.
3. Rust avvia tre owner C09 separati, ciascuno con ammissione e cleanup propri.
   Node serve esclusivamente endpoint sintetici: 256 MiB, 4 processi, CPU 10%.
   Il sidecar conserva i limiti del prodotto: 1 GiB, 768 MiB per processo,
   8 processi, CPU 20%. Claude conserva il limite della prova precedente:
   1 GiB, 768 MiB per processo, 8 processi, CPU 10%. Ogni owner ha code
   256 KiB/4; l'aggregato congelato è 2,25 GiB, 20 processi, CPU 40%,
   code 768 KiB/12 e massimo tre prenotazioni simultanee. Nessun limite del
   prodotto viene aumentato. Headroom aggiuntivo: 512 MiB fisici e 1 GiB commit.
4. Due proxy di guardia consentono solo l'autorità esatta dell'origine sintetica
   locale: Claude → guardia → sidecar canonico MITM → guardia → HTTPS/SSE finto.
   Le guardie connettono esclusivamente indirizzi numerici `127.0.0.1`; altre
   destinazioni ricevono 403. Il sidecar usa `BILI_UPSTREAM_PROXY` esplicito.
5. Due processi Claude distinti e senza credenziali reali eseguono lo stesso prompt
   sintetico: senza CA deve risultare un rifiuto di certificato e zero richieste
   Messages; con CA solo nel figlio deve arrivare il marker fisso. Il secondo
   caso parte solo dopo il rilascio verificato dell'intero job del primo.
   Cleanup di tutti gli owner anche su errore; ledger vuoto prima di dichiarare
   il successo. Fixture preservata.

La fixture Node non importa `child_process` e non avvia alcun figlio. Rust
costruisce tutti gli argv e gli ambienti fissi, usando la ricetta canonica del
sidecar e il pin Claude verificato. Il protocollo sui pipe privati comunica
solo porte loopback numeriche, PID del sidecar già posseduto, fasi e risultati
booleani: nessun argv arbitrario, token, credenziale o output CLI. Rust drena e
campiona tutti gli owner durante ciascuna attesa; un guasto conserva i proprietari
necessari al cleanup e non produce evidenza positiva.

## Argomenti ed ambiente da revisionare

Claude riceve solo questi argomenti costanti (oltre al percorso exe fissato):

```text
--restricted --safe-mode --tools "" --disallowedTools "mcp__*"
--mcp-config {"mcpServers":{}} --strict-mcp-config
--setting-sources "" --settings {"disableAllHooks":true}
--no-chrome --no-session-persistence --max-turns 1
--model claude-sonnet-4-6 --output-format json
-p "Reply with the fixed synthetic marker."
```

Ambiente del figlio costruito da zero: solo SystemRoot/WINDIR, PATH System32,
home/config/AppData/temp nuovi sotto la fixture, proxy HTTP/HTTPS/ALL alla guardia,
NO_PROXY vuoto, `ANTHROPIC_BASE_URL` all'origine
HTTPS locale e chiave finta `shinatama-synthetic-no-provider-key`. Il prompt non
contiene dati reali. Nessun account, file di credenziali o configurazione personale
entra nell'ambiente. Sicurezza TLS non disattivata.

Disabilita traffico non essenziale, updater, telemetria, autoinstallazione
marketplace, integrazione IDE, log MCP; safe mode attivo. `API_TIMEOUT_MS=3000`,
`CLAUDE_STREAM_FIRST_BYTE_TIMEOUT_MS=3000`, `CLAUDE_CODE_MAX_RETRIES=0`,
`CLAUDE_CODE_CERT_STORE=bundled`, gzip richieste disattivato, output 128 token,
thinking disattivato. Nel solo caso positivo: `NODE_EXTRA_CA_CERTS` e
`SSL_CERT_FILE` indicano la CA privata appena generata. Nessuna mutazione
dell'ambiente del processo padre. I nomi delle variabili Windows non distinguono
maiuscole/minuscole; la ricetta Rust rifiuta alias duplicati e passa un solo nome
per proxy. Le variabili di rete non sono un firewall OS;
la prova qualifica il comportamento del solo binario fissato e del percorso
osservato attraverso le guardie. L'evidenza registra l'endpoint sintetico e zero
inoltri remoti delle guardie; non pretende un'osservazione completa dell'egress OS.

Deadline: 11 s per caso Claude, watchdog fixture 37 s, osservazione Rust 40 s,
lease 45 s per owner e rundown 2 s per job. Cattura massima 64 KiB per stream
di ciascun owner; comandi Rust ≤1 KiB ciascuno/8 KiB totali, righe Node ≤4 KiB;
solo booleani, contatori, codice di uscita, hash e marker verificato nell'evidenza.
Nessun output CLI o corpo HTTP persistito dal test. Il sidecar può scrivere il
proprio log sintetico esclusivamente nella fixture privata.

Al superamento del limite del singolo owner, una sola riga diagnostica riporta
il ruolo fisso, contatori del job, completezza e numero di identità verificate
e limite applicato. Nessuna command line, ambiente, output CLI o percorso di
processo. L'API di campionamento non espone nomi delle immagini. Il limite Claude
resta 8: Windows può associare helper console oltre `ActiveProcessLimit`, già
previsto dal lettore C09. L'evidenza positiva include il picco osservato e il
rilascio di ciascun owner; il nuovo aggregato descrive tre contenitori distinti,
non autorizza 20 processi nel job Claude.

## Aggiornamento del pin installato

Il 2026-11-03 il vecchio pin 2.1.316
(`e896a9d8da519353a49bf69ca4a695ba7e09067752687c675b3969041e632d52`,
335469216 byte) ha rifiutato l'eseguibile prima di avviare la fixture.
La lettura dei metadati PE di `C:\Users\alfio\.local\bin\claude.exe` riporta
`FileVersion` e `ProductVersion` 2.1.317.0, 338187936 byte. Il file
`C:\Users\alfio\.local\share\claude\versions\2.1.317` ha stessi metadati,
dimensione e SHA-256 `26b01ec22df2c17ac7f5703aa547d36706e8d58412136ced0344a8ced4d86038`.
Questo aggiornamento usa solo letture di file/metadati: nessuna CLI, autenticazione
o prova nativa è stata eseguita per identificare la nuova versione.

Comando eseguito con risultato **PASS**:

```powershell
cargo test --offline --locked -p shinatama-engine -j1 --test claude_proxy_tls -- --ignored --test-threads=1
```

Ricevuta locale: `.prototype-evidence/claude-proxy-tls.json`. Tutti gli owner
sono `Released`; il ledger delle prenotazioni è vuoto e l'ambiente TLS del
padre è invariato. Picchi osservati: fixture 2/4 processi, sidecar 2/8,
Claude negativo 5/8, Claude positivo 6/8. Le guardie hanno rifiutato una
destinazione non ammessa e non hanno inoltrato traffico remoto.

Il precedente job combinato raggiungeva 9 processi con un limite di 8.
Separare fixture, sidecar e harness rispecchia la proprietà del prodotto;
nessun limite del prodotto è stato aumentato. Il primo tentativo dopo la
separazione ha rifiutato la seconda ammissione perché riusava la revisione
iniziale del ledger. La correzione legge la revisione corrente prima di
ciascuna prenotazione, conservando la politica e i massimali congelati.

Limiti: endpoint API interamente finto, profilo headless separato, safe/restricted,
nessun modello reale, nessun login, nessuna fatturazione, nessuna qualifica degli
altri agenti o dell'intero C12/C15. La fiducia TLS del solo Node era già provata;
questa prova deve osservare proprio il binario Claude/Bun, senza inferirlo da Node.

Riferimenti primari consultati il 2026-11-03:

- [Claude Code: rete e CA](https://code.claude.com/docs/en/network-config): proxy
  ed aggiunta di CA nel processo figlio.
- [Claude Code: CLI](https://code.claude.com/docs/en/cli-reference): print,
  restricted, safe mode, disabilitazione strumenti e persistenza.
- [Claude Code: variabili](https://code.claude.com/docs/en/env-vars): timeout,
  retry e controlli del traffico non essenziale.
- [Claude Code: gateway](https://code.claude.com/docs/en/llm-gateway): endpoint
  custom e credenziale del gateway. Qui endpoint e chiave sono sintetici.
