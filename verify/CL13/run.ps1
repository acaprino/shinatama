# CL13 - Claude Code sign-in status without reading credentials (read-only; no login).
# Runs the signed-in case on the real home and the signed-out case in a scratch CLAUDE_CONFIG_DIR.
# Identity fields are redacted before anything is written. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null

function Redact($text) {
  $text -replace '("email"\s*:\s*)"[^"]*"', '$1"<redacted>"' `
        -replace '("orgId"\s*:\s*)"[^"]*"', '$1"<redacted>"' `
        -replace '("orgName"\s*:\s*)"[^"]*"', '$1"<redacted>"'
}
function RunCase($label, $file) {
  $raw = (& claude auth status --json 2>&1 | Out-String)
  $code = $LASTEXITCODE
  Set-Content -Path (Join-Path $out $file) -Value (Redact $raw) -Encoding utf8
  $keys = try { ((($raw | ConvertFrom-Json).PSObject.Properties.Name) -join ', ') } catch { '<not json>' }
  "$label : exit=$code keys=$keys"
}

RunCase 'real home' 'claude-auth-status.real.json'

$scratch = Join-Path $env:LOCALAPPDATA 'shinatama-verify\signin\claude'
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
$env:CLAUDE_CONFIG_DIR = $scratch
RunCase 'scratch CLAUDE_CONFIG_DIR' 'claude-auth-status.scratch.json'
Remove-Item Env:CLAUDE_CONFIG_DIR
