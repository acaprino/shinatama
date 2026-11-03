# Codex: operazione tipizzata per la futura ripresa esatta

Stato: preparazione interna implementata; **Resume Codex non abilitato**.
L'avvio New conserva gli stessi argomenti, permessi e profilo già esistenti.

`CodexSessionOperation` distingue New da Resume di un UUID canonico esatto.
Rifiuta nomi, prefissi, percorsi, `--last`, normalizzazioni e argomenti aggiunti.
La forma Resume conserva `Operation::ResumeNative`; non può essere trasformata
in una ricetta usando la qualifica StartNew. Il costruttore della ricetta la
rifiuta con `QualificationRequired`. Non esiste ancora un ingresso pubblico
`prepare_resume` e il catalogo continua a dichiarare `native_resume=false`.

Verifica essenziale: `cargo test --offline --locked -p shinatama-engine -j1
codex_launch::tests --lib`, **3/3 PASS**. Copre argomenti New invariati, selezione
esatta, rifiuto degli identificatori ambigui e impossibilità di usare la qualifica
di avvio per Resume. Nessun CLI, provider o account è stato avviato da queste prove.
`cargo check --offline --locked -p shinatama-desktop --features custom-protocol
-j1`: **PASS**, 45,64 s, incluso il mapping dell'errore nel driver desktop.

Il lettore Codex esistente e la verifica root/UUID/cwd di `agents/resume.rs`
possono essere riutilizzati. Manca la cattura autorevole dell'UUID di New e il
collegamento al lifecycle comune. Il tag ufficiale
[Codex 0.166.0](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/cli/src/main.rs)
accetta `resume <UUID>`; per New non viene inventato un ID da Shinatama.
Il payload [SessionStart](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/hooks/src/events/session_start.rs)
fornisce sessione e cwd, ma affidabilità, fiducia limitata al bridge e momento
effettivo dell'evento devono essere verificati prima di abilitare la funzione.

Prossimo incremento: adattare gli eventi Codex al trasporto autenticato e al
lifecycle condiviso, senza duplicare salvataggio e pulizia o modificare la
fiducia degli hook personali. I thread precedenti privi di binding non vengono
associati per supposizione.
