# H27 / Claude Code - dot-source this file (`. .\env.ps1`). It points Claude Code at a scratch
# home and at the local mock model. Nothing here reads or writes the real ~/.claude.
#   $H27Scratch  %LOCALAPPDATA%\shinatama-verify\H27\claude_code   (everything the runs write)
#   $H27Home     <scratch>\home      CLAUDE_CONFIG_DIR (settings, .claude.json, projects\, ...)
#   $H27Work     <scratch>\work      cwd of the source session (alpha.txt, beta.txt)
#   $H27Port     4131                mock model (ANTHROPIC_BASE_URL)
#   $H27TrapPort 4191                trap proxy: HTTPS_PROXY points here, so any https request
#                                    to a real endpoint is refused and logged instead of sent.
$H27Scratch  = Join-Path $env:LOCALAPPDATA 'shinatama-verify\H27\claude_code'
$H27Home     = Join-Path $H27Scratch 'home'
$H27Work     = Join-Path $H27Scratch 'work'
$H27Port     = 4131
$H27TrapPort = 4191
$H27Dir      = $PSScriptRoot
$H27Repo     = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
# Local copy of verify/H27/mock/server.mjs: Claude Code puts a role "system" message after the
# user turn, which the shared mock reads as the last turn (see the diff in FINDINGS.md).
$H27Mock     = Join-Path $PSScriptRoot 'server.local.mjs'

# Variables inherited from a parent Claude Code session would make the child think it is a
# sub-session of it (session id, messaging socket, entrypoint); drop them all.
foreach ($n in @(Get-ChildItem Env: | Where-Object { $_.Name -match '^(CLAUDE|ANTHROPIC)' } | ForEach-Object { $_.Name })) {
  Remove-Item "Env:$n" -ErrorAction SilentlyContinue
}
foreach ($n in 'AWS_BEARER_TOKEN_BEDROCK', 'HTTP_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy') {
  Remove-Item "Env:$n" -ErrorAction SilentlyContinue
}

$env:CLAUDE_CONFIG_DIR = $H27Home
$env:ANTHROPIC_BASE_URL = "http://127.0.0.1:$H27Port"
$env:ANTHROPIC_API_KEY = 'h27-dummy'
$env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC = '1'
$env:DISABLE_TELEMETRY = '1'
$env:DISABLE_ERROR_REPORTING = '1'
$env:DISABLE_AUTOUPDATER = '1'
$env:DISABLE_BUG_COMMAND = '1'
$env:CLAUDE_CODE_USE_BEDROCK = '0'
$env:CLAUDE_CODE_USE_VERTEX = '0'
$env:HTTPS_PROXY = "http://127.0.0.1:$H27TrapPort"
$env:NO_PROXY = '127.0.0.1,localhost'

# Writes the scratch home's own config (never the real one): onboarding done, the dummy key
# approved, bypass-permissions accepted, the work dir trusted.
function Initialize-H27Home {
  New-Item -ItemType Directory -Force -Path $H27Home, $H27Work | Out-Null
  $projects = @{}
  $projects[($H27Work -replace '\\', '/')] = @{ hasTrustDialogAccepted = $true; allowedTools = @() }
  $cfg = [ordered]@{
    hasCompletedOnboarding        = $true
    lastOnboardingVersion         = '2.1.311'
    bypassPermissionsModeAccepted = $true
    customApiKeyResponses         = @{ approved = @('h27-dummy'); rejected = @() }
    autoUpdates                   = $false
    projects                      = $projects
  }
  $cfg | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $H27Home '.claude.json') -Encoding ascii
  @{ autoUpdates = $false; includeCoAuthoredBy = $false } | ConvertTo-Json | Set-Content -Path (Join-Path $H27Home 'settings.json') -Encoding ascii
}

# Starts the mock model and the trap proxy; returns both processes. Refuses a busy port.
function Start-H27Servers([string]$CaptureFile, [string]$TrapLog) {
  foreach ($port in $H27Port, $H27TrapPort) {
    if (Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue) {
      throw "port $port is already in use; stop the process listening on it first"
    }
  }
  New-Item -ItemType Directory -Force -Path (Split-Path $CaptureFile), (Split-Path $TrapLog) | Out-Null
  $node = (Get-Command node).Source
  $mockOut = Join-Path $H27Scratch 'mock.stdout.txt'
  $mock = Start-Process -FilePath $node -ArgumentList "`"$H27Mock`" --port $H27Port --capture `"$CaptureFile`"" -NoNewWindow -PassThru -RedirectStandardOutput $mockOut -RedirectStandardError (Join-Path $H27Scratch 'mock.stderr.txt')
  $trap = Start-Process -FilePath $node -ArgumentList "`"$(Join-Path $H27Dir 'trap-proxy.mjs')`" --port $H27TrapPort --log `"$TrapLog`"" -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $H27Scratch 'trap.stdout.txt') -RedirectStandardError (Join-Path $H27Scratch 'trap.stderr.txt')
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline) {
    $up = @($H27Port, $H27TrapPort | Where-Object { Get-NetTCPConnection -State Listen -LocalPort $_ -ErrorAction SilentlyContinue })
    if ($up.Count -eq 2) { return @($mock, $trap) }
    Start-Sleep -Milliseconds 250
  }
  Stop-H27Servers @($mock, $trap)
  throw "mock or trap proxy did not start (see $H27Scratch\mock.stderr.txt, trap.stderr.txt)"
}

function Stop-H27Servers($procs) {
  foreach ($p in $procs) { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
}

# Quotes one argument for CommandLineToArgvW (PowerShell 5.1 Start-Process does not).
function ConvertTo-H27Arg([string]$a) {
  if ($a -ne '' -and $a -notmatch '[\s"]') { return $a }
  '"' + ($a -replace '(\\*)"', '$1$1\"' -replace '(\\+)$', '$1$1') + '"'
}

# Runs `claude <args>` in $Cwd with stdout/stderr to files. Kills the process tree after
# $KillAfterSec (an interrupted turn) or fails it after $TimeoutSec. Returns a result object.
function Invoke-H27Claude([string[]]$ClaudeArgs, [string]$Cwd, [string]$OutBase, [int]$TimeoutSec = 300, [int]$KillAfterSec = 0) {
  $exe = (Get-Command claude).Source
  $argLine = ($ClaudeArgs | ForEach-Object { ConvertTo-H27Arg $_ }) -join ' '
  $so = "$OutBase.stdout.txt"; $se = "$OutBase.stderr.txt"
  $stdin = Join-Path $H27Scratch 'empty-stdin.txt'
  if (-not (Test-Path $stdin)) { New-Item -ItemType File -Force -Path $stdin | Out-Null }
  $p = Start-Process -FilePath $exe -ArgumentList $argLine -WorkingDirectory $Cwd -NoNewWindow -PassThru -RedirectStandardOutput $so -RedirectStandardError $se -RedirectStandardInput $stdin
  $null = $p.Handle
  $wait = if ($KillAfterSec -gt 0) { $KillAfterSec } else { $TimeoutSec }
  $finished = $p.WaitForExit($wait * 1000)
  $killed = $false
  if (-not $finished) {
    & taskkill.exe /T /F /PID $p.Id 2>&1 | Out-Null
    $p.WaitForExit(10000) | Out-Null
    $killed = $true
  }
  $exit = if ($killed) { 'killed' } else { $p.ExitCode }
  Set-Content -Path "$OutBase.exit.txt" -Value $exit -Encoding ascii
  $stdout = Get-Content $so -Raw -ErrorAction SilentlyContinue
  $sid = $null
  if ($stdout) { try { $sid = ($stdout | ConvertFrom-Json).session_id } catch { } }
  [pscustomobject]@{ args = $argLine; cwd = $Cwd; exit = $exit; killed = $killed; session_id = $sid; stdout = $so; stderr = $se }
}

# UTF-8 without BOM (PowerShell 5.1 Set-Content -Encoding utf8 writes one, which JSON.parse rejects).
function Write-H27Text([string]$Path, [string]$Text) {
  [IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding($false)))
}

function Get-H27LineCount([string]$file) {
  if (Test-Path $file) { @(Get-Content $file).Count } else { 0 }
}
