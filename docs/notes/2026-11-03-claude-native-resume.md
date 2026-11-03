# Claude: hook reali, archivio nativo e ripresa esatta

Stato: **PASS al primo avvio, 1/1 in 9,78 secondi il 3 novembre 2026** con
Claude reale 2.1.317. Due `SessionStart` autenticati, una richiesta Messages per
avvio, stesso UUID e stesso file nativo con entrambi i turni. Durante la ripresa
il provider sintetico ha ricevuto la prima risposta assistant nel contesto e
il secondo prompt (`resumeHistoryObserved=true`).

L'evidenza è in `.prototype-evidence/claude-native-resume.json`: identità dei due
avvii distinte, matcher `startup`/`resume` verificati, quattro owner rilasciati e
ledger vuoto. Picchi osservati: 2 processi per gli endpoint, 2 per il sidecar,
5 per il primo Claude e 6 per il secondo; nessun limite aumentato. L'ambiente del
padre è rimasto invariato. Nessun accesso utente, provider a pagamento o archivio
personale è stato usato; le guardie non hanno inoltrato connessioni remote.

Il test ignorato
`installed_claude_emits_authenticated_hooks_and_resumes_exact_saved_uuid` resta
separato dalla prova TLS. Riusa `tests/support/claude_native.rs` per owner,
risorse e provider sintetico. La regressione TLS con questo supporto condiviso
è passata nello stesso comando in **9,18 secondi**.

## Verifiche superate

1. Claude reale avvia una conversazione con `--session-id <UUID>` in home,
   configurazione e progetto appena creati. Un hook `SessionStart` con matcher
   `startup` chiama il bridge reale `shinatama-hook.exe`.
2. Il ricevitore autentica l'identità del processo e accetta soltanto UUID,
   istanza, thread e cartella attesi. Il token passa esclusivamente nell'ambiente
   privato del figlio; non entra in argv, file, log o risultati.
3. Dopo il cleanup verificato, il test legge soltanto il nuovo file sintetico
   `projects/<cartella>/<UUID>.jsonl`: limite 1 MiB, 32 cartelle, 128 righe e
   256 KiB per riga. Contiene il primo prompt e la prima risposta.
4. Un secondo owner con una nuova identità esegue `--resume <stesso UUID>` nello
   stesso progetto. Il matcher `resume` produce un nuovo hook autenticato.
   Il provider riceve la risposta precedente nel contesto e il secondo prompt;
   il file originale conserva entrambi i turni.
5. Tutti gli owner e il ledger risultano rilasciati prima di scrivere
   l'evidenza `.prototype-evidence/claude-native-resume.json`. Nessun retry,
   login, chiave reale, lettura di archivi personali o modifica dell'app utente.

La prova usa Claude e Node fissati dal supporto TLS condiviso. Git Bash è
`C:/Program Files/Git/usr/bin/bash.exe`, SHA-256
`acf4ecb52e601f7b4a37db51b07650b5d0315eafd010590e98079fa026da4b7b`.
Il bridge è quello già compilato in `target/debug/shinatama-hook.exe`; i suoi
byte e i nomi degli antenati sono mantenuti aperti contro scritture/rinomine
durante la prova. Il digest del bridge osservato è
`abbfa9c664cf8a99ddab19154d6a4e7caeadcbf7351f198064ceb3b063a452b6`.
Git Bash viene indicato con
`CLAUDE_CODE_GIT_BASH_PATH`; nessun PATH personale è importato.

## Ricetta fissa

```text
--tools "" --disallowedTools "mcp__*"
--mcp-config {"mcpServers":{}} --strict-mcp-config
--setting-sources "" --settings <nuovo file privato del singolo avvio>
--no-chrome --max-turns 1 --model claude-sonnet-4-6 --output-format json
--session-id <UUID> -p SHINATAMA_SYNTHETIC_START_PROMPT_A
```

Il secondo avvio sostituisce soltanto l'ultima riga con
`--resume <stesso UUID> -p SHINATAMA_SYNTHETIC_RESUME_PROMPT_B` e usa un nuovo
file hook con il matcher `resume`. Nessun `--continue`, picker, fork o bypass
dei permessi. Safe mode e `--no-session-persistence` sono assenti perché
disattiverebbero le funzioni da osservare. Il progetto è vuoto; MCP e strumenti
sono vuoti; la configurazione contiene soltanto i due hook del bridge.

Provider e CA sono sintetici e locali. Entrambi gli avvii usano la CA privata
nel solo figlio; le guardie consentono unicamente l'origine loopback esatta.
Gli stessi limiti congelati della prova TLS restano attivi: massimo tre owner
simultanei, 8 processi nel job Claude, 1 GiB per job Claude, 768 MiB per processo,
CPU 10%, cattura 64 KiB per stream e 11 secondi per avvio. Il bridge è compreso
nel job del relativo Claude; il limite non viene aumentato per farlo passare.
`SessionEnd` è osservazione supplementare: non sostituisce il cleanup C09.

## Limite dell'evidenza

Il test dimostra il percorso **headless** del binario fissato. Non attraversa
la UI, il controller di ripresa, il consumer persistente `ThreadLifecycle` o
ConPTY; non concede autorità C11 e non certifica il profilo TUI normale.
La ricetta TUI preparata è sigillata: il test non aggiunge `-p` ai suoi argv.
L'ispezione JSONL riguarda soltanto dati sintetici prodotti qui e non introduce
un lettore alternativo nel prodotto. Le guardie osservano il loro inoltro,
non sono un firewall Windows né una misura completa dell'egress OS.

La documentazione ufficiale conferma gli hook in sessione `-p` senza dialogo
di fiducia: [workspace trust](https://code.claude.com/docs/en/hooks#workspace-trust).
Persistenza e ripresa sono documentate nella
[CLI reference](https://code.claude.com/docs/en/cli-reference).
Il percorso esplicito della shell Windows è documentato nelle
[variabili d'ambiente](https://code.claude.com/docs/en/env-vars).

Comando eseguito dal coordinatore dopo revisione della ricetta; entrambe le
prove sono passate in sequenza:

```powershell
cargo test --offline --locked -p shinatama-engine -j1 --test claude_proxy_tls --test claude_native_resume -- --ignored --test-threads=1
```
