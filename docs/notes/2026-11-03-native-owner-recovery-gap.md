# Recupero del proprietario nativo dopo crash: limite C25/F02

Stato: **mancanza reale ancora aperta**, rilevata con audit del codice. Questa nota
non attesta implementazione o prove del recupero e non blocca gli altri incrementi.
Nessuna modifica a prodotto, piano o checkpoint accompagna la nota.

Dopo un arresto improvviso di Shinatama, un'attivazione senza `ended_at` può restare
salvata. `agents/resume.rs::target` rifiuta qualsiasi `recorded_activation` presente:
oggi non esiste un percorso qualificato che ricostruisca l'autorità necessaria per
chiuderla da un nuovo processo Shinatama. Non è corretto aggirare questo rifiuto.

## Primitive esistenti e confine della loro prova

| File e simbolo | Cosa fornisce; cosa manca |
| --- | --- |
| `crates/state/src/threads.rs::{ActivationOwner, ActivationRecord, ThreadResumeTarget, end_thread_activation_exact}` | Istanza, PID e FILETIME del processo agente, intervallo e revisione. La chiusura confronta proprietario, sequenza e revisione in transazione; non osserva processi e non certifica pulizia. Mancano host e collegamento al ticket/job. |
| `crates/state/src/resource_reservations.rs::{ReservationOwner, ReservationTicket, EmptyJobProof, release_reservation, reconcile_reservations}` | Host esatto, sessione Windows, nonce, job, budget e revisioni. Il rilascio richiede prova di job chiuso e vuoto. Il ticket non contiene istanza/thread; il rilascio lo elimina senza lasciare una ricevuta per l'attivazione. |
| `crates/engine/src/resources/{admission.rs,job.rs}::{PreparedWorkload, ClosedEmptyJobProof, reconcile_dead_owner}` | `ticket()` e `owned_key()` espongono il collegamento nativo durante la vita del proprietario. `finish_cleanup()` chiude il lavoro e rilascia il ledger solo dopo le prove richieste. Dopo crash, `reconcile_dead_owner()` restituisce soltanto `Retired`: il censimento locale con handle dei discendenti è perso. |
| `crates/state/src/records/recovery.rs::probe_owner_identity`, `crates/state/src/global_workspace.rs::{probe_global_workspace_owner, recover_global_workspace}` | Osservazione limitata di PID e creazione, con handle e attesa a tempo zero; distingue vivo, morto, PID mancante/riutilizzato e stato incerto. La prova del workspace è legata a proprietario e revisione del layout: non collega una sessione al suo job né prova la fine dei discendenti. |
| `crates/terminal/src/windows_host.rs::{WindowsNativeSessionLifecycle, owner_worker}`, `apps/desktop/src-tauri/src/agents/thread_lifecycle.rs::{prepare, finish}`, `crates/engine/src/launch/identity.rs::LaunchIdentityRecord` | `before_run` avviene dopo contenimento e `seal_assignments`, prima di `ResumeShell`; riceve solo istanza e processo agente. `after_cleanup` avviene dopo pulizia confermata. L'identità di lancio e il proprietario completo restano in memoria; l'attivazione persistita conserva soltanto parte di quei dati. |

Il runtime desktop non richiama attualmente `reconcile_dead_owner()` o
`reconcile_reservations()` per le prenotazioni lasciate da un host precedente.
Un crash può quindi lasciare anche capacità riservata nel ledger. Ripristinare
la disponibilità del thread e rilasciare quella capacità sono operazioni distinte,
entrambe subordinate alle proprie prove.

## Finestra di crash concreta

L'ordine attuale è:

1. `PreparedWorkload::finish_cleanup` conferma la pulizia nativa e delle code.
2. `Database::release_reservation` rimuove il ticket dal ledger.
3. `owner_worker` richiama `session.finish_binding(native_released)`.
4. `ThreadLifecycle::finish`, dopo il drenaggio degli eventi, chiude l'attivazione
   con `end_thread_activation_exact`.

Un crash fra 2 e 4 lascia **attivazione aperta e ticket assente**. Il nuovo host
non può distinguere questa situazione da un'associazione mancante, un dato
precedente al protocollo o un'altra prenotazione: non esiste il collegamento
persistito fra le due registrazioni né una ricevuta durevole della pulizia.

## Cosa non autorizza lo sblocco

- Scadenza del lease, tempo trascorso o solo numero PID non provano la morte del
  proprietario esatto. PID riutilizzato significa morte della vecchia identità,
  senza alcuna autorità sul processo nuovo.
- Morte del solo agente non prova morte dell'host o fine del gruppo di subprocessi.
  Morte dell'host, job assente per nome e contatore dei membri a zero non recuperano
  il censimento con handle perso nel crash. La policy corrente li rifiuta come
  prova completa di pulizia.
- Assenza del ticket, recupero del workspace, evento `SessionEnd` e presenza del
  file della conversazione non sono ricevute di terminazione del job.
- Accesso negato, altra sessione Windows o identità non verificabile restano
  condizioni esplicite di blocco. Non si termina un processo aperto per nome o PID
  per fabbricare la prova mancante.

## Interventi minimi ordinati e riprendibili

1. **Persistire il collegamento prima dell'esecuzione.** Al punto `ShellReady`,
   usare `PreparedWorkload::ticket()`/`owned_key()` e l'identità verificata del
   processo agente per registrare un'associazione fidata fra istanza, host
   PID/FILETIME/sessione Windows, nonce, ticket e job. Gli intervalli di attivazione
   della stessa istanza devono riferirsi a quell'associazione immutabile. Integrare
   il salvataggio con il confine `before_run`: se fallisce, `ResumeShell` resta
   vietato. Nessun campo di autorità proviene da IPC. I record storici senza
   associazione restano riconoscibili e non vengono completati per supposizione.
2. **Rendere durevole la prova di pulizia.** Collegare la prova nativa autentica
   alla stessa associazione e conservarne una ricevuta nella transazione che
   rilascia il ticket. L'assenza del ticket non sostituisce la ricevuta. La normale
   finalizzazione del thread può consumarla solo insieme alla chiusura esatta
   dell'intervallo; una transazione fallita la conserva per il recupero. Definire
   retention limitata dopo finalizzazione, senza eliminare prove ancora necessarie.
3. **Recuperare esplicitamente un solo thread.** La richiesta contiene solo
   selettori e revisione attesa. Il backend carica associazione e attivazione,
   osserva fuori dalla transazione le identità esatte del vecchio host e del
   processo agente, e richiede la ricevuta di pulizia collegata. Un proprietario
   vivo o un dato incerto impedisce la modifica. La transazione finale confronta
   l'intera associazione, proprietario, intervallo e revisione prima della chiusura;
   un cambiamento concorrente richiede ricaricamento. Poi ricarica il target:
   Resume resta un'azione separata con le verifiche native già esistenti.

Questi interventi coprono la finestra **pulizia provata → finalizzazione interrotta**.
Un crash precedente alla prova di pulizia resta bloccato: per recuperarlo serve un
protocollo ulteriore qualificato di terminazione dell'intero gruppo. Non introdurre
un reaper generale o indebolire `reconcile_dead_owner()` dentro questo incremento.
Se il proprietario è ancora nel processo corrente, usare il percorso già disponibile
di arresto/riprova sul medesimo `TerminalHost`, che conserva handle e autorità.

Verifiche essenziali pianificate: proprietario vivo e PID riutilizzato; accesso
negato/associazione mancante; crash dopo ricevuta durevole prima della finalizzazione;
crash senza ricevuta con budget conservato; due recuperi concorrenti o intervallo
cambiato, con una sola chiusura valida. Sono casi da implementare ed eseguire,
non risultati osservati da questa nota.

Prima azione alla ripresa: definire il record di associazione e il suo salvataggio
al confine `ShellReady`, mantenendo invariato il rifiuto attuale di Resume finché
non esiste una prova completa.
