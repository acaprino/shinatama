# CX4 runtime probe runner - codex app-server detect/import on a synthetic Claude session.
# Scratch homes only; no login, no model calls. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$env:STX_CODEX_BIN = (Get-Command codex).Source
$env:STX_CX4_SCRATCH = Join-Path $env:LOCALAPPDATA 'shinatama-verify\CX4'
$stamp = Get-Date -Format 'HHmmss'
$log = Join-Path $out "runtime-$stamp.txt"
node (Join-Path $PSScriptRoot 'app-server-probe.mjs') 2>&1 | Tee-Object -FilePath $log
"exit code: $LASTEXITCODE"
"log: $log"
