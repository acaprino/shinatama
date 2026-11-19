# CX11 - Codex sign-in status without reading credentials (read-only; no login).
# Runs the signed-in case on the real home and the signed-out case in a scratch CODEX_HOME.
# Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$exe = (Get-Command codex).Source

function RunCase($label, $tag) {
  $o = Join-Path $out "codex-login-status.$tag.stdout.txt"
  $e2 = Join-Path $out "codex-login-status.$tag.stderr.txt"
  $p = Start-Process -FilePath $exe -ArgumentList 'login','status' -NoNewWindow -Wait -PassThru -RedirectStandardOutput $o -RedirectStandardError $e2
  $so = (Get-Content $o -Raw); if ($null -eq $so) { $so = '' }
  $se = (Get-Content $e2 -Raw); if ($null -eq $se) { $se = '' }
  "$label : exit=$($p.ExitCode) stdout='$(($so -replace '\s+',' ').Trim())' stderr='$(($se -replace '\s+',' ').Trim())'"
}

RunCase 'real home' 'real'

$scratch = Join-Path $env:LOCALAPPDATA 'shinatama-verify\signin\codex'
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
$env:CODEX_HOME = $scratch
RunCase 'scratch CODEX_HOME' 'scratch'
Remove-Item Env:CODEX_HOME
