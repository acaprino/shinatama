# H27 / OpenCode - dot-source me: . .\verify\H27\opencode\env.ps1
# Points OpenCode 2.1.x at a scratch home and at the H27 mock model (Anthropic Messages on 127.0.0.1:4133).
# Only this PowerShell process (and its children) is affected:
#   - OPENCODE_DB + XDG_* + OPENCODE_TEST_HOME move every OpenCode store into the scratch root;
#   - provider API keys are removed from the environment; the only key is the literal h27-dummy;
#   - HTTP(S)_PROXY points at a dead local port, so any call to a real endpoint fails fast
#     (NO_PROXY keeps 127.0.0.1 direct, i.e. the mock and OpenCode's own local server);
#   - models.dev fetch, project config and ~/.claude skill discovery are disabled.
# Always pass --standalone or --server to opencode: the default "background service" was started
# with the real environment and would use the real home.

$H27Harness    = 'opencode'
$H27Root       = Join-Path $env:LOCALAPPDATA 'shinatama-verify\H27\opencode'
$H27MockPort   = 4133
$H27ServePort  = 43133
$H27Model      = 'anthropic/h27-mock'
$H27Repo       = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
# Local copy of verify\H27\mock\server.mjs: OpenCode 2.1.4 fails a compaction whose summary lacks its
# template headings, and the shared mock took "H27-T3 after compaction" for a summary request.
$H27Mock       = Join-Path $PSScriptRoot 'server.local.mjs'
$H27Out        = Join-Path $H27Repo 'verify\H27\out\2026-10-27\opencode'
$H27Work       = Join-Path $H27Root 'work'
$H27Xdg        = Join-Path $H27Root 'xdg'
$H27ConfigFile = Join-Path $H27Xdg 'config\opencode\opencode.json'
$H27DbFile     = Join-Path $H27Xdg 'data\opencode\opencode.db'
$H27ServerPassword = 'h27-opencode-local'
# The npm shim (opencode.ps1/.cmd) is bypassed: the binary is called directly.
$H27Exe = Join-Path $env:APPDATA 'npm\node_modules\@opencode\cli\bin\opencode.exe'
if (-not (Test-Path $H27Exe)) {
  $shim = Get-Command opencode.cmd -ErrorAction SilentlyContinue
  if ($shim) { $H27Exe = Join-Path (Split-Path $shim.Source) 'node_modules\@opencode\cli\bin\opencode.exe' }
}
$H27Utf8 = New-Object System.Text.UTF8Encoding($false)

# Real provider credentials never reach OpenCode.
Get-ChildItem env: | Where-Object {
  $_.Name -match '(_API_KEY|_AUTH_TOKEN|_ACCESS_TOKEN)$' -or $_.Name -match '^(OPENROUTER_|AWS_|AZURE_|GOOGLE_|VERTEX|ANTHROPIC_|OPENAI_|OPENCODE_)'
} | ForEach-Object { Remove-Item -LiteralPath ("env:" + $_.Name) -ErrorAction SilentlyContinue }
# `opencode run` takes its directory from $PWD before the process cwd (Git Bash exports a POSIX PWD).
Remove-Item env:PWD -ErrorAction SilentlyContinue

$env:ANTHROPIC_API_KEY = 'h27-dummy'
$env:OPENCODE_DB = $H27DbFile
$env:XDG_DATA_HOME = Join-Path $H27Xdg 'data'
$env:XDG_CONFIG_HOME = Join-Path $H27Xdg 'config'
$env:XDG_STATE_HOME = Join-Path $H27Xdg 'state'
$env:XDG_CACHE_HOME = Join-Path $H27Xdg 'cache'
$env:OPENCODE_TEST_HOME = Join-Path $H27Root 'home'
$env:OPENCODE_CONFIG = $H27ConfigFile
$env:OPENCODE_DISABLE_MODELS_FETCH = '1'
$env:OPENCODE_DISABLE_PROJECT_CONFIG = '1'
$env:OPENCODE_DISABLE_CLAUDE_CODE = '1'
$env:OPENCODE_DISABLE_AUTOUPDATE = '1'
$env:OPENCODE_PASSWORD = $H27ServerPassword
$env:TEMP = Join-Path $H27Root 'tmp'
$env:TMP = $env:TEMP
$env:HTTP_PROXY = 'http://127.0.0.1:9'
$env:HTTPS_PROXY = 'http://127.0.0.1:9'
$env:http_proxy = $env:HTTP_PROXY
$env:https_proxy = $env:HTTPS_PROXY
$env:NO_PROXY = '127.0.0.1,localhost'
$env:no_proxy = $env:NO_PROXY

function Initialize-H27OpenCodeHome {
  foreach ($d in @($H27Root, $H27Work, (Split-Path $H27ConfigFile), (Split-Path $H27DbFile), $env:XDG_STATE_HOME, $env:XDG_CACHE_HOME, $env:OPENCODE_TEST_HOME, $env:TEMP)) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
  }
  # OpenCode 2.x config: "providers" (plural) with "settings"; anthropic/h27-mock is declared here,
  # so no catalog download is needed. Rewritten on every dot-source (same content), never deleted.
  $config = [ordered]@{
    model     = $H27Model
    update    = 'disable'
    share     = 'disabled'
    snapshots = $false
    providers = [ordered]@{
      anthropic = [ordered]@{
        settings = [ordered]@{ baseURL = "http://127.0.0.1:$H27MockPort/v1"; apiKey = 'h27-dummy' }
        models   = [ordered]@{
          'h27-mock' = [ordered]@{
            name         = 'H27 mock'
            capabilities = [ordered]@{ tools = $true; reasoning = [ordered]@{ supported = $true } }
            limit        = [ordered]@{ context = 200000; output = 8192 }
          }
        }
      }
    }
  }
  [IO.File]::WriteAllText($H27ConfigFile, ($config | ConvertTo-Json -Depth 20), $H27Utf8)
}

function ConvertTo-H27CommandLine([string[]]$Arguments) {
  ($Arguments | ForEach-Object {
    if ($_ -eq '') { '""' }
    elseif ($_ -notmatch '[\s"]') { $_ }
    else { '"' + (($_ -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"' }
  }) -join ' '
}

# Runs opencode.exe with the prompt on stdin (no trailing newline): `opencode run` quotes every
# positional argument that contains a space, so stdin is the only way to send a prompt verbatim.
# Kills the whole process tree (the CLI spawns `opencode serve --stdio`) when the timeout expires.
function Invoke-H27OpenCode {
  param([string[]]$Arguments, [string]$Stdin = '', [string]$WorkDir = $H27Work, [int]$TimeoutSec = 240, [string]$OutPrefix)
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $H27Exe
  $psi.Arguments = ConvertTo-H27CommandLine $Arguments
  $psi.WorkingDirectory = $WorkDir
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.StandardOutputEncoding = $H27Utf8
  $psi.StandardErrorEncoding = $H27Utf8
  $started = Get-Date
  $p = [System.Diagnostics.Process]::Start($psi)
  $outTask = $p.StandardOutput.ReadToEndAsync()
  $errTask = $p.StandardError.ReadToEndAsync()
  $bytes = $H27Utf8.GetBytes($Stdin)
  $p.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
  $p.StandardInput.BaseStream.Flush()
  $p.StandardInput.Close()
  $timedOut = -not $p.WaitForExit($TimeoutSec * 1000)
  if ($timedOut) {
    & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null
    $p.WaitForExit(15000) | Out-Null
  } else { $p.WaitForExit() }
  $null = $outTask.Wait(15000); $null = $errTask.Wait(15000)
  $r = [pscustomobject]@{
    ExitCode = $(if ($p.HasExited) { $p.ExitCode } else { 'running' })
    TimedOut = $timedOut
    Seconds  = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
    Stdout   = $(if ($outTask.IsCompleted) { $outTask.Result } else { '' })
    Stderr   = $(if ($errTask.IsCompleted) { $errTask.Result } else { '' })
    CommandLine = 'opencode ' + $psi.Arguments
  }
  if ($OutPrefix) {
    [IO.File]::WriteAllText("$OutPrefix.stdout.txt", $r.Stdout, $H27Utf8)
    [IO.File]::WriteAllText("$OutPrefix.stderr.txt", $r.Stderr, $H27Utf8)
    $meta = "exit=$($r.ExitCode)`ntimedOut=$($r.TimedOut)`nseconds=$($r.Seconds)`ncwd=$WorkDir`ncommand=$($r.CommandLine)`nstdin=$Stdin`n"
    [IO.File]::WriteAllText("$OutPrefix.exit.txt", $meta, $H27Utf8)
  }
  $r
}

function Get-H27SessionIdFromJson([string]$Text) {
  $m = [regex]::Match($Text, '"sessionID":"(ses_[A-Za-z0-9]+)"')
  if ($m.Success) { $m.Groups[1].Value } else { $null }
}

function Wait-H27Port([int]$Port, [int]$Seconds) {
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { return $true }
    Start-Sleep -Milliseconds 300
  }
  $false
}

# Frees a port held by a leftover H27 process of ours (mock node or scratch opencode serve); refuses otherwise.
function Clear-H27Port([int]$Port) {
  foreach ($c in @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)) {
    $proc = Get-CimInstance Win32_Process -Filter "ProcessId = $($c.OwningProcess)" -ErrorAction SilentlyContinue
    $cmd = "$($proc.CommandLine)"
    if ($cmd -match 'server\.mjs' -or ($cmd -match 'opencode' -and $cmd -match "--port\s+$Port")) {
      & taskkill.exe /PID $c.OwningProcess /T /F 2>&1 | Out-Null
    } else {
      throw "port $Port is held by another process: $cmd"
    }
  }
  Start-Sleep -Milliseconds 300
}

function Start-H27Mock([string]$Capture) {
  Clear-H27Port $H27MockPort
  $base = [IO.Path]::ChangeExtension($Capture, $null).TrimEnd('.')
  $p = Start-Process node -ArgumentList @("`"$H27Mock`"", '--port', $H27MockPort, '--capture', "`"$Capture`"") -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput "$base.mock-stdout.txt" -RedirectStandardError "$base.mock-stderr.txt"
  if (-not (Wait-H27Port $H27MockPort 20)) { throw "mock did not listen on $H27MockPort" }
  $p
}

function Stop-H27Process($Process) {
  if ($Process -and -not $Process.HasExited) { & taskkill.exe /PID $Process.Id /T /F 2>&1 | Out-Null }
}

function Invoke-H27Http([string]$Method, [string]$Path, [string]$Body, [string]$OutFile, [int]$TimeoutSec = 120) {
  $url = "http://127.0.0.1:$H27ServePort$Path"
  $curl = @('-s', '--noproxy', '*', '-m', "$TimeoutSec", '-X', $Method, '-H', (Get-H27Auth), '-o', $OutFile, '-w', '%{http_code}')
  if ($PSBoundParameters.ContainsKey('Body')) {
    $bf = Join-Path $env:TEMP ('h27-' + [guid]::NewGuid().ToString('N') + '.json')
    [IO.File]::WriteAllText($bf, $Body, $H27Utf8)
    $curl += @('-H', 'Content-Type: application/json', '--data-binary', "@$bf")
  }
  $code = (& curl.exe @curl $url) 2>$null
  if ($bf) { Remove-Item $bf -Force -ErrorAction SilentlyContinue }
  "$code"
}

function Start-H27Serve([string]$LogBase) {
  Clear-H27Port $H27ServePort
  $p = Start-Process $H27Exe -ArgumentList @('serve', '--hostname', '127.0.0.1', '--port', $H27ServePort) -WorkingDirectory $H27Work -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput "$LogBase.stdout.txt" -RedirectStandardError "$LogBase.stderr.txt"
  $deadline = (Get-Date).AddSeconds(90)
  $probe = Join-Path $env:TEMP 'h27-serve-probe.json'
  while ((Get-Date) -lt $deadline) {
    if ((Invoke-H27Http GET '/api/session' -OutFile $probe -TimeoutSec 5) -eq '200') { return $p }
    Start-Sleep -Milliseconds 700
  }
  Stop-H27Process $p
  throw "opencode serve did not answer on $H27ServePort"
}

function Get-H27Auth {
  'Authorization: Basic ' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("opencode:$H27ServerPassword"))
}

# The real OpenCode store (never written by these scripts): size and mtime of the DB and WAL files
# (-shm is left out: any reader of the real DB, e.g. the user's background service, touches it).
function Get-H27RealStoreFingerprint {
  $real = Join-Path $env:USERPROFILE '.local\share\opencode'
  Get-ChildItem -Path $real -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^opencode.*\.db(-wal)?$' } | Sort-Object Name |
    ForEach-Object { '{0} {1} {2:o}' -f $_.Name, $_.Length, $_.LastWriteTimeUtc }
}

Initialize-H27OpenCodeHome
