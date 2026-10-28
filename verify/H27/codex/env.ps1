# H27 codex leg - environment. Dot-source it:  . "$PSScriptRoot\env.ps1"
# Points codex-cli at the H27 mock (127.0.0.1:4132, OpenAI Responses) through a scratch CODEX_HOME.
# Never uses the real ~/.codex: CODEX_HOME, USERPROFILE and HOME all move under the scratch root, the
# only API key is the literal h27-dummy, and HTTP(S)_PROXY routes any other egress to a local sink
# that logs and refuses it. Also defines the helpers shared by gen-source.ps1 and resume-capture.ps1.

$ErrorActionPreference = 'Stop'

$H27Date    = if ($env:H27_DATE) { $env:H27_DATE } else { '2026-10-27' }
$H27Port    = 4132
$H27Sink    = 4142
$H27Dir     = $PSScriptRoot
$H27Repo    = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$H27Out     = Join-Path $H27Repo "verify\H27\out\$H27Date\codex"
$H27Scratch = Join-Path $env:LOCALAPPDATA 'shinatama-verify\H27\codex'
$H27Home    = Join-Path $H27Scratch 'home'
$H27Profile = Join-Path $H27Scratch 'profile'
$H27Work    = Join-Path $H27Scratch 'work'
$H27Logs    = Join-Path $H27Scratch 'logs'

# The real profile is remembered once so a second dot-source does not mistake the scratch one for it.
if (-not $env:H27_REAL_USERPROFILE) { $env:H27_REAL_USERPROFILE = $env:USERPROFILE }
$H27RealCodexHome = Join-Path $env:H27_REAL_USERPROFILE '.codex'

$H27Codex = 'C:\Users\alfio\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe'
if (-not (Test-Path $H27Codex)) { $H27Codex = (Get-Command codex -CommandType Application | Select-Object -First 1).Source }
$H27Mock = Join-Path $H27Dir 'server.local.mjs'
if (-not (Test-Path $H27Mock)) { $H27Mock = Join-Path $H27Repo 'verify\H27\mock\server.mjs' }

$env:CODEX_HOME  = $H27Home
$env:USERPROFILE = $H27Profile
$env:HOME        = $H27Profile
$env:H27_API_KEY = 'h27-dummy'
foreach ($k in 'OPENAI_API_KEY', 'CODEX_API_KEY', 'OPENAI_BASE_URL', 'CODEX_SQLITE_HOME') { Remove-Item "Env:$k" -ErrorAction SilentlyContinue }
$env:HTTP_PROXY  = "http://127.0.0.1:$H27Sink"
$env:HTTPS_PROXY = "http://127.0.0.1:$H27Sink"
$env:ALL_PROXY   = "http://127.0.0.1:$H27Sink"
$env:NO_PROXY    = '127.0.0.1,localhost'

function Write-H27Config {
    # Writes config.toml into the scratch CODEX_HOME (never the real one). Leaves an existing file alone
    # unless -Force, so resume-capture.ps1 never rewrites a home it did not create.
    param([switch]$Force)
    foreach ($d in $H27Home, $H27Profile, $H27Work, $H27Logs) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
    $cfg = Join-Path $H27Home 'config.toml'
    if ((Test-Path $cfg) -and -not $Force) { return $cfg }
    @"
# H27 codex leg: every model request goes to the local mock; nothing here is a real account.
model = "h27-mock"
model_provider = "h27"
approval_policy = "never"
sandbox_mode = "danger-full-access"
check_for_update_on_startup = false

[model_providers.h27]
name = "H27 mock"
base_url = "http://127.0.0.1:$H27Port/v1"
wire_api = "responses"
env_key = "H27_API_KEY"

[analytics]
enabled = false

# With plugins on, every codex process tries CONNECT chatgpt.com:443 (x2), github.com:443 and
# api.github.com:443 (plugin sync); off, it tries nothing outside 127.0.0.1.
[features]
plugins = false
"@ | Set-Content -Path $cfg -Encoding ASCII
    return $cfg
}

function Test-H27Port([int]$Port) {
    $c = New-Object System.Net.Sockets.TcpClient
    try { $c.Connect('127.0.0.1', $Port); return $true } catch { return $false } finally { $c.Close() }
}

function Start-H27Mock {
    # Starts the mock (capture to $Capture) and the egress sink; returns both processes.
    param([Parameter(Mandatory)][string]$Capture, [Parameter(Mandatory)][string]$EgressLog)
    foreach ($p in $H27Port, $H27Sink) { if (Test-H27Port $p) { throw "port $p is already in use (a leftover mock?); stop it and rerun" } }
    New-Item -ItemType Directory -Force -Path (Split-Path $Capture), (Split-Path $EgressLog), $H27Logs | Out-Null
    New-Item -ItemType File -Force -Path $EgressLog | Out-Null  # stays empty when nothing tries to leave
    $tag = [IO.Path]::GetFileNameWithoutExtension($Capture)
    $mock = Start-Process node -ArgumentList "`"$H27Mock`" --port $H27Port --capture `"$Capture`"" -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $H27Logs "mock-$tag.out.txt") -RedirectStandardError (Join-Path $H27Logs "mock-$tag.err.txt")
    $sink = Start-Process node -ArgumentList "`"$(Join-Path $H27Dir 'egress-sink.mjs')`" --port $H27Sink --log `"$EgressLog`"" -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $H27Logs "sink-$tag.out.txt") -RedirectStandardError (Join-Path $H27Logs "sink-$tag.err.txt")
    for ($i = 0; $i -lt 50 -and -not ((Test-H27Port $H27Port) -and (Test-H27Port $H27Sink)); $i++) { Start-Sleep -Milliseconds 200 }
    if (-not (Test-H27Port $H27Port)) { throw "mock did not start on port $H27Port (see $H27Logs)" }
    "mock: $H27Mock on 127.0.0.1:$H27Port, capture $Capture"
    "egress sink: 127.0.0.1:$H27Sink, log $EgressLog"
    $script:H27Procs = @($mock, $sink)
}

function Stop-H27Mock {
    foreach ($p in $script:H27Procs) { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    $script:H27Procs = @()
}

function ConvertTo-H27ArgLine([string[]]$ArgList) {
    # Windows (MSVC / Rust std) argument quoting, so prompts and paths reach codex.exe intact.
    ($ArgList | ForEach-Object {
        if ($_ -eq '') { '""' }
        elseif ($_ -notmatch '[\s"]') { $_ }
        else { '"' + (($_ -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"' }
    }) -join ' '
}

function Invoke-H27Codex {
    # Runs codex.exe with stdout/stderr to "$OutPrefix.stdout.jsonl" / ".stderr.txt" and the exit code to
    # ".exit.txt". -KillAfterSec kills the process tree after that many seconds (interrupted turn).
    param([Parameter(Mandatory)][string[]]$ArgList, [Parameter(Mandatory)][string]$OutPrefix,
          [string]$Cwd = $H27Work, [int]$TimeoutSec = 300, [int]$KillAfterSec = 0)
    $empty = Join-Path $H27Logs 'empty-stdin.txt'
    if (-not (Test-Path $empty)) { New-Item -ItemType File -Path $empty | Out-Null }
    $line = ConvertTo-H27ArgLine $ArgList
    $p = Start-Process -FilePath $H27Codex -ArgumentList $line -WorkingDirectory $Cwd -PassThru -NoNewWindow `
        -RedirectStandardInput $empty -RedirectStandardOutput "$OutPrefix.stdout.jsonl" -RedirectStandardError "$OutPrefix.stderr.txt"
    $null = $p.Handle  # PS 5.1: keeps ExitCode readable after exit
    $limit = if ($KillAfterSec -gt 0) { $KillAfterSec } else { $TimeoutSec }
    $killed = $false
    if (-not $p.WaitForExit($limit * 1000)) {
        & taskkill.exe /T /F /PID $p.Id | Out-Null
        $p.WaitForExit(10000) | Out-Null
        $killed = $true
    }
    $code = if ($killed) { "killed after ${limit}s" } else { "$($p.ExitCode)" }
    "codex $line" | Set-Content -Path "$OutPrefix.cmd.txt" -Encoding UTF8
    $code | Set-Content -Path "$OutPrefix.exit.txt" -Encoding ASCII
    [pscustomobject]@{ Exit = $code; Killed = $killed; Stdout = "$OutPrefix.stdout.jsonl"; Stderr = "$OutPrefix.stderr.txt" }
}

function Get-H27ThreadId([string]$StdoutFile) {
    foreach ($l in (Get-Content $StdoutFile -ErrorAction SilentlyContinue)) {
        if ($l -match '"type":"thread\.started"' -and $l -match '"thread_id":"([^"]+)"') { return $Matches[1] }
    }
    return $null
}

function Find-H27Rollout([string]$SessionId) {
    $dir = Join-Path $H27Home 'sessions'
    if (-not (Test-Path $dir)) { return $null }
    Get-ChildItem $dir -Recurse -File -Filter "rollout-*$SessionId.jsonl" | Select-Object -First 1
}

function Get-H27RealStoreStamp {
    # Names and mtimes only (never contents) of the real Codex store, to prove the runs left it alone.
    $idx = Join-Path $H27RealCodexHome 'session_index.jsonl'
    $today = Join-Path $H27RealCodexHome ("sessions\" + (Get-Date -Format 'yyyy\\MM\\dd'))
    $newest = if (Test-Path $today) { Get-ChildItem $today -File | Sort-Object LastWriteTime | Select-Object -Last 1 } else { $null }
    [pscustomobject]@{
        IndexMtime = if (Test-Path $idx) { (Get-Item $idx).LastWriteTime.ToString('o') } else { '<none>' }
        TodayCount = if (Test-Path $today) { @(Get-ChildItem $today -File).Count } else { 0 }
        Newest     = if ($newest) { "$($newest.Name) @ $($newest.LastWriteTime.ToString('o'))" } else { '<none>' }
    }
}
