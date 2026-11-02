# F32CI — Verifica continua Windows

Data: 2026-11-02. Incremento implementato e verificato localmente su Windows
11 build 26200 x64. Tauri e definitivo; macOS/Linux e Daodan restano rinviati.
La configurazione GitHub e stata verificata staticamente; nessun run remoto
e dichiarato eseguito e nessuna release e stata pubblicata.

## Consegna

`npm run verify` / `pwsh -NoProfile -File scripts/verify.ps1` sono il punto unico
locale e CI. Toolchain Rust 1.99.1, Node 24.14.1, Python 3.13.14, PowerShell
7.6.5, npm 11.11.0; candidato WebView2 installato rilevato 156.0.4314.58.

Workflow Windows 2025 con sole azioni ufficiali fissate a SHA completi,
`contents: read`, checkout senza credenziali persistenti, concorrenza cancellabile,
deadline 30 minuti e artefatti su elenco esplicito. Il checkout completo permette
la verifica degli SHA storici del checkpoint. Il runner locale ha deadline
25 minuti, 15 minuti per controllo, limite log 32 MiB e diagnostica per stream
limitata a 4 KiB; i guasti del runner sono provati con processi sintetici.

## Risultato eseguito

Run locale finale: 16:14:58–16:16:04 UTC, circa 66 secondi su cache calda.
**15/15 controlli passati**:

| Gruppo | Esito |
| --- | --- |
| npm ci e piano | Dipendenze ripristinate; 709/709 requisiti assegnati e grafo valido |
| Runner | Successo, exit code 7, timeout per controllo/globale e log eccessivo verificati |
| Python | 29 test passati: piano/checkpoint e policy/inventario dipendenze |
| Frontend | 3 test passati e build/typecheck completati |
| Dipendenze | 281 pacchetti Windows, 487 testi/notice, zero errori di inventario |
| Rust | fmt e clippy con warnings come errori; 12 test unitari passati |
| App | Build embedded, PTY reale, controlli borderless e tre riavvii progetti passati |

I test GUI sono eseguiti in sequenza dentro il job globale del probe P14;
nessun agente reale, provider, credenziale o modello e usato. Questo non
aggiunge il job globale al runtime di produzione e non chiude P14.

Eseguibile debug non firmato: 13.558.272 byte, SHA-256
`40fe87efa7c60fbaf7e59a078eddcfc70cf5879426688f7e86c98c99eb4f9bcc`.
Report completi locali in `.verification/verification.json` e `dependencies.json`;
log locali esclusi dagli artefatti. Sono allegabili solo i due report, due
screenshot sintetici nominati, eseguibile debug e notice/testi di licenza.

## Dipendenze e limiti

Inventario Windows: 256 pacchetti Rust e 25 npm. Le categorie runtime e build/test
possono sovrapporsi; nessuna allowance non utilizzata e spacciata per componente
gia integrato. Il tool verifica membri Cargo effettivi e dichiarati, inclusi
path members impliciti, ereditarieta workspace, versioni npm esatte e lockfile.
Testi mancanti negli archivi usano supplementi revisionati legati a versione,
revisione VCS e hash; i wrapper npm hanno controlli di versione/repository/relazione.

Sette pacchetti MPL rimangono `release-review-required`: inventario e testi non
equivalgono ad approvazione legale di distribuzione. X3 resta aperto, insieme a
X1/X2, X4/X5 completi, TL1, P14 e provisioning Node/sidecar SC13. Un futuro run
Windows Server non certifica da solo Windows 10/11. La prova sidecar <750 ms
rimane a C13.5 e alle successive qualificazioni; non e eseguita da F32CI.

L'aggiornamento concorrente di FRD-0001/R59–R63 e stato letto e collegato a
C09R (quattro unita da 6–12 ore), C09/C13, QA e accettazione prolungata V1.
I gate RES restano aperti; la copertura del piano non li promuove.

## Fonti degli action pin

- [checkout v7.0.1](https://github.com/actions/checkout/commit/3d3c42e5aac5ba805825da76410c181273ba90b1)
- [setup-node v7.1.0](https://github.com/actions/setup-node/commit/949feb2413d6458794dcd2491c4babbbce0c15c1)
- [setup-python v7.0.0](https://github.com/actions/setup-python/commit/5fda3b95a4ea91299a34e894583c3862153e4b97)
- [upload-artifact v7.0.2](https://github.com/actions/upload-artifact/commit/cf430e030ddbb5b0abf93d22962f4752f3646cd9)
