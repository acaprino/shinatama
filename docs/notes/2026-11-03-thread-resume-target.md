# Destinazione esatta per la ripresa dei thread

Incremento C25: `Database::thread_resume_target` legge in una sola transazione
il thread, il suo segmento più recente, la radice nativa esatta e l'eventuale
apertura già registrata. Non cerca una conversazione alternativa e non libera
un proprietario solo perché è trascorso del tempo.

Gli esiti distinguono thread inesistente, non ancora collegato, archiviato,
radice modificata o indisponibile, schema non supportato e errore di lettura.
La disponibilità della radice non viene presentata come prova dell'esistenza
della conversazione nativa. Nessuna migrazione del database.

Verifica eseguita: test mirato
`threads::tests::resume_target_keeps_thread_root_and_owner_identity_exact`,
passato su Windows. Copre identità uguali in radici diverse, proprietario di
un segmento precedente, rilascio esplicito, radice sostituita e schema futuro.

Questo incremento prepara la ripresa: non espone ancora un pulsante Resume.
Servono il collegamento nativo autenticato, l'acquisizione esclusiva prima
dell'esecuzione e il rilascio dopo la chiusura confermata del processo.
