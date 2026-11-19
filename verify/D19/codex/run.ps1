# D19, Codex leg: the whole row in order (rerunnable without edits; wipes only %LOCALAPPDATA%\shv\d19cx and
# %LOCALAPPDATA%\shinatama-verify\D19\codex\out). Ports 47131-47138 only; kills only the PIDs it started.
#   setup.ps1     synthetic home + fixtures (local marketplace, MCP stub, user-level skills), warm-up turn
#   variants.ps1  per-launch switches with the set plugins NOT installed (what -c can and cannot do)
#   sets.ps1      fallback: set plugins installed but disabled; none / A / B / empty / profile / --ignore-user-config;
#                 two instances at once in the one home; thread/list; plain resume of B's thread
#   pwsh -NoProfile -File verify\D19\codex\run.ps1
$ErrorActionPreference = 'Stop'
$free = (Get-PSDrive C).Free / 1GB
if ($free -lt 2) { throw ("drive C: has {0:N2} GB free (< 2 GB); stopping" -f $free) }
$log = Join-Path $env:LOCALAPPDATA 'shinatama-verify\D19\codex\run.log'
New-Item -ItemType Directory -Force -Path (Split-Path $log) | Out-Null
Remove-Item -Recurse -Force (Join-Path $env:LOCALAPPDATA 'shinatama-verify\D19\codex\out') -ErrorAction SilentlyContinue
& { foreach ($s in 'setup.ps1', 'variants.ps1', 'sets.ps1') {
    "===== $s ($(Get-Date -Format o))"
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot $s) 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE) { throw "$s failed with exit $LASTEXITCODE" }
} } 2>&1 | Tee-Object -FilePath $log
("drive C: free after: {0:N2} GB" -f ((Get-PSDrive C).Free / 1GB)) | Tee-Object -FilePath $log -Append
"log: $log"
