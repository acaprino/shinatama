# Codex: ripresa nativa della conversazione salvata

Incremento del 3 novembre 2026 (sera tarda), commit `97e904a`. Completa il
percorso aperto da `2026-11-03-codex-thread-tracking.md`: la ricetta Resume
tipizzata (interna dal commit `755618b`) è ora esposta, qualificata e provata
nativamente con Codex reale 0.166.0. Il catalogo pubblicizza
`native_resume=true` per Codex; le azioni UI Resume/Recover diventano
disponibili sui thread Codex.

## Contratto implementato

- `codex.rs::prepare_session` accetta `SessionOperation::Resume(target)` solo
  con bersaglio esatto: stesso thread, stessa radice del box, segmento
  `harness=="codex"`, nessuna attivazione già registrata. Costruisce
  `CodexSessionOperation::resume_native(<uuid>)` — solo UUID canonico esatto;
  nomi, prefissi e `--last` restano rifiutati (`InvalidNativeSession`).
- Argomenti sealed: `["resume", <uuid>]` + 15 fissi + 6 hook = **23**, dentro
  il bound condiviso `MAX_LAUNCH_ARGUMENTS=32`. La ricetta è qualificata come
  `Operation::ResumeNative` con descriptor proprio: un descriptor StartNew non
  può richiedere né pianificare argv resume (`Composition`), e il permit lega
  operation+digest argv, quindi argv resume non può viaggiare su un permit
  StartNew.
- La ricevuta del canary StartNew tracked è riutilizzata (stesso schema già
  provato per Claude: `receipt.matches` confronta agente/programma/versione/
  profilo; l'operation risiede nel binding del lancio, non nella ricevuta).
- Lifecycle `PendingThread::Resume`: l'attivazione è **sigillata alla barriera
  sospesa** (prima di ResumeShell) con l'UUID salvato; `observe_start` per
  Codex esige `SessionStart` con `source=resume` e UUID identico a quello
  sigillato. Qualsiasi difformità marca il tracking fallito: la finalizzazione
  resta bloccata e la ricevuta di cleanup resta trattenuta per il recupero
  esplicito (nessuna chiusura di storia su conferma dubbia).

## Prove

Smoke nuova `apps/desktop/tests/codex-resume-smoke.mjs` (protetta; provider
sintetico irraggiungibile, guardia solo-inoltro; evidenza
`.prototype-evidence/codex-resume.json`): **PASS ×2** su build isolata nel
worktree `.jcode/local/worktrees/race-proof` (contenuto di `97e904a`, target
dedicato, nessuna interferenza con altre sessioni). 10/10 controlli:

1. New session → hook `source=startup` → UUID nativo esatto (come prova
   precedente); Stop → ricevuta consumata, thread `inactive`;
2. **Resume via pulsante UI reale** (sidebar): l'attivazione #2 è sigillata
   sull'UUID salvato prima di qualsiasi hook (stesso singolo segmento, owner
   distinto, ricevuta di cleanup attiva sul processo esatto);
3. **Continuità nativa**: Codex stesso renderizza la conversazione precedente
   (PROMPT_A visibile a schermo prima di digitare PROMPT_B) e il **suo rollout
   sotto lo stesso UUID cresce** (118.439 → 130.999 byte, unico file
   accodato, `session_meta.id` == UUID, entrambi i prompt presenti);
4. conferma hook `source=resume` con UUID esatto: evidenziata negativamente
   dal clean stop (una difformità avrebbe trattenuto owner e ricevuta —
   coperta dai test unitari); stop → attivazione #2 chiusa, ricevuta consumata,
   thread `inactive`, 1 thread / 0 terminali;
5. nessuna onboarding ripetuta (credenziali/fiducia nel box), limiti tracked
   esatti applicati, phase `running`, zero rifiuti di coda; guard: 0 inoltri,
   98 dinieghi; provider: 0 colpi; frame chiuso con exit 0.

Unitari: `codex_resume_selection_is_exact_and_qualifies_only_its_own_operation`
(argv esatto, separazione capability StartNew/Resume, 23 argv tracked, ID
invalidi rifiutati); lifecycle desktop +2
(`codex_resume_seals_the_saved_uuid_and_confirms_with_source_resume`,
`codex_resume_rejects_wrong_source_or_uuid_and_retains_the_sealed_owner`,
5/5); engine lib 103/103; desktop bins 30/30. Regressione StartNew:
`codex-native-smoke.mjs` **PASS** sulla stessa build.

## Limiti (non dichiarare superati)

- Nessun turno di modello completato: ogni rotta esterna è rifiutata dalla
  guardia (Codex 0.166.0 non onora override env del base URL); la continuità è
  provata su transcript/rollout nativi, non su risposta del modello.
- La conferma hook positiva non ha un delta DB osservabile (l'attivazione è
  già sigillata): l'enforcement è negativo (mismatch → owner trattenuto),
  provato a livello unitario e indirettamente dal clean stop della smoke.
- Credenziali sintetiche salvate nel box dall'onboarding reale del prodotto;
  nessun account reale o OAuth ChatGPT; guardia proxy-forwarding-only, non un
  firewall OS.
- Il canary resta quello StartNew tracked: la qualifica di avvio contenuto non
  è ripetuta per l'argv resume (il permit lega operation+digest; pattern
  identico a Claude già provato).
