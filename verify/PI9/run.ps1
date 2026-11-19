# PI9 - Pi sign-in detection surface (no login; no credential output).
# `pi auth check --json --no-refresh` is the candidate detector: --no-refresh avoids refreshing OAuth
# credentials; --credentials and the print-* commands must never be used.
# Runs the help capture plus the check on the real home and in a scratch PI_CODING_AGENT_DIR.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null

$job = Start-Job { (pi auth --help 2>&1 | Out-String) }
if (Wait-Job $job -Timeout 120) {
  $o = Receive-Job $job
  Set-Content -Path (Join-Path $out 'pi-auth-help.txt') -Value $o -Encoding utf8
  'saved pi-auth-help.txt'
} else {
  Stop-Job $job
  'TIMEOUT >120s on pi auth --help (see PI11 for pi start-up variance)'
}
Remove-Job $job -Force

function RunCheck($label, $tag) {
  $job2 = Start-Job { ((& pi auth check --json --no-refresh --provider anthropic 2>&1 | Out-String) + "`nexit=$LASTEXITCODE") }
  if (Wait-Job $job2 -Timeout 90) {
    $o = Receive-Job $job2
    Set-Content -Path (Join-Path $out "pi-auth-check.$tag.txt") -Value $o -Encoding utf8
    "$label : $($o.Trim())"
  } else {
    Stop-Job $job2
    "$label : TIMEOUT >90s"
  }
  Remove-Job $job2 -Force
}

RunCheck 'real home' 'real'

$s = Join-Path $env:LOCALAPPDATA 'shinatama-verify\signin\pi'
New-Item -ItemType Directory -Force -Path $s | Out-Null
$env:PI_CODING_AGENT_DIR = $s
RunCheck 'scratch PI_CODING_AGENT_DIR' 'scratch'
Remove-Item Env:PI_CODING_AGENT_DIR
