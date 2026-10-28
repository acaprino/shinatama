# H27 / OpenCode: resume an EXISTING session of the scratch home with one prompt, capturing what the
# model receives.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\opencode\resume-capture.ps1 -SessionId ses_... -Prompt "..." -Tag name
# Never resets anything. Refuses a session id that is not in the scratch DB (`opencode run --session`
# would silently create it). Starts the mock on 4133, runs
#   opencode run --standalone --session <id> --format json --auto   (prompt on stdin, verbatim)
# and writes to verify\H27\out\2026-10-27\opencode\resume\:
#   <Tag>.capture.jsonl  mock capture of this run
#   <Tag>.stdout.txt / .stderr.txt / .exit.txt   the harness's output and exit code
#   <Tag>.rows-before.json / .rows-after.json    read-only dump of the session's rows around the run
param(
  [Parameter(Mandatory = $true)][string]$SessionId,
  [Parameter(Mandatory = $true)][string]$Prompt,
  [Parameter(Mandatory = $true)][string]$Tag,
  [int]$TimeoutSec = 240
)
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot 'env.ps1')

$dir = Join-Path $H27Out 'resume'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$base = Join-Path $dir $Tag
$capture = "$base.capture.jsonl"
Remove-Item "$base.*" -Force -ErrorAction SilentlyContinue

if (-not (Test-Path $H27DbFile)) { throw "no scratch DB at $H27DbFile (run gen-source.ps1 or import a session first)" }
& node --no-warnings (Join-Path $PSScriptRoot 'dump-rows.mjs') $H27DbFile $SessionId "$base.rows-before.json"
if ($LASTEXITCODE -ne 0) { throw "session $SessionId is not in the scratch DB $H27DbFile" }

$realBefore = Get-H27RealStoreFingerprint
$mock = Start-H27Mock $capture
$r = Invoke-H27OpenCode -Arguments @('run', '--standalone', '--session', $SessionId, '--format', 'json', '--auto') -Stdin $Prompt -TimeoutSec $TimeoutSec -OutPrefix $base
Start-Sleep -Seconds 1
Stop-H27Process $mock
& node --no-warnings (Join-Path $PSScriptRoot 'dump-rows.mjs') $H27DbFile $SessionId "$base.rows-after.json"
$realAfter = Get-H27RealStoreFingerprint

$requests = @(Get-Content $capture -ErrorAction SilentlyContinue)
"session     : $SessionId"
"prompt      : $Prompt"
"opencode    : exit=$($r.ExitCode) timedOut=$($r.TimedOut) $($r.Seconds)s"
"capture     : $($requests.Count) request(s)"
foreach ($line in $requests) {
  $j = $line | ConvertFrom-Json
  $roles = @($j.body.messages | ForEach-Object { $_.role }) -join ','
  "  #$($j.n) $($j.method) $($j.url) messages=$(@($j.body.messages).Count) [$roles]"
}
"real store unchanged: $((($realBefore -join '|') -eq ($realAfter -join '|')))"
"written to  : $dir ($Tag.*)"
