# Shared bench for the P20, D15 and D16 Claude Code runs. Dot-source AFTER
# verify/H27/claude_code/env.ps1 (reused: mock + trap proxy + scratch .claude.json); this file
# re-points its variables at a per-row scratch folder and adds a launcher that takes an
# explicit executable, per-child environment overrides and an optional real console.
$BenchLibDir = $PSScriptRoot

# Re-points env.ps1 at %LOCALAPPDATA%\shinatama-verify\<Row>\claude_code\<Variant>.
function Initialize-Bench([string]$Row, [int]$Port, [int]$TrapPort, [string]$Variant, [string]$Mock = (Join-Path $BenchLibDir 'server.p20.mjs')) {
  $script:BenchRoot = Join-Path $env:LOCALAPPDATA "shinatama-verify\$Row\claude_code"
  $script:H27Scratch = Join-Path $script:BenchRoot $Variant
  $script:H27Home = Join-Path $script:H27Scratch 'home'
  $script:H27Work = Join-Path $script:H27Scratch 'work'
  $script:H27Port = $Port
  $script:H27TrapPort = $TrapPort
  $script:H27Mock = $Mock
  $env:CLAUDE_CONFIG_DIR = $script:H27Home   # env.ps1 set it to the H27 home at dot-source time
  Remove-Item Env:AI_AGENT -ErrorAction SilentlyContinue   # env.ps1 drops CLAUDE*/ANTHROPIC* only
  $env:ANTHROPIC_BASE_URL = "http://127.0.0.1:$Port"
  $env:HTTPS_PROXY = "http://127.0.0.1:$TrapPort"
}

# Environment overrides that move every per-user root the OS knows about into a decoy.
function Get-DecoyEnv([string]$DecoyRoot) {
  $h = Join-Path $DecoyRoot 'home'
  $drive = $h.Substring(0, 2)
  @{
    USERPROFILE = $h; HOME = $h; HOMEDRIVE = $drive; HOMEPATH = $h.Substring(2)
    APPDATA = (Join-Path $DecoyRoot 'appdata'); LOCALAPPDATA = (Join-Path $DecoyRoot 'localappdata')
    TEMP = (Join-Path $DecoyRoot 'temp'); TMP = (Join-Path $DecoyRoot 'temp')
  }
}

function New-DecoyRoot([string]$DecoyRoot) {
  foreach ($d in 'home', 'appdata', 'localappdata', 'temp') { New-Item -ItemType Directory -Force -Path (Join-Path $DecoyRoot $d) | Out-Null }
}

# Every file under $Dir as "<relative path>`t<size>", sorted.
function Get-Tree([string]$Dir) {
  if (-not (Test-Path -LiteralPath $Dir)) { return @() }
  $full = (Resolve-Path -LiteralPath $Dir).Path
  @(Get-ChildItem -LiteralPath $full -Recurse -Force -File -ErrorAction SilentlyContinue |
      ForEach-Object { "{0}`t{1}" -f $_.FullName.Substring($full.Length).TrimStart('\'), $_.Length } | Sort-Object)
}

# Runs $Exe with $ClaudeArgs in $Cwd. $Env: name -> value ($null removes it) for this child only.
# -Console: own (hidden) console window, so stdin/stdout are a real console = interactive TUI;
# no stdout capture then. Kills the tree after $KillAfterSec (0 = wait up to $TimeoutSec).
function Invoke-Bench([string]$Exe, [string[]]$ClaudeArgs, [string]$Cwd, [string]$OutBase, [hashtable]$Env = @{}, [int]$TimeoutSec = 180, [int]$KillAfterSec = 0, [switch]$Console) {
  $argLine = ($ClaudeArgs | ForEach-Object { ConvertTo-H27Arg $_ }) -join ' '
  $so = "$OutBase.stdout.txt"; $se = "$OutBase.stderr.txt"
  $stdin = Join-Path $script:BenchRoot 'empty-stdin.txt'
  if (-not (Test-Path $stdin)) { New-Item -ItemType File -Force -Path $stdin | Out-Null }
  $saved = @{}
  foreach ($k in $Env.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $Env[$k]) }
  try {
    if ($Console) {
      $p = Start-Process -FilePath $Exe -ArgumentList $argLine -WorkingDirectory $Cwd -WindowStyle Hidden -PassThru
    } else {
      $p = Start-Process -FilePath $Exe -ArgumentList $argLine -WorkingDirectory $Cwd -NoNewWindow -PassThru -RedirectStandardOutput $so -RedirectStandardError $se -RedirectStandardInput $stdin
    }
  } finally {
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
  }
  $null = $p.Handle
  $wait = if ($KillAfterSec -gt 0) { $KillAfterSec } else { $TimeoutSec }
  $finished = $p.WaitForExit($wait * 1000)
  $killed = $false
  if (-not $finished) { & taskkill.exe /T /F /PID $p.Id 2>&1 | Out-Null; $p.WaitForExit(10000) | Out-Null; $killed = $true }
  $exit = if ($killed) { 'killed' } else { $p.ExitCode }
  Set-Content -Path "$OutBase.exit.txt" -Value "$exit pid=$($p.Id) args=$argLine" -Encoding utf8
  $sid = $null
  if (-not $Console -and (Test-Path $so)) { try { $sid = ((Get-Content $so -Raw) | ConvertFrom-Json).session_id } catch { } }
  [pscustomobject]@{ args = $argLine; exit = $exit; killed = $killed; session_id = $sid; pid = $p.Id }
}

# Names of the requests the mock received, one line each (count_tokens probes included).
function Get-CaptureSummary([string]$Capture) {
  if (-not (Test-Path $Capture)) { return @() }
  Get-Content $Capture | ForEach-Object { $c = $_ | ConvertFrom-Json; "{0} {1} {2}" -f $c.n, $c.method, $c.url }
}

# Waits (up to 15 s) until neither bench port is still listening (a just-stopped mock).
function Wait-BenchPortsFree {
  $deadline = (Get-Date).AddSeconds(15)
  while ((Get-Date) -lt $deadline -and @($script:H27Port, $script:H27TrapPort | Where-Object { Get-NetTCPConnection -State Listen -LocalPort $_ -ErrorAction SilentlyContinue }).Count) { Start-Sleep -Milliseconds 300 }
}
