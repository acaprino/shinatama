# macOS e Linux rinviati su decisione dell'utente

Richiesta del 2026-11-02: "non ho mac nè linux, al momento supportiamo solo windows ufficialmente".
Questo è un rinvio di scope, non una rimozione e nemmeno una promozione dei requisiti.

## Resta rinviato

- `F11-macos`, `F11-linux` e i dodici passi `QA-*-macos` e `QA-*-linux`.
- Build, firma, notarizzazione, CI e misure su macOS e Linux, compreso Linux musl.
- Le clausole di FRD e righe del registro che nominano quei sistemi.

## Prosegue

CI, TL1, QA, pacchetti e accettazione V1 valgono su Windows. Il codice portabile già presente
resta, senza essere provato né promesso. Riprendere richiede una nuova indicazione dell'utente e una
nuova versione di ADR-0047, poi TL1 e i passi rinviati sul sistema ripreso.
