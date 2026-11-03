# Collegamento del thread al ciclo di vita nativo

Preparazione C25/F02: il terminale Windows accetta callback Rust per registrare
il proprietario prima dell'esecuzione e chiuderlo dopo il cleanup confermato.
L'API di avvio esistente resta disponibile senza callback.

`before_run` riceve l'identità esatta del processo sospeso, già verificato nel
job e dopo la chiusura dell'ammissione di nuovi processi. Un errore o panic
impedisce `ResumeShell` e passa al cleanup ordinario della sola sessione.

`after_cleanup` viene invocato solo dopo rilascio nativo e dei worker I/O.
Un errore conserva la sessione come chiusura del collegamento pendente; Close
ritenta soltanto il salvataggio finale. Il callback deve essere idempotente.
Non viene dedotta la morte di un processo da un timeout o da un PID isolato.

Il compositore Claude ora supporta anche operazioni esplicite con UUID
canonico: `--session-id` per una nuova conversazione e `--resume` per quella
esatta. Il test mirato del compositore è passato. La ricetta esistente resta
immutata finché il controller non sceglie questa nuova API.

I due test mirati `lifecycle_` sono passati: errori e panic durante la
registrazione, nessun rilascio senza cleanup nativo confermato e ritentativo
della sola chiusura pendente. Compilazione desktop e helper Windows riuscita.

Queste API non registrano da sole segmenti nativi e non rendono disponibile
Resume nella UI. Il collegamento degli eventi Claude, compresi i cambi con
`/resume` nella TUI, e la prova di ripresa effettiva restano da integrare.
