# H27 / Pi - resume an EXISTING session of the scratch home with one prompt, non-interactively,
# and capture exactly what Pi sends to the mock. Resets nothing.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify/H27/pi/resume-capture.ps1 -SessionId <id> -Prompt <text> -Tag <name>
# The session is found by id anywhere under <scratch home>\sessions and opened by file path.
# Pi runs in the session header's cwd when that folder exists (as `pi --session` would for its own
# project), else in the scratch work folder; -Cwd overrides.
param(
  [Parameter(Mandatory = $true)][string]$SessionId,
  [Parameter(Mandatory = $true)][string]$Prompt,
  [Parameter(Mandatory = $true)][string]$Tag,
  [string]$Cwd,
  [int]$TimeoutSec = 300
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\env.ps1"
. "$PSScriptRoot\lib.ps1"

$file = Find-H27PiSession $SessionId
if (-not $file) { throw "no session $SessionId under $(Join-Path $H27PiHome 'sessions')" }
if (-not $Cwd) {
  $hdr = Get-Content -Path $file.FullName -TotalCount 1 -Encoding UTF8 | ConvertFrom-Json
  $Cwd = if ($hdr.cwd -and (Test-Path -LiteralPath $hdr.cwd -PathType Container)) { $hdr.cwd } else { $H27PiWork }
}
New-Item -ItemType Directory -Force -Path $Cwd | Out-Null

$dir = Join-Path $H27PiOut 'resume'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$base = Join-Path $dir $Tag
$capture = "$base.capture.jsonl"
Copy-Item $file.FullName "$base.session-before.jsonl" -Force

$realBefore = Get-H27RealPiStamp
$mock = Start-H27Mock $capture
try {
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('--session', $file.FullName, $Prompt)) $Cwd -TimeoutSec $TimeoutSec
} finally {
  Stop-H27Mock $mock
}
Save-H27Run $r $base
Copy-Item $file.FullName "$base.session-after.jsonl" -Force
$realAfter = Get-H27RealPiStamp

$n = if (Test-Path $capture) { @(Get-Content $capture).Count } else { 0 }
Write-Host ("session {0}`n  file {1}`n  cwd  {2}" -f $SessionId, $file.FullName, $Cwd)
Write-Host ("exit={0} killed={1} {2}s requests={3}" -f $r.ExitCode, $r.Killed, $r.Seconds, $n)
Write-Host ("stdout: {0}" -f $r.Stdout.Trim())
if ($realAfter -ne $realBefore) { Write-Warning "real pi store changed: $realBefore -> $realAfter" }
Write-Host "wrote $base.{capture.jsonl,stdout.txt,stderr.txt,exit.txt,session-before.jsonl,session-after.jsonl}"
exit $(if ($null -eq $r.ExitCode) { 124 } else { $r.ExitCode })
