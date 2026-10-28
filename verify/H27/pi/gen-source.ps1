# H27 / Pi - generate one authentic Pi session against the mock: T1 tools, T2, compaction, T3, T4 hang.
# Resets the scratch home first. Rerunnable without edits:
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify/H27/pi/gen-source.ps1
param([int]$HangSeconds = 15)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\env.ps1"
. "$PSScriptRoot\lib.ps1"

$realBefore = Get-H27RealPiStamp

# Reset the scratch home and work folder (scratch only).
foreach ($d in $H27PiHome, $H27PiWork) { if (Test-Path $d) { Remove-Item -Recurse -Force $d } }
Initialize-H27PiHome
New-Item -ItemType Directory -Force -Path $H27PiWork | Out-Null
$alpha = Join-Path $H27PiWork 'alpha.txt'
$beta = Join-Path $H27PiWork 'beta.txt'
Write-H27Utf8 $alpha 'ALPHA-CONTENT-42'
Write-H27Utf8 $beta 'BETA-CONTENT-17'

$src = Join-Path $H27PiOut 'source'
if (Test-Path $src) { Remove-Item -Recurse -Force $src }
New-Item -ItemType Directory -Force -Path $src | Out-Null
$capture = Join-Path $H27PiRoot 'capture-gen.jsonl'
$mock = Start-H27Mock $capture
$log = New-Object Collections.Generic.List[string]
function Note([string]$s) { $log.Add($s); Write-Host $s }

try {
  # T1: new session, two parallel reads.
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @("H27-TOOLS read $alpha and $beta")) $H27PiWork
  Save-H27Run $r (Join-Path $src 'T1'); Note ("T1 exit={0} {1}s stdout={2}" -f $r.ExitCode, $r.Seconds, $r.Stdout.Trim())
  $file = Get-ChildItem -Path (Join-Path $H27PiHome 'sessions') -Recurse -File -Filter '*.jsonl' | Select-Object -First 1
  if (-not $file) { throw 'T1 wrote no session file' }
  $sid = (Get-Content -Path $file.FullName -TotalCount 1 | ConvertFrom-Json).id
  Note "session $sid -> $($file.FullName)"

  # T2: same session, resumed by id.
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('--session', $sid, 'H27-T2 say hello')) $H27PiWork
  Save-H27Run $r (Join-Path $src 'T2'); Note ("T2 exit={0} {1}s stdout={2}" -f $r.ExitCode, $r.Seconds, $r.Stdout.Trim())

  # Compaction: print mode sends "/compact" to the model as plain text, so use RPC `compact`.
  $r = Invoke-H27PiRpc ($H27PiFlags + @('--session', $sid)) $H27PiWork @(
    @{ Send = '{"id":"c1","type":"compact"}'; WaitFor = '"id":"c1".*"type":"response"|"type":"response".*"id":"c1"' }
  )
  Save-H27Run $r (Join-Path $src 'compact-rpc'); Note ("compact exit={0} completed={1}" -f $r.ExitCode, $r.Completed)
  if (-not $r.Completed -or -not (Select-String -Path (Find-H27PiSession $sid).FullName -Pattern '"type":"compaction"' -Quiet)) {
    throw "compaction did not complete; see $src\compact-rpc.stdout.txt"
  }

  # T3: same session, after compaction.
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('--session', $sid, 'H27-T3 after compaction')) $H27PiWork
  Save-H27Run $r (Join-Path $src 'T3'); Note ("T3 exit={0} {1}s stdout={2}" -f $r.ExitCode, $r.Seconds, $r.Stdout.Trim())

  # T4: hanging turn, killed after $HangSeconds (hard kill of the process tree).
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('--session', $sid, 'H27-HANG interrupted turn')) $H27PiWork -TimeoutSec $HangSeconds
  Save-H27Run $r (Join-Path $src 'T4'); Note ("T4 killed={0} after {1}s" -f $r.Killed, $r.Seconds)

  # Side variant, not part of the source session: a graceful interruption (RPC `abort`) in a
  # fork of it, to see what Pi itself writes for an aborted turn and a forked session header.
  $before = @(Get-ChildItem -Path (Join-Path $H27PiHome 'sessions') -Recurse -File -Filter '*.jsonl' | ForEach-Object FullName)
  $r = Invoke-H27PiRpc ($H27PiFlags + @('--fork', $sid)) $H27PiWork @(
    @{ Send = '{"id":"p1","type":"prompt","message":"H27-HANG aborted turn"}'; WaitFor = 'H27-PARTIAL' },
    @{ Send = '{"id":"a1","type":"abort"}'; WaitFor = '"command":"abort"'; Delay = $HangSeconds }
  )
  $fork = Get-ChildItem -Path (Join-Path $H27PiHome 'sessions') -Recurse -File -Filter '*.jsonl' | Where-Object { $before -notcontains $_.FullName } | Select-Object -First 1
  $av = Join-Path $src 'abort-variant'
  New-Item -ItemType Directory -Force -Path $av | Out-Null
  Save-H27Run $r (Join-Path $av 'abort-rpc'); Note ("abort variant exit={0} completed={1} fork={2}" -f $r.ExitCode, $r.Completed, $(if ($fork) { $fork.Name } else { 'none' }))
  if ($fork) { Copy-Item $fork.FullName (Join-Path $av $fork.Name) }
} finally {
  Stop-H27Mock $mock
}

$file = Find-H27PiSession $sid
Copy-Item $file.FullName (Join-Path $src $file.Name)
Copy-Item $capture (Join-Path $src 'capture-gen.jsonl')
$realAfter = Get-H27RealPiStamp
Note "real pi store newest file before: $realBefore"
Note "real pi store newest file after:  $realAfter"
Note ("scratch home files:`n  " + ((Get-ChildItem -Recurse -File $H27PiHome | ForEach-Object { $_.FullName }) -join "`n  "))
Note "wrote $src"
Note "SESSION_ID=$sid"
Write-H27Utf8 (Join-Path $src 'gen-source.log.txt') (($log -join "`n") + "`n")
Write-H27Utf8 (Join-Path $src 'session-id.txt') "$sid`n"
