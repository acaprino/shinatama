# Recupero del cleanup canary ritardato

Un canary poteva completare il rilascio dopo la prima attesa di Stop. Al Check
successivo, il servizio confermava Stop e scartava l'owner senza leggere la
ricevuta di qualifica eventualmente pubblicata nel frattempo. La cartella
temporanea rimaneva conservata e contribuiva al limite di otto cartelle.

`CanaryOwner::finish_released` riunisce la finalizzazione del percorso ordinario,
del recupero ritardato e dell'errore di avvio. I chiamanti lo invocano solo dopo
Stop confermato e dopo aver rilasciato i propri pin C01. Il metodo rilascia gli
allegati, preleva la ricevuta di quel canary e usa la stessa pulizia limitata già
esistente. L'owner viene scartato dopo questa sequenza.

Senza ricevuta nativa non viene tentata alcuna rimozione. I canary non qualificati
o ritirati conservano le cartelle; nessuna ricevuta viene ricostruita e nessun
percorso preesistente viene adottato.

La compilazione `cargo build --offline --locked -p shinatama-desktop --features
custom-protocol -j1` è passata in 24,06 secondi. La nuova prova UI Claude ha
attraversato il servizio comune fino alla TUI originale e al primo onboarding;
si è fermata sulla classificazione della schermata di autenticazione sintetica,
prima di inviare messaggi. Non dimostra il caso ritardato.

Verifica mirata ancora aperta: una prova con completamento ritardato deve
mostrare che il successivo Check consuma la ricevuta reale e rimuove la sola
cartella posseduta. La variante senza ricevuta deve conservarla. La correzione
non allarga i criteri di rimozione: riusa la stessa ricevuta e lo stesso metodo
di pulizia del percorso ordinario, anche quando il rilascio arriva più tardi.
