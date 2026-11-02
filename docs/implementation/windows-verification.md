# Verifica Windows — F32CI

Eseguire dalla radice:

```powershell
pwsh -NoProfile -File scripts/verify.ps1
```

Equivalente: `npm run verify`. Lo stesso script e usato da GitHub Actions.
Windows x64 e l'unico target supportato; macOS/Linux e Daodan restano rinviati.

## Prerequisiti

PowerShell 7, Python 3.13, Node dalla `.node-version`, rustup con la toolchain
fissata in `rust-toolchain.toml`, MSVC/Windows SDK e WebView2 installati.
La prima esecuzione ripristina le dipendenze esatte di npm/Cargo: servono le
normali connessioni ai registri. Nessun browser Playwright viene scaricato;
i test si collegano al WebView2 dell'app tramite una porta locale di test.

## Controlli

Il runner controlla versioni e ambiente, usa `npm ci` senza audit/fund,
valida piano e checkpoint, esegue i test Python e frontend, costruisce il frontend,
controlla inventario/notice, formattazione, clippy e test Rust con i lockfile.
Poi costruisce l'eseguibile embedded e prova PTY reale, controlli della finestra
e tre avvii per la cronologia progetti.

I test GUI sono sequenziali: porte CDP e profili isolati non vanno condivisi da
run simultanei nello stesso checkout. I processi sintetici passano per il job
kill-on-close del probe P14; questo protegge la pulizia della verifica, non
aggiunge il job globale al runtime del prodotto e non chiude P14.

Ogni controllo ha un limite di 15 minuti; il run locale ha un limite predefinito
di 25 minuti, modificabile entro 1–45 con `-TimeoutMinutes`. Il job remoto dura
al massimo 30 minuti. Un processo in timeout viene terminato con il proprio
albero; i test GUI hanno anche il loro job di contenimento. Log per controllo
limitati a 32 MiB, diagnostica mostrata limitata a 4 KiB per stream. Le prove
del runner coprono errore, timeout singolo/globale e output eccessivo.

## Risultati e artefatti

`verification.json` registra versioni, stato/durata/exit code, commit sorgente,
modifiche locali e hash/dimensione dell'eseguibile. La versione WebView2 riportata
e il candidato installato rilevato, non una certificazione del motore effettivo.

Sono caricabili solo:

- `.verification/verification.json` e `dependencies.json` sanitizzati;
- `desktop-terminals.png` e `projects-picker.png`, prodotti con dati sintetici;
- eseguibile debug, `THIRD_PARTY_NOTICES.md` e testi completi delle licenze.

Nessun profilo WebView, cronologia, impostazione personale, dump ambiente o log
generico e incluso negli artefatti. La CI usa `contents: read`, checkout senza
credenziali persistenti e azioni ufficiali fissate a SHA completi. Non pubblica
release, non firma e non avvia modelli/agenti reali.

## Confini della prova

Configurazione del workflow ed esecuzione locale sono distinte da un run GitHub
effettivo. Un'immagine Windows Server non certifica da sola Windows 10/11.
TL1, P14, installazione/firma, sidecar/Node provisioning e i gate completi
X3/X4/X5 restano aperti. L'inventario segnala obblighi MPL da riesaminare per la
release e non fornisce approvazione legale automatica.
