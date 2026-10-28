# H27 / OpenCode: generate an authentic native OpenCode 2.1 session against the H27 mock model.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\opencode\gen-source.ps1
# Resets the scratch home (%LOCALAPPDATA%\shinatama-verify\H27\opencode), starts the mock on 4133
# (capture ...\H27\opencode\capture-gen.jsonl), then in ONE session:
#   T1 `opencode run` H27-TOOLS (two parallel reads), T2 resume, compaction (HTTP POST
#   /api/session/:id/compact on a scratch `opencode serve`), T3 resume, T4 H27-HANG killed after 15 s.
# Copies export JSON, DB rows, capture and per-turn stdout/stderr to verify\H27\out\2026-10-27\opencode\source.
param([int]$HangSeconds = 15)
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot 'env.ps1')

$src = Join-Path $H27Out 'source'
$turns = Join-Path $src 'turns'
$capture = Join-Path $H27Root 'capture-gen.jsonl'

# ---------- reset (scratch only) ----------
Clear-H27Port $H27MockPort
Clear-H27Port $H27ServePort
foreach ($d in @('xdg', 'work', 'home', 'tmp', 'probe')) { Remove-Item (Join-Path $H27Root $d) -Recurse -Force -ErrorAction SilentlyContinue }
Get-ChildItem $H27Root -Filter 'capture-gen*' -ErrorAction SilentlyContinue | Remove-Item -Force
Remove-Item $src -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $turns | Out-Null
Initialize-H27OpenCodeHome
$alpha = Join-Path $H27Work 'alpha.txt'
$beta = Join-Path $H27Work 'beta.txt'
[IO.File]::WriteAllText($alpha, 'ALPHA-CONTENT-42', $H27Utf8)
[IO.File]::WriteAllText($beta, 'BETA-CONTENT-17', $H27Utf8)
$realBefore = Get-H27RealStoreFingerprint
"opencode    : $H27Exe ($(& $H27Exe --version 2>&1))"
"scratch     : $H27Root"
"work        : $H27Work"
"capture     : $capture"

$mock = Start-H27Mock $capture
"mock        : pid $($mock.Id) on $H27MockPort"

function Turn($name, $argsList, $prompt, $timeout) {
  $r = Invoke-H27OpenCode -Arguments $argsList -Stdin $prompt -TimeoutSec $timeout -OutPrefix (Join-Path $turns $name)
  Write-Host ("{0,-11} : exit={1} timedOut={2} {3}s  <- {4}" -f $name, $r.ExitCode, $r.TimedOut, $r.Seconds, $prompt)
  $r
}

# ---------- T1: new session, two parallel reads ----------
$t1 = Turn 'T1' @('run', '--standalone', '--model', $H27Model, '--format', 'json', '--auto', '--title', 'H27-opencode-source') "H27-TOOLS read $alpha and $beta" 240
$sid = Get-H27SessionIdFromJson $t1.Stdout
if (-not $sid) { Stop-H27Process $mock; throw 'T1 produced no session id (see source\turns\T1.*)' }
"session id  : $sid"

# ---------- T2: same session, resumed ----------
$null = Turn 'T2' @('run', '--standalone', '--session', $sid, '--format', 'json', '--auto') 'H27-T2 say hello' 240

# ---------- compaction: the session's own compaction over HTTP on a scratch `opencode serve` ----------
$serve = Start-H27Serve (Join-Path $turns 'compact-serve')
"serve       : pid $($serve.Id) on $H27ServePort"
$code = Invoke-H27Http -Method POST -Path "/api/session/$sid/compact" -Body '{}' -OutFile (Join-Path $turns 'compact.response.json')
"compact     : POST /api/session/$sid/compact -> $code"
$code = Invoke-H27Http -Method POST -Path "/api/experimental/session/$sid/wait" -OutFile (Join-Path $turns 'compact-wait.response.json') -TimeoutSec 180
"wait        : POST /api/experimental/session/$sid/wait -> $code"
$code = Invoke-H27Http -Method GET -Path "/api/session/$sid/context" -OutFile (Join-Path $src 'context-after-compaction.json')
"context     : GET /api/session/$sid/context -> $code"
$code = Invoke-H27Http -Method GET -Path "/api/experimental/session/$sid/export" -OutFile (Join-Path $src 'export-after-compaction.json')
"export      : GET /api/experimental/session/$sid/export -> $code"
Stop-H27Process $serve
Start-Sleep -Seconds 1

# ---------- T3: same session after compaction ----------
$null = Turn 'T3' @('run', '--standalone', '--session', $sid, '--format', 'json', '--auto') 'H27-T3 after compaction' 240

# ---------- T4: hang, killed (whole process tree, private server included) ----------
$null = Turn 'T4' @('run', '--standalone', '--session', $sid, '--format', 'json', '--auto') 'H27-HANG' $HangSeconds
Start-Sleep -Seconds 2
$requestsAfterT4 = @(Get-Content $capture -ErrorAction SilentlyContinue).Count

# ---------- read back: rows first (read-only, the state the kill left), then the official export ----------
& node --no-warnings (Join-Path $PSScriptRoot 'dump-rows.mjs') $H27DbFile $sid (Join-Path $src 'rows.json')
$exp = Invoke-H27OpenCode -Arguments @('session', 'export', $sid, '--standalone') -TimeoutSec 180 -OutPrefix (Join-Path $turns 'export')
$raw = $exp.Stdout
$i = $raw.IndexOf('{'); $j = $raw.LastIndexOf('}')
if ($i -ge 0 -and $j -gt $i) { [IO.File]::WriteAllText((Join-Path $src 'session-export.json'), $raw.Substring($i, $j - $i + 1), $H27Utf8) }
"export cli  : exit=$($exp.ExitCode) -> source\session-export.json"
Start-Sleep -Seconds 1
Stop-H27Process $mock
$requestsTotal = @(Get-Content $capture -ErrorAction SilentlyContinue).Count
"capture     : $requestsTotal requests ($requestsAfterT4 after T4; $($requestsTotal - $requestsAfterT4) during read-back)"

Copy-Item $capture (Join-Path $src 'capture-gen.jsonl') -Force
Copy-Item $H27ConfigFile (Join-Path $src 'opencode.json') -Force
Copy-Item (Join-Path $env:XDG_DATA_HOME 'opencode\log') (Join-Path $src 'scratch-log') -Recurse -Force -ErrorAction SilentlyContinue
[IO.File]::WriteAllText((Join-Path $src 'session-id.txt'), $sid, $H27Utf8)

# ---------- proof: scratch only ----------
$realAfter = Get-H27RealStoreFingerprint
$proof = @('real store before:') + $realBefore + @('real store after:') + $realAfter + @("real store unchanged: $((($realBefore -join '|') -eq ($realAfter -join '|')))", '', "scratch root $H27Root :") +
  (Get-ChildItem $H27Root -Recurse -File | Where-Object { $_.FullName -notmatch '\\(tmp|cache|repos)\\' } | ForEach-Object { $_.FullName.Substring($H27Root.Length) })
$realLog = Join-Path $env:USERPROFILE '.local\share\opencode\log\opencode.log'
$proof += "session id in the real log: $([bool](Select-String -Path $realLog -SimpleMatch $sid -Quiet -ErrorAction SilentlyContinue))"
$proof += "outbound URLs in scratch log (non-127.0.0.1): " + ((Select-String -Path (Join-Path $src 'scratch-log\*') -Pattern 'https?://(?!127\.0\.0\.1)[^\s"'']+' -AllMatches -ErrorAction SilentlyContinue | ForEach-Object { $_.Matches.Value } | Sort-Object -Unique) -join ' ')
[IO.File]::WriteAllLines((Join-Path $src 'isolation-proof.txt'), [string[]]$proof, $H27Utf8)
$proof | Select-Object -First 8
"session id  : $sid"
"written to  : $src"
