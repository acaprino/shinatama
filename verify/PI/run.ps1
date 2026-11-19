# C18 - Pi adapter installed-runtime fixtures (no login, no prompt, no model call).
# Runs the contained engine checks against the installed Pi: the version/Node-floor
# probe and the offline RPC control check (get_state, get_commands and one
# unsupported command that must be refused). Both use fresh private homes only.
# The TUI open/close proof is apps/desktop/tests/new-session-smoke.mjs with
# SHINATAMA_SMOKE_SESSION_HARNESS=pi after a desktop + helper build.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
Push-Location $root
try {
  $log = Join-Path $out 'pi-engine-fixtures.txt'
  & cargo test --offline -p shinatama-engine --lib -- pi_probe --include-ignored --nocapture --test-threads=1 2>&1 |
    Tee-Object -FilePath $log | Select-String -Pattern '^test |test result|pi_version_observation'
  if ($LASTEXITCODE -ne 0) { throw "Pi engine fixtures failed (exit $LASTEXITCODE); see $log" }
} finally {
  Pop-Location
}
