# OC8 - OpenCode sign-in detection (read-only; no login).
# Runs `opencode auth list --standalone` on the real home and in a scratch OPENCODE_DB/XDG, each case
# bounded by a timeout. Reliability note (2026-10-27): the real-home case succeeded twice early and then
# failed five times in a row with "Location plugins did not finish activating" (both via the shim and the
# direct binary); the scratch case answers with --standalone. See the OC8 record; prefer the HTTP route.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$exe = (Get-Command opencode).Source
if ($exe -notlike '*.exe') {
  $cand = Join-Path (npm root -g) '@opencode\cli\bin\opencode.exe'
  if (Test-Path $cand) { $exe = $cand }
}
"binary: $exe"

function RunCase($label, $tag, $envs, $cwd, $seconds) {
  $job = Start-Job -ScriptBlock {
    param($x, $e, $c)
    foreach ($k in $e.Keys) { Set-Item -Path ("env:" + $k) -Value $e[$k] }
    Set-Location $c
    ((& $x auth list --standalone 2>&1 | Out-String) + "`nexit=$LASTEXITCODE")
  } -ArgumentList $exe, $envs, $cwd
  if (Wait-Job $job -Timeout $seconds) {
    $o = Receive-Job $job
    Set-Content -Path (Join-Path $out "opencode-auth-list.$tag.txt") -Value $o -Encoding utf8
    $flat = (($o -replace '\s+',' ').Trim())
    if ($flat.Length -gt 300) { $flat = $flat.Substring(0,300) }
    "$label : $flat"
  } else {
    Stop-Job $job
    Set-Content -Path (Join-Path $out "opencode-auth-list.$tag.txt") -Value "TIMEOUT >$seconds s" -Encoding utf8
    "$label : TIMEOUT >$seconds s"
  }
  Remove-Job $job -Force
}

RunCase 'real home' 'real' @{} (Get-Location).Path 90

$s = Join-Path $env:LOCALAPPDATA 'shinatama-verify\signin\oc'
New-Item -ItemType Directory -Force -Path $s | Out-Null
$envs = @{ OPENCODE_DB = "$s\opencode.db"; XDG_DATA_HOME = "$s\data"; XDG_CONFIG_HOME = "$s\config"; XDG_STATE_HOME = "$s\state" }
RunCase 'scratch OPENCODE_DB/XDG' 'scratch' $envs $s 90
