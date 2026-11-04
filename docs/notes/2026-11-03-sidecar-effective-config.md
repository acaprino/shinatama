# C12.4 — configurazione effettiva: prova source e limiti CLI

Stato: **prova source-runtime PASS; C12.4 resta aperto**. Il 3 novembre 2026 il
test confinato ha superato 1/1 prove in **5,98 s** (compilazione: 1,17 s).
Sorgente canonico, artefatti originali e pin rimangono invariati. Sono verificati
i controlli sotto descritti nel sorgente eseguito; non il traffico Claude/Bun
o l'intera configurazione effettiva del CLI originale in esecuzione.

Identità: `sidecar/source-build/provenance.lock.json`, package `0.1.207`,
source tree `68d08d963cc1e081f6605677db824db62d3eddc1c3ec7126e444fdc9cd885579`,
artifact tree `6bac74bc9ce76fdd38635adb7e6b2aa4fa536c55e6c61fd8f4a642787675f1e8`.
Radice canonica: `D:/Projects/horness-launcher/packages/billion-context`.
Copia preparata: `sidecar/source-build/prepared`.

## Percorsi effettivi verificati nel sorgente

| Voce | Riferimento canonico | Cosa è dimostrabile |
| --- | --- | --- |
| Tiers | `src/compress-settings.ts:314` e `:339`; `resolveRequestConfig` a `:379` | Il valore risolto, non una compressione realmente avvenuta nel CLI. |
| Adaptive pressure | `src/musashi-terminal/adaptive-turn.ts:64–83`; `adaptive-pressure.ts:23` | `enabled` e `maxTokens` sono consumati. La curva usa `config.nudge.maxContextLimitPct`. |
| Target ratio/growth window | `src/musashi-terminal/adaptive-pressure.ts:3–20` | Sono accettati/validati, ma nei moduli `musashi-terminal` non esiste un consumatore dei due campi. Il rapporto nudge predefinito è già `0.75` in `kernel/src/config.ts:10`: la coincidenza non prova che il controllo targetRatio funzioni. |
| Epoche | `src/musashi-terminal/epoch.ts:52–58` | Il passo è `epochTokens`, oppure metà soglia con minimo 32.000 token. Nessuna corrispondenza dimostrata con `growthWindow=20`. |
| Prompt rewrite | `src/config.ts:1552`; `musashi-terminal/rewrite-hook.ts:20–33`; `prompt-rewrite.ts:140–205` | La modalità attiva è `opts.promptRewrite`. L'ammissione viene persistita prima del dispatch. La fixture usa solo catalogo/completion locali iniettati. |
| Output steering | `src/server/relay.ts:362–365`; `src/output-steering.ts:448–458` | Risoluzione per provider, decisione kernel e modifica del corpo. Routing off si prova con effort iniziale high; controllo positivo con routing on. |
| Compression off | `src/config.ts:1301–1304`; `server/handle.ts:839–847` | Passthrough disarma il percorso preparato; il solo resolver non prova i byte inoltrati dal CLI. |

`GET /__bili/config` (`src/web/api.ts:100–154`) rilegge il file e non attesta
gli `opts` già catturati. Il manifest plugin (`server/admin.ts:405–413`,
`plugin.ts:853–921`) espone strumenti/capacità ma non tutti questi controlli.
Status/overview (`server/admin.ts:1115` e `:1160`) non li includono.

La fixture `crates/engine/tests/fixtures/effective-sidecar.mjs` verifica dieci
file contro il pin e l'entry CLI prima/dopo. Il bundle di prova vive solo nella
directory temporanea privata. Protocollo: `built` → censimento del supervisore
Rust → `census-complete` → `effective-source-complete`. Non eseguirla fuori
dall'ammissione C09R. Riporta distintamente source runtime e
`runningCliEffectiveConfigVerified=false`; target/growth restano non qualificati.

## Risultato reale del 3 novembre 2026

Comando eseguito, senza altre suite o Clippy:

```text
cargo test --offline -p shinatama-engine --test effective_sidecar -j1 -- --ignored --test-threads=1 --nocapture
```

Il driver `crates/engine/tests/effective_sidecar.rs` verifica tutti i file del
bundle preparato e il Node fissato, crea configurazione e ledger SQLite nuovi
in una directory privata C01 e ammette fixture ed esbuild locale nello stesso
job prima della loro esecuzione. Il censimento prima di fermare esbuild rileva
**3 processi attivi, 3 totali, appartenenza completa**. Limiti immutati: job
1 GiB, processo 768 MiB, 8 processi, CPU 20%, output complessivo 64 KiB e tempo
massimo 30 s. Output catturato: **3.014 byte**. Dopo l'ultima istruzione il driver
chiude la propria pipe stdin con EOF reale, senza `process.exit` nella fixture.
Cleanup `Released`, handle del processo noto segnalato e ledger vuoto sono
verificati prima della rimozione della directory temporanea.

Risultati osservati:

1. Tiers e pressione adattiva risolti; percorso adattivo attivo/disattivo eseguito,
   finestra reale del modello preservata.
2. Prompt rewrite `auto` ammesso una volta; modalità `off` senza dispatch.
   Un completamento sintetico locale, **zero chiamate a pagamento**.
3. Output steering modifica realmente il corpo con verbosità L2. Effort `high`
   resta invariato con routing off e diminuisce nel controllo positivo on;
   steering disabilitato lascia il corpo invariato.
4. Resolver passthrough canonico osservato. Nessun traffico provider o byte
   inoltrato dal CLI è qualificato da questa prova.
5. Configurazione privata, dieci sorgenti selezionati e entry CLI originale
   conservati. `targetRatio=0.75` e `growthWindow=20` sono configurati ma non
   consumati dal percorso osservato: entrambi i controlli restano non qualificati.

Ricevuta locale: `.prototype-evidence/effective-sidecar.json`; log:
`.verification/effective-sidecar-live.log`. La ricevuta riporta esplicitamente
`runningCliEffectiveConfigVerified=false`, `providerTrafficVerified=false` e
lo scope `canonical-source-runtime-not-cli`. L'artefatto di prova deriva dal
sorgente verificato; non sostituisce né riqualifica il bundle CLI originale.

| Evidenza | SHA-256 del run PASS |
| --- | --- |
| Driver Rust | `cdab76b5f3eec193e2f9ebf4e239e60e23dfbeb67e06c31ca8e788eea489eed2` |
| Fixture JavaScript | `38ccfdbdea090ef3f61e100a2d71a7a5c2309bc29935989e5712435a3db77395` |
| Artefatto source-runtime privato | `81262ec240fe48fe1b8abcb1a51a95f1f3c4cdec3a1fd3b901d1f1d46a4e74db` |
| Log | `6863324be9c64eab24a046958292ed6117e6c76f3927dd22f2aa68c6dd3474eb` |
| Ricevuta | `ab9302117e2f1d3ebc75daee1df1219a1c308e2d3a7144709815104666a282cf` |

## Proposta concreta, non applicata

Aggiungere al fornitore canonico un endpoint locale read-only
`GET /__bili/runtime-settings`, nel perimetro amministrativo esistente, che usi
**gli oggetti opts/config del processo**, senza rilettura file/credenziali.
Proiezione versionata, scope esplicito `global`, soli valori whitelist:

```json
{
  "schemaVersion": 1,
  "scope": "global",
  "compressionEnabled": true,
  "tiersEnabled": true,
  "adaptivePressure": {
    "enabled": true,
    "maxTokens": 160000,
    "effectiveCurveRatio": 0.75,
    "targetRatioControlEffective": false,
    "growthWindowControlEffective": false
  },
  "promptRewriteMode": "auto",
  "outputSteering": {
    "enabled": true,
    "verbosityLevel": 2,
    "effortRouting": false
  },
  "providerOverridesPresent": false
}
```

Questa è una forma proposta, non una risposta osservata. Il globale deve
segnalare gli overlay: non è la configurazione di ogni richiesta. Eventuali
selettori per route/modello devono essere registrati e limitati, senza URL
arbitrari, percorsi o metadati di autenticazione nella risposta.

La modifica minima per rendere operativo targetRatio sarebbe la seguente,
**solo dopo** aver stabilito la precedenza rispetto a `compress.maxContextLimit`:

```diff
--- a/src/musashi-terminal/adaptive-turn.ts
+++ b/src/musashi-terminal/adaptive-turn.ts
@@ parametro della chiamata adaptivePressureThreshold (riga 83)
- ratio: config.nudge.maxContextLimitPct,
+ ratio: settings.targetRatio,
```

È un diff semantico, non una patch da applicare ciecamente alla riga compatta.
Il parser assegna 0.75 anche quando il campo non è esplicito: senza una decisione
sulla precedenza si cambierebbe chi aveva configurato un diverso maxContextLimit.
L'alternativa conservativa per Shinatama è configurare il controllo canonico
effettivo `compress.maxContextLimit=0.75` e non presentare targetRatio come
controllo indipendente funzionante.

Per growthWindow non si propone una formula inventata: le epoche hanno altre
unità. Prima il fornitore deve definirne l'effetto oppure deprecarlo. Non
convertire 20 in messaggi, turni o token; lo stato runtime deve indicare che
attualmente non ha effetto.

## Addendum 4 novembre 2026 — percorso operativo verso l'accettazione 2–3

Verifiche odierne (sola lettura): `sidecar/prepare.ps1` (Verify) **PASS**;
l'hash della sorgente canonica `src/web/api.ts` in
`D:/Projects/horness-launcher` (`64dd0c79…`) è identico a quello pinato;
**horness-launcher È il progetto Musashi** (`res/sidecar-prepare.mjs`,
`sidecar/source-build`, `musashi-terminal.exe`). La catena operativa di C12.4
coinvolge quindi due progetti, non tre, e l'ultimo commit sidecar canonico è
`91ac78a` (2 nov), già dentro il pin corrente.

Percorso A (accettazione 2 letterale — endpoint nel processo canonico):

1. **horness-launcher**: aggiungere `GET /__bili/runtime-settings` in
   `packages/billion-context/src/web/api.ts` secondo il contratto whitelist
   proposto sopra (oggetti opts/config di processo, nessuna rilettura file o
   credenziali) + test in quel repo. Richiede coordinamento: un'altra sessione
   agente lavora su quel progetto.
2. **Musashi artefatti**: rebuild del bundle tramite la pipeline esistente
   `res/sidecar-prepare.mjs` (nessuna scorciatoia: l'artefatto va ricreato dal
   sorgente modificato).
3. **Shinatama re-pin**: `provenance.lock.json` è fail-closed (Verify/Prepare
   non aggiornano il lock; identità, `upstreamCommit 7e69b49f…` e alberi sono
   hardcoded in `prepare.ps1`). Il re-pin è un atto deliberato: nuova emissione
   del lock + costanti aggiornate + diff revisionabile.
4. **Prova Shinatama**: estendere la fixture `effective-sidecar` sotto
   ammissione C09R per interrogare l'endpoint dal processo sidecar vivo
   (identità di processo legata a porta/pid noti al supervisore) e confrontare
   i valori whitelist con gli effetti sulle richieste sintetiche catturate da
   upstream loopback (pattern `claude_proxy_tls` per l'accettazione 3).
   Regressioni C12/C13 obbligatorie (accettazione 4).

Percorso B (solo Shinatama, nessun cambio canonico): prova su **traffico vivo**
— Claude CLI reale attraverso il sidecar canonico con provider sintetico
guardato, catturando i byte upstream e asserendo gli effetti osservabili della
configurazione effettiva (steering L2 sul corpo, ammissione prompt-rewrite,
passthrough con compressione off). Soddisfa "il solo GET del file non basta"
con effetti osservati, non introspezione di processo: più debole rispetto alla
lettera dell'accettazione 2; va registrato come tale.

In entrambi i percorsi resta la decisione pendente su `targetRatio`
(precedenza rispetto a `compress.maxContextLimit`) e `growthWindow` (contratto
con unità o deprecazione). Via conservativa già documentata: usare il
controllo canonico effettivo `compress.maxContextLimit=0.75` e non presentare
i due campi come controlli indipendenti funzionanti.

## Accettazione prima di chiudere C12.4

1. **Superato:** fixture source nel job posseduto, con hash invariati, controlli
   positivi/negativi e zero chiamate a provider. Il risultato è limitato allo
   scope source-runtime sopra documentato.
2. Nel CLI originale preparato verificare identità del processo e valori runtime
   whitelist; confrontarli con richieste sintetiche catturate da upstream
   loopback. Il solo GET del file non basta.
3. Provare effort high invariato con routing off, verbosità attiva, compressione
   on/off e parametri adaptive realmente implementati. Dopo una decisione e
   modifica canonica, provare il cambio rapporto. Growth window resta aperto
   fino a contratto con unità e test.
4. Ogni modifica canonica richiede nuovo artefatto/pin e regressioni C12/C13.
   Una prova source non qualifica automaticamente il CLI, Claude/Bun o l'effetto
   della configurazione nel prodotto.
