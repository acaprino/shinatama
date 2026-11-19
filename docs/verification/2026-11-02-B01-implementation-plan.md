# B01 — Piano completo e checkpoint riprendibile

Data: 2026-11-02. Fonti: tutti i 17 FRD Markdown accettati, indice ADR,
specifiche companion/token e stato reale del prototipo. Il piano e il primo
incremento sono autorizzati dalla richiesta di procedere per passi funzionanti,
committabili e riprendibili.

## Risultato verificato

`docs/implementation/plan.json` assegna tutti i 704 requisiti a 121 incrementi,
con dipendenze, risultati, file, criteri osservabili, verifiche, gate e stime.
I 40 incrementi piu grandi sono suddivisi in 131 unita di commit sequenziali:
212 confini di commit complessivi, considerando gli incrementi piccoli singoli.
Ogni unita grande richiede al massimo 12 ore stimate e mantiene avviabile l'app.
Le stime non costituiscono date di consegna e si aggiornano con le integrazioni.

`progress.json` distingue pianificato, in corso, verificato, committato e bloccato.
I gate restano nel registro: copertura del piano non significa requisito consegnato.
Non sono stati creati commit o SHA storici fittizi. Il protocollo descrive il
bootstrap del lavoro gia presente e l'esclusione del lavoro parallelo estraneo.

## Verifiche eseguite

- `python scripts/implementation.py validate`: 121 passi, 704/704 assegnazioni,
  nessun riferimento sconosciuto, nessun ciclo, checkpoint coerente.
- `python -m unittest discover -s scripts -p test_implementation.py`: 13 test passati.
  Coprono requisiti persi, cicli, dipendenze inesistenti, due passi attivi,
  prerequisiti saltati, post-V1 senza opt-in, SHA inesistente, padre chiuso prima
  delle unita e unita avviate fuori ordine.
- `python scripts/implementation.py render`: documento leggibile rigenerato
  dai file autorevoli; `status` individua il prossimo passo effettivamente pronto.
- Revisione indipendente di copertura e dipendenze core/estensioni, seguita da
  correzioni: C14 distingue manifest da prove live; C09 distingue meccanismi da
  certificazioni per agente/piattaforma; DeepSeek certifica ACP e non una TUI;
  finalizzazione V1 dipende dalle capacita e certificazioni necessarie.

Il successivo emendamento FRD-0009/R24 dello stesso giorno e stato incorporato
in C12/C13.5: fix storage schema v9/CCR ereditato dal singolo sorgente, regressione
di latenza locale sotto 750 ms e integrita dati. La prova estesa di un ora, dopo
l'integrazione degli adattatori, e esplicita nelle certificazioni Claude per OS.
I risultati upstream restano distinti dalla futura qualificazione Shinatama.

Sono documentati esplicitamente i percorsi di S1 ricerca/resume e S2 hop seed,
poi le estensioni V1 e il widget, con importazione nativa e valutazioni fuori V1.
Le vecchie assunzioni native-only vengono riallineate alla shell Tauri autorizzata
senza riscrivere le approvazioni storiche. Una release parziale non viene chiamata
V1 completa se mancano requisiti obbligatori.

## Limiti

Le future verifiche sono pianificate; comandi/test non ancora esistenti devono
essere realizzati nel relativo passo prima di dichiararne il successo. Gli OS
non provati, le credenziali di test, gli account/firme di distribuzione e le seam
esterne ancora sconosciute restano prerequisiti reali delle rispettive consegne.
Il validatore non sostituisce i test del prodotto, la revisione o i gate di sicurezza.
