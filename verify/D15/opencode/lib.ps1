# D15/D16/D17/P20 - OpenCode: shared helpers. Dot-source me (PowerShell 7 or 5.1).
# Unlike verify\H27\opencode\env.ps1 this never changes the calling process's environment: every child gets an
# explicit environment block built here, so one script can compare several variable sets side by side.
#   - inherited agent markers (CLAUDECODE, CLAUDE_CODE_*, AI_AGENT, OPENCODE*, ...) are removed unless a run adds them back;
#   - provider keys are removed; the only key is the literal h27-dummy;
#   - HTTP(S)_PROXY points at a dead port (NO_PROXY keeps 127.0.0.1 direct), models.dev fetch and autoupdate are off;
#   - opencode is always started with --standalone (the user's background service has the real environment).

$ShnUtf8 = New-Object System.Text.UTF8Encoding($false)
$ShnRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$ShnExe = Join-Path $env:APPDATA 'npm\node_modules\@opencode\cli\bin\opencode.exe'
$ShnMock = Join-Path $ShnRepo 'verify\D17\opencode\server.shell.mjs'
$ShnDump = Join-Path $ShnRepo 'verify\D17\opencode\dump-env.mjs'
$ShnModel = 'anthropic/h27-mock'
$ShnPassword = 'shn-opencode-local'
# Names removed from every child (agent markers of the session this runs in, and OpenCode's own).
$ShnMarkerRegex = '^(CLAUDECODE|CLAUDE_CODE_.*|CLAUDE_PID|CLAUDE_EFFORT|CLAUDE_AGENT_SDK.*|AI_AGENT|AGENT|OPENCODE|OPENCODE_.*|BILLION_CONTEXT_PROXY|MUSASHI_TERMINAL_.*)$'
$ShnSecretRegex = '(_API_KEY|_AUTH_TOKEN|_ACCESS_TOKEN)$|^(OPENROUTER_|AWS_|AZURE_|GOOGLE_|VERTEX|ANTHROPIC_|OPENAI_)'

# The current environment minus markers, provider keys, PWD and proxies (case-insensitive keys).
function Get-ShnBaseEnv {
  $h = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([StringComparer]::OrdinalIgnoreCase)
  foreach ($e in [Environment]::GetEnvironmentVariables().GetEnumerator()) {
    $n = [string]$e.Key
    if ($n -match $ShnMarkerRegex -or $n -match $ShnSecretRegex -or $n -eq 'PWD' -or $n -match '^(HTTPS?_PROXY|NO_PROXY|ALL_PROXY)$') { continue }
    $h[$n] = [string]$e.Value
  }
  $h
}

# Variable sets for a private OpenCode home under $Private:
#   full   = H27 set: OPENCODE_DB + XDG_DATA/CONFIG/STATE/CACHE_HOME + OPENCODE_TEST_HOME (+ OPENCODE_CONFIG file)
#   narrow = no XDG_*: OPENCODE_DB + OPENCODE_CONFIG_DIR + OPENCODE_TEST_HOME
#   dbonly = OPENCODE_DB alone (config through OPENCODE_CONFIG file, which only adds config, see notes)
# Returns @{ Env = <dictionary>; ConfigDir = <dir where opencode.json goes> }.
function New-ShnOpenCodeEnv {
  param([ValidateSet('full', 'narrow', 'dbonly')][string]$Set, [string]$Private, [int]$MockPort, [string]$Decoy, [hashtable]$Add = @{}, [switch]$AllowClaudeDir)
  $e = Get-ShnBaseEnv
  if ($Decoy) {
    foreach ($d in 'home', 'appdata', 'localappdata', 'localappdata\Temp') { New-Item -ItemType Directory -Force -Path (Join-Path $Decoy $d) | Out-Null }
    $e['USERPROFILE'] = Join-Path $Decoy 'home'; $e['HOME'] = $e['USERPROFILE']
    $e['HOMEDRIVE'] = Split-Path -Qualifier $e['USERPROFILE']; $e['HOMEPATH'] = (Split-Path -NoQualifier $e['USERPROFILE'])
    $e['APPDATA'] = Join-Path $Decoy 'appdata'; $e['LOCALAPPDATA'] = Join-Path $Decoy 'localappdata'
    $e['TEMP'] = Join-Path $Decoy 'localappdata\Temp'; $e['TMP'] = $e['TEMP']
  } else {
    $e['TEMP'] = Join-Path $Private 'tmp'; $e['TMP'] = $e['TEMP']
  }
  New-Item -ItemType Directory -Force -Path $Private, $e['TEMP'] | Out-Null
  $e['OPENCODE_DB'] = Join-Path $Private 'data\opencode\opencode.db'
  switch ($Set) {
    'full' {
      $e['XDG_DATA_HOME'] = Join-Path $Private 'data'; $e['XDG_CONFIG_HOME'] = Join-Path $Private 'config'
      $e['XDG_STATE_HOME'] = Join-Path $Private 'state'; $e['XDG_CACHE_HOME'] = Join-Path $Private 'cache'
      $e['OPENCODE_TEST_HOME'] = Join-Path $Private 'home'
      $cfg = Join-Path $Private 'config\opencode'
      $e['OPENCODE_CONFIG'] = Join-Path $cfg 'opencode.json'
    }
    'narrow' {
      $cfg = Join-Path $Private 'config\opencode'
      $e['OPENCODE_CONFIG_DIR'] = $cfg
      $e['OPENCODE_TEST_HOME'] = Join-Path $Private 'home'
    }
    'dbonly' {
      $cfg = Join-Path $Private 'config\opencode'
      $e['OPENCODE_CONFIG'] = Join-Path $cfg 'opencode.json'
    }
  }
  New-Item -ItemType Directory -Force -Path $cfg, (Split-Path $e['OPENCODE_DB']) | Out-Null
  if ($e.ContainsKey('OPENCODE_TEST_HOME')) { New-Item -ItemType Directory -Force -Path $e['OPENCODE_TEST_HOME'] | Out-Null }
  $e['ANTHROPIC_API_KEY'] = 'h27-dummy'
  $e['OPENCODE_DISABLE_MODELS_FETCH'] = '1'
  $e['OPENCODE_DISABLE_PROJECT_CONFIG'] = '1'
  if (-not $AllowClaudeDir) { $e['OPENCODE_DISABLE_CLAUDE_CODE'] = '1' }
  $e['OPENCODE_DISABLE_AUTOUPDATE'] = '1'
  $e['OPENCODE_PASSWORD'] = $ShnPassword
  foreach ($n in 'HTTP_PROXY', 'HTTPS_PROXY', 'http_proxy', 'https_proxy') { $e[$n] = 'http://127.0.0.1:9' }
  $e['NO_PROXY'] = '127.0.0.1,localhost'; $e['no_proxy'] = $e['NO_PROXY']
  foreach ($k in $Add.Keys) { $e[$k] = [string]$Add[$k] }
  Write-ShnConfig -Dir $cfg -MockPort $MockPort
  @{ Env = $e; ConfigDir = $cfg }
}

function Write-ShnConfig([string]$Dir, [int]$MockPort) {
  $config = [ordered]@{
    model = $ShnModel; update = 'disable'; share = 'disabled'; snapshots = $false
    providers = [ordered]@{ anthropic = [ordered]@{
        settings = [ordered]@{ baseURL = "http://127.0.0.1:$MockPort/v1"; apiKey = 'h27-dummy' }
        models   = [ordered]@{ 'h27-mock' = [ordered]@{ name = 'H27 mock'; capabilities = [ordered]@{ tools = $true; reasoning = [ordered]@{ supported = $true } }; limit = [ordered]@{ context = 200000; output = 8192 } } }
      } }
  }
  [IO.File]::WriteAllText((Join-Path $Dir 'opencode.json'), ($config | ConvertTo-Json -Depth 20), $ShnUtf8)
}

function ConvertTo-ShnCommandLine([string[]]$Arguments) {
  ($Arguments | ForEach-Object {
      if ($_ -eq '') { '""' } elseif ($_ -notmatch '[\s"]') { $_ } else { '"' + (($_ -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"' }
    }) -join ' '
}

# Starts $Exe with exactly $Env (a dictionary), prompt on stdin; kills the tree on timeout.
function Invoke-ShnProcess {
  param([string]$Exe = $ShnExe, $Env, [string[]]$Arguments, [string]$Stdin = '', [string]$WorkDir, [int]$TimeoutSec = 180, [string]$OutPrefix)
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe; $psi.Arguments = ConvertTo-ShnCommandLine $Arguments; $psi.WorkingDirectory = $WorkDir
  $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
  $psi.StandardOutputEncoding = $ShnUtf8; $psi.StandardErrorEncoding = $ShnUtf8
  $psi.EnvironmentVariables.Clear()
  foreach ($k in $Env.Keys) { $psi.EnvironmentVariables[$k] = $Env[$k] }
  $started = Get-Date
  $p = [System.Diagnostics.Process]::Start($psi)
  $o = $p.StandardOutput.ReadToEndAsync(); $er = $p.StandardError.ReadToEndAsync()
  $b = $ShnUtf8.GetBytes($Stdin); $p.StandardInput.BaseStream.Write($b, 0, $b.Length); $p.StandardInput.Close()
  $timedOut = -not $p.WaitForExit($TimeoutSec * 1000)
  if ($timedOut) { & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null; $p.WaitForExit(15000) | Out-Null } else { $p.WaitForExit() }
  $null = $o.Wait(15000); $null = $er.Wait(15000)
  $r = [pscustomobject]@{ ExitCode = $(if ($p.HasExited) { $p.ExitCode } else { 'running' }); TimedOut = $timedOut
    Seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 1); Stdout = $o.Result; Stderr = $er.Result; CommandLine = "$Exe $($psi.Arguments)" }
  if ($OutPrefix) {
    [IO.File]::WriteAllText("$OutPrefix.stdout.txt", $r.Stdout, $ShnUtf8)
    [IO.File]::WriteAllText("$OutPrefix.stderr.txt", $r.Stderr, $ShnUtf8)
    [IO.File]::WriteAllText("$OutPrefix.exit.txt", "exit=$($r.ExitCode)`ntimedOut=$timedOut`nseconds=$($r.Seconds)`ncwd=$WorkDir`ncommand=$($r.CommandLine)`nstdin=$Stdin`nenvNames=$(($Env.Keys | Sort-Object) -join ',')`n", $ShnUtf8)
  }
  $r
}

function Get-ShnSessionId([string]$Text) {
  $m = [regex]::Match($Text, '"sessionID":"(ses_[A-Za-z0-9]+)"'); if ($m.Success) { $m.Groups[1].Value } else { $null }
}

function Start-ShnMock([int]$Port, [string]$Capture) {
  foreach ($c in @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)) {
    $cmd = "$((Get-CimInstance Win32_Process -Filter "ProcessId = $($c.OwningProcess)").CommandLine)"
    if ($cmd -match 'server\.shell\.mjs') { & taskkill.exe /PID $c.OwningProcess /T /F 2>&1 | Out-Null } else { throw "port $Port held by: $cmd" }
  }
  $base = $Capture -replace '\.jsonl$', ''
  $p = Start-Process node -ArgumentList @("`"$ShnMock`"", '--port', $Port, '--capture', "`"$Capture`"") -PassThru -WindowStyle Hidden -RedirectStandardOutput "$base.mock-stdout.txt" -RedirectStandardError "$base.mock-stderr.txt"
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline) { if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { return $p }; Start-Sleep -Milliseconds 300 }
  throw "mock did not listen on $Port"
}

function Stop-ShnProcess($p) { if ($p -and -not $p.HasExited) { & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null } }

# Every file under $Root: relative path and size (names and sizes only).
function Get-ShnTree([string]$Root) {
  if (-not (Test-Path $Root)) { return @() }
  Get-ChildItem -LiteralPath $Root -Recurse -File -Force -ErrorAction SilentlyContinue | Sort-Object FullName |
    ForEach-Object { '{0}  {1}' -f $_.FullName.Substring($Root.Length).TrimStart('\'), $_.Length }
}

# A forward-slash path that both Git Bash and PowerShell accept unquoted (no spaces allowed).
function ConvertTo-ShnSlash([string]$Path) { if ($Path -match '\s') { throw "path has a space: $Path" }; $Path -replace '\\', '/' }
