# PI10 — Pi package name and update path

Item: PI10 (G2) — "The correct package name and update path (`@earendil-works/pi-coding-agent` installed; docs earlier named `@mariozechner/…`)". Pass when: "Checkup uses the right one."
Status: **running (partial)** — installed package and update path recorded; the checkup does not exist yet, so the pass criterion cannot be met.
Date: 2026-10-27.
Runner: JanuraCode agent, on the user's machine; npm registry queries only, cost 0.

Machine: Windows 11 Home build 26200; Node v24.14.1; npm 11.11.0.
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/PI10/run.ps1` (rerunnable without edits).
Raw output: `verify/PI10/out/2026-10-27/` (`pi-shim.txt`, `npm-ls-g.txt`, `npm-view-earendil.txt`, `npm-view-mariozechner.txt`).

## Result

1. `npm ls -g --depth=0` lists `@earendil-works/pi-coding-agent@1.2.0` (globally installed); the `pi` shim is `C:\Users\alfio\AppData\Roaming\npm\pi.ps1`; the package directory exists and contains `dist`, `docs`, `examples`, `schemas`, `README.md`, `package.json`; `bin` maps `pi` to `dist/bundle/cli.js` (useful for PI1 on Windows).
2. Registry: `@earendil-works/pi-coding-agent` latest = **1.4.0** (dist-tag `legacy-node20` = 0.74.2) → the update path is `npm install -g @earendil-works/pi-coding-agent@latest`; installed 1.2.0 is older than latest (drift to expect when the update runs).
3. The old name `@mariozechner/pi-coding-agent` still exists on the registry at 0.73.1, but is **not** what is installed. Engine 5.7 already names `@earendil-works/…` — no spec correction needed; the register's remark ("docs earlier named `@mariozechner/…`") stays accurate as history.

## Criterion

Not met yet (no checkup exists). The evidence above fixes what the checkup must use.

## Spec before / after

Before: engine 5.6/5.7 name `@earendil-works/pi-coding-agent` (correct).
After: unchanged.
