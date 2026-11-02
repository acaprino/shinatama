# Priorita: resilienza dell'host e controllo adattivo delle risorse

Data: 2026-11-02. Stato: requisiti utente registrati; proposta tecnica da provare.
Nessuna nuova garanzia di runtime e dimostrata da questo documento.

## Decisioni dell'utente

- La stabilita di Shinatama ha priorita: l'instabilita di un subprocesso non deve
  causare crash, blocco dell'interfaccia o perdita dello stato degli altri lavori.
- Il consumo di memoria e delle altre risorse richiede calibrazione intelligente.
- Dopo la riduzione del carico e il blocco di nuovi avvii, se un subprocesso continua
  a minacciare l'host, Shinatama arresta automaticamente solo l'albero responsabile,
  conserva quanto e gia salvato e indica come riprendere. Confermato esplicitamente
  dall'utente in questa chat. Non e promessa di salvare la memoria del processo morto.

## Impostazione proposta e rapporto con le decisioni esistenti

Un controllore locale deterministico nel runtime proprietario misura, ammette gli
avvii e applica budget ai singoli alberi. Non richiede chiamate a modelli o un servizio
residente; mantiene ADR-0008. Un semplice allarme non impedisce l'esaurimento delle
risorse. Aggiungere un demone non garantirebbe da solo limiti o recupero corretto.
Le soglie numeriche e la frequenza di misura sono scelte da qualificare, non valori
gia approvati o prestazioni gia ottenute.

Il job globale serve alla pulizia al termine dell'app. I limiti restrittivi di CPU,
memoria e numero di processi vanno sui job dei carichi, separati dall'host e dal suo
renderer: un limite globale che include Shinatama potrebbe affamare l'app stessa.
Il budget aggregato coordina le finestre attraverso lo stato condiviso esistente;
le prenotazioni di avvio sono atomiche, hanno proprietario e scadenza, e vengono
riconciliate dopo un crash senza usare soltanto il PID. Non introduce uno scheduler
di attivita degli agenti. Il margine per OS, runtime e renderer e headroom monitorato,
non una promessa di RAM fisicamente riservata contro ogni altro programma.

C09 conserva le prove P14 e la sonda ConPTY gia registrate: questa richiesta aggiunge
la protezione dai guasti dei figli verso l'host, distinta dalla chiusura dei figli
quando termina l'host. Se un componente nativo interno al processo non supera le
prove, la sua separazione in un helper richiede una revisione architetturale esplicita;
non si dichiara contenuto un crash nativo soltanto intercettando errori Rust.

## Misure e limiti

Per albero: memoria privata impegnata (commit), memoria residente, picchi e velocita
di crescita, CPU, numero di processi, velocita di output e occupazione delle code.
Per sistema: RAM disponibile, commit residuo e pressione; per host/renderer:
consumo, latenza di input, frame time e tempo di risposta dei comandi. Distinguere
commit e memoria residente; non sommare contatori dello stesso processo due volte.
Identita delle misure: PID e istante di creazione, con gestione di accesso negato,
processo terminato e campioni mancanti. La telemetria e locale, limitata nello spazio
e nella conservazione; non contiene transcript, credenziali o ambiente completo.

Servono un budget aggregato e budget per albero, con limiti applicati dal sistema
operativo prima dell'esecuzione del carico, oltre alle soglie preventive. Il limite
di memoria impegnata puo far fallire un'allocazione nel figlio: non equivale a un
arresto ordinato e non salva il suo lavoro. Il supervisore deve gestire anche questo
caso. Un solo campionatore non protegge da una crescita piu rapida del campionamento.
Le notifiche preventive sono affiancate da controlli periodici e limiti effettivi.

Anche Shinatama deve limitare allocazioni, messaggi IPC, letture, parsing, code,
scrollback, cache e log. I byte PTY non vengono scartati arbitrariamente rompendo
lo stato del terminale: usare contropressione e quote eque, eliminando solo dati
esplicitamente sacrificabili. Letture, attese, parsing e arresti non bloccano la UI
ne mantengono lock condivisi durante operazioni lente. EOF, pipe interrotte, input
malformato e guasti dei worker producono uno stato locale visibile.

## Calibrazione e intervento

1. Partire da un profilo conservativo misurato sulla macchina; ammettere un nuovo
   albero solo se resta il margine stabilito per host, renderer, OS e carichi attivi.
2. Usare campioni sani e picchi osservati per tipo/versione di carico, con finestra
   temporale limitata e tetto configurato. Un consumo in crescita continua o un
   incidente non alza automaticamente la baseline o il tetto massimo.
3. In pressione ridurre cache sacrificabili, rendering non visibile e attivita
   opzionali dell'host; rinviare nuovi avvii e applicare quote ai carichi. Nascondere
   un pannello non arresta l'agente. Non sospendere ciecamente un processo che puo
   mantenere lock o transazioni aperti.
4. In emergenza arrestare solo l'albero responsabile secondo la policy confermata,
   con tentativo ordinato entro un tempo massimo e terminazione forzata successiva.
   Se la pressione e esterna o non attribuibile, non scegliere una vittima arbitraria:
   degradare l'host, rifiutare nuovi avvii e segnalare il motivo.
5. Ripristinare gradualmente i budget solo dopo un intervallo stabile e sotto una
   soglia di rientro piu bassa di quella di intervento. Misurare l'overhead del
   controllore. Se le misure mancano o il controllore fallisce, conservare i limiti
   di sicurezza e negare nuovi avvii non coperti dal budget.

La UI mostra il consumo, il limite applicato e il motivo della limitazione/arresto.
Le impostazioni espongono profili e limiti comprensibili, senza permettere a un
override di eliminare il margine minimo di sicurezza. I valori esatti vengono
congelati prima delle prove di accettazione, senza allentarli per farle passare.

Il recupero dei sidecar idempotenti ha proprietario unico, attesa crescente, numero
massimo di tentativi e blocco dei riavvii dopo guasti ripetuti. Un processo vivo ma
occupato non viene duplicato; la perdita di heartbeat non basta a dichiararlo morto.
Un agente arrestato per risorse non viene riavviato/resumato automaticamente e non
riceve nuovamente l'ultimo comando. La UI conserva i riferimenti alla sessione e
offre la ripresa nativa solo quando l'adattatore la supporta.

## Prove richieste e collocazione nel piano

- C09R: misure e ammissione aggregate, limiti nativi per albero, calibrazione con
  isteresi e prove sintetiche riproducibili; prerequisito di C09 e C13.
- C09: integra i controlli nei terminali e prova crash, blocco, saturazione CPU,
  allocazioni fino al limite, proliferazione di figli, output continuo/malformato
  e code piene con almeno un terminale sano che continua a rispondere.
- C13: applica gli stessi budget ai sidecar e prova recupero limitato, assenza di
  duplicati, errori di allocazione, pipe rotte e guasti del monitor stesso.
- QA Windows di ogni agente: ripete le prove applicabili su sessioni sintetiche
  con versioni reali; V1 richiede anche una prova prolungata di almeno un'ora con
  guasti ripetuti, piu finestre e pressione variabile. Misura crescita residua dopo
  recupero, memoria, CPU e latenza; confronta i limiti fissati prima dell'esecuzione.
- RES1-RES4 nel registro restano aperti. Crash/OOM di sistema si simulano in un
  ambiente di test con limiti o in VM, mai esaurendo deliberatamente la macchina
  di lavoro. Un fallimento dell'host blocca la qualifica, non si archivia come rumore.

F32CI resta il prossimo passo del piano; questa registrazione non lo esegue ne lo
chiude. C09R aggiunge una stima iniziale di 24-48 ore, in quattro unita da 6-12 ore;
si aggiorna dopo le prove native e la misura del controllore.

## Fonti tecniche verificate (documentazione, non prove locali)

- [Limiti di memoria dei Job Objects](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information).
- [Quote CPU e gerarchie di job](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_cpu_rate_control_information).
- [Soglie di notifica: i processi possono continuare a superarle](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_notification_limit_information_2).
