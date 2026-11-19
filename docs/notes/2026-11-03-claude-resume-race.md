# Claude: prova di esclusività della ripresa con richieste concorrenti

Incremento C25.2 del 3 novembre 2026 (sera). Protocollo
`apps/desktop/tests/claude-resume-race-smoke.mjs`, stessa postura della prova
UI Resume: app/ConPTY/UI reali, provider sintetico loopback dietro guardia
solo-inoltro, bearer sintetico, nessun account reale. Evidenza:
`.prototype-evidence/claude-resume-race.json`.

## Esiti

Run #1 **PASS** (10/10 controlli, build 183174f) e run #5 **PASS** — copia
definitiva su build isolata nel worktree `.jcode/local/worktrees/race-proof`
(commit bf6bf2f, target dedicato, nessuna interferenza con altre sessioni):
vincitore 6,85 s contro rifiuto `busy` del perdente a 7 ms, `owned_elsewhere`
post-commit, revisione 3→4, provider 1 start + 1 resume con contesto
precedente, guard 2 inoltri e 0 overflow. Sull'identica build isolata è stata
ripetuta anche `codex-native-smoke.mjs`: **PASS** 11/11 (con il prewarm del
sidecar ora incluso in entrambi gli smoke). Contenuto dei run:

1. thread riprendibile creato con la UI (turno start, Stop, ricevuta cleanup
   consumata, thread `inactive`);
2. varianti rifiutate prima di qualsiasi lancio: `projectId` estraneo e
   `expectedRevision` scaduta → `source_changed`, zero sessioni avviate;
3. **razza reale**: due `thread_resume` nello stesso tick JS
   (`Promise.all` nella stessa `evaluate`) → esattamente un vincitore
   (7,0 s) e un perdente rifiutato con `busy` in 9 ms, mentre il vincitore
   stava ancora preparando: sovrapposizione autentica, non sequenziale;
4. il vincitore lega l'unico owner: stesso segmento/UUID nativo del primo
   turno, attivazione aperta contro la sua istanza, ricevuta di cleanup
   collegata a processo e ticket esatti; il perdente non lascia alcuna
   traccia (nessuna sessione, attivazione o voce di journal in più);
5. duplicato post-commit → `owned_elsewhere` con la revisione ricaricata
   (il binding del vincitore alza la revisione del thread 3→4: una richiesta
   con revisione precedente riceve `source_changed`, come osservato);
6. vincitore fermato via `terminal_stop`: ricevuta consumata, thread di nuovo
   `inactive` e riprendibile;
7. gamba funzionale: Resume UI reale mantiene thread, UUID e contesto
   precedente (MARKER_A nel contesto del provider, MARKER_B a schermo, terzo
   owner esatto, stesso file di transcript nativo con entrambi i turni);
8. tutte e tre le ricevute consumate a fine prova; guard: 2 inoltri (i due
   turni), 0 overflow; provider: 1 start + 1 resume, contesto precedente
   osservato; frame chiuso con exit 0.

## Mappa dei livelli di esclusività dimostrati

- duplicato **in volo** → CAS `busy` per harness (`agents.rs::reserve`):
  il secondo richiedente è rifiutato finché il primo non restituisce;
- duplicato **a owner committato** → `recorded_activation` →
  `owned_elsewhere` (`resume.rs::target`);
- **sorgente scaduta/estranea** → `source_changed` (revisione o progetto non
  corrispondenti, verificati due volte: prima e dopo `verify_native`);
- **backstop cross-processo** → quattro indici UNIQUE parziali su
  `activation` (thread/segment/native/instance aperti) più il lock owner della
  data directory: due processi app non possono condividere lo stesso home, e
  anche in caso di lock violato il binding doppio è strutturalmente
  impossibile a livello SQL (evidenza unitaria: test di recupero concorrente
  esatto in `native_session_cleanup`).

La gamba "Claude riprende soltanto nella sua cwd" è esercitata sul lato
positivo da ogni Resume riuscito (`verify_native` confronta la cwd del
transcript nativo con il progetto salvato); il lato negativo (transcript con
cwd diversa) non è costruibile dalla UI senza manomissione del box ed è
coperto a livello engine dalle prove headless esistenti (2f72e7c).

## Nota ambientale (run #2–#4, non valide come esiti di prodotto)

- Run #2: ripetizione sotto pressione di commit dell'host (1–3 GiB liberi
  contro ≥5 richiesti); il primo Open riusciva, il vincitore della razza fu
  rifiutato con `unavailable` senza avviare processi. Coerente con
  l'ammissione risorse del governatore (trap #2), ma la causa non fu
  registrata separatamente: da allora lo smoke cattura `agent_status`
  (`startupFailure`) nei fallimenti.
- Run #3/#4: **invalidati da una sessione agente concorrente sullo stesso
  repository**, che ha ricompilato i binari condivisi in `target/debug`
  durante la corsa (hook.exe sostituito alle 20:10:28 a prova in corso). Il
  prodotto ha rifiutato correttamente con "Claude startup no longer matches
  its qualified configuration" perché l'identità pinnata dei programmi era
  cambiata tra qualifica e lancio: comportamento atteso, prova non valida.
  Rimedio metodologico: ripetizione in worktree isolato
  (`.jcode/local/worktrees/race-proof`) con target dedicato e binari built
  dall'esatto commit della prova.
- In entrambi i casi nessun processo nativo è rimasto attivo e le fixture
  sono state chiuse pulite; i rifiuti erano statici e classificati.

Dal run #1 lo smoke attende inoltre che il sidecar sia `ready`+`healthy`
prima del primo Open (avvio esplicito via `sidecar_start`, la stessa command
della UI): elimina la corsa con la finestra fissa di 5 secondi della
preparazione quando l'host è lento, senza toccare la logica di prodotto.

## Limiti

- La razza è esercitata dentro un unico processo app (due richieste IPC
  concorrenti su due worker `spawn_blocking`); la variante a due processi OS
  sullo stesso DB è esclusa a monte dal lock owner ed è provata solo a
  livello di indici/unittest, come dichiarato sopra.
- Provider e bearer sintetici in loopback; guardia proxy-forwarding-only, non
  un firewall OS; nessuna qualità modello o account reale.
- Il perdente della razza è rifiutato dallo slot per-harness prima di
  raggiungere il DB: il livello SQL resta il backstop, non il primo rifiuto
  osservato in questa prova.
