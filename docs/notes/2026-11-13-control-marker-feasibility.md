# Marcatore dei messaggi di controllo: fattibilita' per harness

Il piano di stabilizzazione prevede un bypass strutturale (SC9): il sidecar non altera ne' esternalizza i messaggi di controllo di Shinatama (seed, bootstrap, ricevuta). Il segnale e' un header HTTP della richiesta al modello, `x-shinatama-control: <kind>;<sha256 esadecimale>`, con `kind` in `seed`, `bootstrap`, `receipt`. Il marcatore non si deduce mai dal testo del prompt, dalla sua lunghezza o da `max_tokens`: se un harness non permette di aggiungere un header, si documenta il limite e non si ripiega su un marcatore nel testo.

## Come arriva davvero il bootstrap

`hop_launch` apre la destinazione con il percorso Open qualificato e `hop_deliver` incolla il bootstrap come bracketed paste (`apps/desktop/src-tauri/src/hop_run.rs`): nessun harness ha ancora una ricetta di prompt in argv. Il testo del bootstrap e' pero' noto prima del lancio (`crates/hop/src/channel.rs`, `bootstrap()`), quindi l'hash si calcola prima di aprire l'harness e l'header puo' essere statico per la vita del processo.

Un header statico accompagna anche le richieste ordinarie. Il sidecar applica il bypass solo se l'hash coincide con il testo del messaggio umano corrente (l'ultimo messaggio user che non e' un puro risultato di strumento); su ogni altra richiesta l'hash non coincide e il trattamento e' quello ordinario. Durante il primo turno il bootstrap resta il messaggio umano corrente anche nei giri di strumenti, perche' i risultati di strumento non sono messaggi umani: il bypass vale per tutto il turno.

## Esito per harness

Le prove sono state eseguite con i binari installati su questa macchina contro un'origine sintetica locale (nessuna chiamata a provider), leggendo l'header ricevuto dall'origine.

| Harness | Versione provata | Esito | Meccanismo e prova |
|---|---|---|---|
| Claude Code | 2.1.323 | Possibile | Variabile `ANTHROPIC_CUSTOM_HEADERS`, righe `Nome: valore` separate da newline ([env-vars](https://code.claude.com/docs/en/env-vars), richiede 2.1.227 o successiva). Con `ANTHROPIC_CUSTOM_HEADERS="x-shinatama-control: bootstrap;abc"` la richiesta `POST /v1/messages` arriva all'origine con `x-shinatama-control: bootstrap;abc`. Un valore gia' presente nell'ambiente va unito, non sostituito. |
| Codex | 0.169.0 | Non possibile | `model_providers.<id>.http_headers` e `env_http_headers` esistono ([config reference](https://learn.chatgpt.com/docs/config-file/config-reference)), ma il provider integrato `openai` e' riservato: `-c model_providers.openai.http_headers=...` fallisce con `model_providers contains reserved built-in provider IDs: openai. Built-in providers cannot be overridden.` Un provider personalizzato con `http_headers` inoltra l'header (provato), ma sostituisce il provider: cambia identita' del provider nelle sessioni, base URL e autenticazione ChatGPT, e rompe la ripresa. Limite di Codex. |
| OpenCode | 2.2.5 | Non possibile in modo generale | L'header si imposta per provider (`providers.<id>.headers`, [providers](https://opencode.ai/docs/providers/)); con `OPENCODE_CONFIG_CONTENT` e il provider `anthropic` la richiesta principale e quella del titolo portano l'header (provato). Il provider scelto dall'utente non e' noto al lancio e non esiste un header globale: elencare provider non configurati crea voci di provider vuote. Limite di OpenCode. |
| Pi | 1.4.0 | Non possibile in modo generale | `models.json` ammette `headers` per provider (`docs/models-configuration.md` del pacchetto installato); con il provider `anthropic` l'origine riceve l'header (provato). Stesso limite di OpenCode: nessun header globale e provider ignoto al lancio. In piu' l'argomento `@file` si fonde nel primo messaggio come `<file name="...">contenuto</file>` seguito dal prompt, quindi il testo del messaggio dipende dal percorso assoluto risolto da Pi. |

Fuori dal bypass restano quindi Codex, OpenCode e Pi: il loro bootstrap e' trattato dal sidecar come traffico ordinario (tag di riferimento, riscrittura del prompt, chiamata ausiliaria). Il limite e' dell'harness, non del sidecar.

## Seed, bootstrap, ricevuta

| Messaggio | Come passa dal sidecar | Marcatore |
|---|---|---|
| Bootstrap | Ultimo messaggio user del primo turno (incollato nell'harness) | `bootstrap;<hash>`, fattibile solo per Claude Code |
| Seed | Non e' un messaggio user: l'harness lo legge dal file indicato nel bootstrap con uno strumento, quindi arriva come risultato di strumento. Solo con il canale `file_arg` di Pi (e `protocol_prompt` di DeepSeek) il seed e' parte del primo messaggio user. | Il `kind` `seed` e' riconosciuto dal sidecar ma nessuna destinazione qualificata lo emette oggi. |
| Ricevuta (`ready <nonce>`) | E' la risposta dell'assistente, non un messaggio user inviato dall'harness: il sidecar la vede solo come storia. Il messaggio di recupero (`catchup.rs`) e' un messaggio user consegnato a un harness gia' in esecuzione, con un testo che non e' noto al lancio. | Il `kind` `receipt` e' riconosciuto ma non e' emesso: un header impostato al lancio non puo' coprire un messaggio successivo. |

## Normalizzazione del testo

Il testo incollato attraversa il terminale dell'harness, che puo' convertire gli a capo e togliere gli spazi esterni (il bootstrap termina con un a capo). L'hash si calcola quindi sul testo canonico: a capo normalizzati a `\n` e spazi, tab e a capo esterni rimossi. Shinatama e il sidecar usano la stessa regola. Non e' stato provato contro il terminale reale di Claude Code (la prova richiede una sessione interattiva con un provider): se il terminale cambia altro, l'hash non coincide e il messaggio riceve il trattamento ordinario, mai un bypass sbagliato. La prova nativa e' al confine del sidecar (header e bootstrap inviati direttamente all'origine sintetica): SC9 resta aperto per l'incolla nel terminale di Claude Code finche' non e' verificata.

## Emissione in Shinatama

`crates/hop/src/channel.rs` costruisce l'header (`control_header`, stessa funzione usata dai test del sidecar). `hop_launch` (`apps/desktop/src-tauri/src/hop_run.rs`) lo calcola dal bootstrap gia' verificato e, solo se la destinazione e' Claude Code, apre l'harness con `Controller::open_handoff`; `crates/engine/src/claude_launch.rs` lo unisce a `ANTHROPIC_CUSTOM_HEADERS` dell'ambiente di lancio (un valore dell'utente resta, un precedente header di controllo viene sostituito). Per le altre destinazioni `hop_launch` apre l'harness come prima.
