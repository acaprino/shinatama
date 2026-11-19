# CX4 - app-server protocol schema from the installed Codex (no session started, no login used).
# Writes the schema bundle under out\<date>\schema\ (default) and out\<date>\schema-experimental\.
# Uses a scratch CODEX_HOME so the user's own Codex home is untouched. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$scratchHome = Join-Path $env:TEMP 'shinatama-verify-CX4-codex-home'
New-Item -ItemType Directory -Force -Path $scratchHome | Out-Null
$env:CODEX_HOME = $scratchHome
"CODEX_HOME scratch: $scratchHome"
"=== default bundle ==="
(codex app-server generate-json-schema --out (Join-Path $out 'schema') 2>&1 | Out-String)
"=== experimental bundle ==="
(codex app-server generate-json-schema --experimental --out (Join-Path $out 'schema-experimental') 2>&1 | Out-String)
$files = Get-ChildItem (Join-Path $out 'schema') -Recurse -File -ErrorAction SilentlyContinue
$filesExp = Get-ChildItem (Join-Path $out 'schema-experimental') -Recurse -File -ErrorAction SilentlyContinue
"bundle files (default): $($files.Count); (experimental): $($filesExp.Count)"
"--- externalAgentConfig hits (default bundle):"
$files | Select-String -Pattern 'externalAgentConfig' -List | Select-Object -First 10 | ForEach-Object { $t = $_.Line.Trim(); $_.Path.Replace($out,'') + ' :: ' + $t.Substring(0, [Math]::Min(110, $t.Length)) }
"--- externalAgentConfig hits (experimental bundle):"
$filesExp | Select-String -Pattern 'externalAgentConfig' -List | Select-Object -First 10 | ForEach-Object { $t = $_.Line.Trim(); $_.Path.Replace($out,'') + ' :: ' + $t.Substring(0, [Math]::Min(110, $t.Length)) }
