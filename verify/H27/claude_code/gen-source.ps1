# H27 / Claude Code - generates the authentic source session against the mock model.
# Resets the scratch home, then in ONE session: T1 two parallel reads, T2 resumed, the native
# /compact, T3, T4 H27-HANG killed after 15 s; then one --fork-session of it. Copies the
# session file(s), the mock capture and per-step outputs to
# verify/H27/out/<Date>/claude_code/source/. Rerunnable without edits.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\claude_code\gen-source.ps1
param([string]$Date = '2026-10-27', [int]$HangSec = 15)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')

$outDir = Join-Path $H27Repo "verify\H27\out\$Date\claude_code\source"
$stepDir = Join-Path $outDir 'steps'
$capture = Join-Path $H27Scratch 'capture-gen.jsonl'
$trapLog = Join-Path $H27Scratch 'trap-gen.jsonl'

# Reset: only ever the scratch folder of this harness.
if ($H27Scratch -notlike '*\shinatama-verify\H27\claude_code') { throw "refusing to reset unexpected path $H27Scratch" }
if (Test-Path $H27Scratch) { Remove-Item -Recurse -Force $H27Scratch }
if (Test-Path $outDir) { Remove-Item -Recurse -Force $outDir }
New-Item -ItemType Directory -Force -Path $H27Scratch, $stepDir | Out-Null
Initialize-H27Home
$alpha = Join-Path $H27Work 'alpha.txt'
$beta = Join-Path $H27Work 'beta.txt'
[IO.File]::WriteAllText($alpha, 'ALPHA-CONTENT-42')
[IO.File]::WriteAllText($beta, 'BETA-CONTENT-17')

$steps = New-Object System.Collections.ArrayList
function Step([string]$Name, [string[]]$ClaudeArgs, [int]$KillAfterSec = 0) {
  $from = (Get-H27LineCount $capture) + 1
  $r = Invoke-H27Claude $ClaudeArgs $H27Work (Join-Path $stepDir $Name) 300 $KillAfterSec
  $to = Get-H27LineCount $capture
  $r | Add-Member -NotePropertyName step -NotePropertyValue $Name
  $r | Add-Member -NotePropertyName capture_lines -NotePropertyValue "$from-$to"
  $r.stdout = Split-Path -Leaf $r.stdout; $r.stderr = Split-Path -Leaf $r.stderr
  [void]$steps.Add($r)
  "{0,-8} exit={1,-6} session={2} capture={3}" -f $Name, $r.exit, $r.session_id, $r.capture_lines | Write-Host
  $r
}

$common = @('--output-format', 'json', '--dangerously-skip-permissions')
$srv = Start-H27Servers $capture $trapLog
try {
  $t1 = Step 'T1' (@('-p', "H27-TOOLS read $alpha and $beta") + $common)
  $sid = $t1.session_id
  if (-not $sid) { throw "T1 returned no session_id (see $stepDir\T1.stdout.txt / T1.stderr.txt)" }
  $t2 = Step 'T2' (@('-p', '--resume', $sid, 'H27-T2 say hello') + $common)
  $cp = Step 'compact' (@('-p', '--resume', $sid, '/compact') + $common)
  $t3 = Step 'T3' (@('-p', '--resume', $sid, 'H27-T3 after compaction') + $common)
  $t4 = Step 'T4' @('-p', '--resume', $sid, 'H27-HANG', '--output-format', 'stream-json', '--verbose', '--dangerously-skip-permissions') $HangSec
  $fk = Step 'fork' (@('-p', '--resume', $sid, '--fork-session', 'H27-FORK say hi from the fork') + $common)
} finally {
  Stop-H27Servers $srv
}

foreach ($s in $t2, $cp, $t3) {
  if ($s.session_id -and $s.session_id -ne $sid) { Write-Warning "step $($s.step) reported session $($s.session_id), not $sid" }
}

# Copy the native session(s) and everything around them.
$projDir = Get-ChildItem (Join-Path $H27Home 'projects') -Directory | Where-Object { Test-Path (Join-Path $_.FullName "$sid.jsonl") } | Select-Object -First 1
Copy-Item (Join-Path $projDir.FullName "$sid.jsonl") (Join-Path $outDir "$sid.jsonl")
if ($fk.session_id -and $fk.session_id -ne $sid) {
  Copy-Item (Join-Path $projDir.FullName "$($fk.session_id).jsonl") (Join-Path $outDir "fork-$($fk.session_id).jsonl") -ErrorAction Continue
}
Copy-Item $projDir.FullName (Join-Path $outDir 'project-dir') -Recurse
Copy-Item $capture (Join-Path $outDir 'capture-gen.jsonl')
if (Test-Path $trapLog) { Copy-Item $trapLog (Join-Path $outDir 'trap-gen.jsonl') }
Write-H27Text (Join-Path $outDir 'steps.json') ($steps | ConvertTo-Json -Depth 4)
Write-H27Text (Join-Path $outDir 'scratch-home-tree.txt') ((Get-ChildItem -Recurse -Force $H27Home | ForEach-Object {
  '{0,10} {1:yyyy-MM-ddTHH:mm:ss} {2}' -f $(if ($_.PSIsContainer) { '<dir>' } else { $_.Length }), $_.LastWriteTime, $_.FullName.Substring($H27Home.Length) }) -join "`r`n")

# Hard rule 1 and 2 evidence: nothing of this run in the real home, nothing sent to a real endpoint.
$realProjects = Join-Path $env:USERPROFILE '.claude\projects'
$leak = @(Get-ChildItem $realProjects -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -like '*shinatama-verify*' }) +
        @(Get-ChildItem $realProjects -Recurse -File -Filter "$sid.jsonl" -ErrorAction SilentlyContinue)
# The real global config would gain a projects entry for the work dir if it had been used.
$realCfg = Join-Path $env:USERPROFILE '.claude.json'
$cfgLeak = (Test-Path $realCfg) -and [bool](Select-String -Path $realCfg -SimpleMatch -Quiet -Pattern 'shinatama-verify/H27/claude_code', 'shinatama-verify\\H27\\claude_code')
if ($cfgLeak) { $leak += $realCfg }
$trapCount = Get-H27LineCount $trapLog
$check = @(
  "real home projects dir: $realProjects",
  "  entries for the scratch work dir or session ${sid}: $($leak.Count - [int]$cfgLeak)",
  "real global config $realCfg mentions the scratch work dir: $cfgLeak",
  "trap proxy (blocked real-endpoint requests): $trapCount",
  "mock requests captured: $(Get-H27LineCount $capture)"
)
Write-H27Text (Join-Path $outDir 'isolation-check.txt') ($check -join "`r`n")
$check | Write-Host
if ($leak.Count -gt 0) { Write-Warning 'session data found in the REAL home - stop and report' }
if ($trapCount -gt 0) { Write-Warning "requests to real endpoints were attempted (blocked): see $trapLog" }

Write-Host "scratch home : $H27Home"
Write-Host "source copied: $outDir"
Write-Host "fork session : $($fk.session_id)"
Write-Host "SESSION ID   : $sid"
