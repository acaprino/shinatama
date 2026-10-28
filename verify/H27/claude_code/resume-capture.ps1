# H27 / Claude Code - resumes an EXISTING session of the scratch home with one prompt and
# records what the model receives. Resets nothing (it is meant for sessions written by txcript).
# Writes to verify/H27/out/<Date>/claude_code/resume/:
#   <Tag>.capture.jsonl   every request Claude Code sent to the mock during this run
#   <Tag>.stdout.txt / .stderr.txt / .exit.txt   the harness's output (JSON result) and exit code
#   <Tag>.session-after.jsonl   the session file after the run (what the resume appended)
#   <Tag>.meta.json       session file, cwd used, line counts, command line
#   <Tag>.trap.jsonl      only if Claude Code tried to reach a real endpoint (blocked)
# The cwd is the one recorded in the session when it lies under %LOCALAPPDATA%\shinatama-verify,
# else the scratch work dir (pass -Cwd to force one).
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\claude_code\resume-capture.ps1 -SessionId <id> -Prompt <text> -Tag <name>
param(
  [Parameter(Mandatory = $true)][string]$SessionId,
  [Parameter(Mandatory = $true)][string]$Prompt,
  [Parameter(Mandatory = $true)][string]$Tag,
  [string]$Cwd,
  [string]$Date = '2026-10-27',
  [int]$TimeoutSec = 300
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')

$outDir = Join-Path $H27Repo "verify\H27\out\$Date\claude_code\resume"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$base = Join-Path $outDir $Tag
$capture = "$base.capture.jsonl"
$trapLog = Join-Path $H27Scratch "trap-resume-$Tag.jsonl"
Remove-Item "$base.*" -Force -ErrorAction SilentlyContinue
Remove-Item $trapLog -Force -ErrorAction SilentlyContinue

$sessionFile = Get-ChildItem (Join-Path $H27Home 'projects') -Recurse -File -Filter "$SessionId.jsonl" -ErrorAction SilentlyContinue |
  Where-Object { $_.Directory.Parent.Name -eq 'projects' } | Select-Object -First 1
if (-not $sessionFile) { Write-Warning "no $SessionId.jsonl under $H27Home\projects - Claude Code will most likely refuse the resume" }

if (-not $Cwd) {
  $recorded = $null
  if ($sessionFile) {
    foreach ($line in [IO.File]::ReadAllLines($sessionFile.FullName)) {
      if ($line -match '"cwd"\s*:') { try { $c = ($line | ConvertFrom-Json).cwd; if ($c) { $recorded = $c } } catch { } }
    }
  }
  $scratchRoot = Join-Path $env:LOCALAPPDATA 'shinatama-verify'
  if ($recorded -and $recorded.StartsWith($scratchRoot, [StringComparison]::OrdinalIgnoreCase)) {
    $Cwd = $recorded
  } else {
    if ($recorded) { Write-Host "session cwd $recorded is outside $scratchRoot; using the scratch work dir instead" }
    $Cwd = $H27Work
  }
}
New-Item -ItemType Directory -Force -Path $Cwd | Out-Null
$linesBefore = if ($sessionFile) { Get-H27LineCount $sessionFile.FullName } else { 0 }

$srv = Start-H27Servers $capture $trapLog
try {
  $r = Invoke-H27Claude @('-p', '--resume', $SessionId, $Prompt, '--output-format', 'json', '--dangerously-skip-permissions') $Cwd $base $TimeoutSec
} finally {
  Stop-H27Servers $srv
}

$after = Get-ChildItem (Join-Path $H27Home 'projects') -Recurse -File -Filter "$SessionId.jsonl" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($after) { Copy-Item $after.FullName "$base.session-after.jsonl" }
$trapCount = Get-H27LineCount $trapLog
if ($trapCount -gt 0) { Copy-Item $trapLog "$base.trap.jsonl"; Write-Warning "Claude Code tried to reach $trapCount real endpoint(s) (blocked): $base.trap.jsonl" }
$meta = [ordered]@{
  session_id          = $SessionId
  returned_session_id = $r.session_id
  session_file        = if ($after) { $after.FullName } else { $null }
  session_lines       = "$linesBefore -> $(if ($after) { Get-H27LineCount $after.FullName } else { 0 })"
  cwd                 = $Cwd
  command             = "claude $($r.args)"
  exit                = $r.exit
  mock_requests       = Get-H27LineCount $capture
  trap_requests       = $trapCount
}
Write-H27Text "$base.meta.json" ($meta | ConvertTo-Json)
$meta.GetEnumerator() | ForEach-Object { '{0,-20} {1}' -f $_.Key, $_.Value } | Write-Host
Write-Host "written to: $outDir ($Tag.*)"
