# D15/D16/P20 Pi - shared helpers, dot-sourced by verify/{D15,D16,P20}/pi/*.ps1.
# Reuses the H27 Pi bench (verify/H27/pi/env.ps1 + lib.ps1): same mock dialect, same process
# launcher (node.exe <cli.js>, no shim), same dead proxy and offline flags.
# The caller sets $BenchRow (e.g. 'D15') before dot-sourcing; everything is process-scoped.

$BenchRealUserProfile = $env:USERPROFILE
$BenchRealLocalAppData = $env:LOCALAPPDATA
$BenchRealTemp = $env:TEMP
$BenchRealHome = $env:HOME
$BenchRealAppData = $env:APPDATA

# Rule 4: no inherited agent markers in any child (exact Claude Code names seen 2026-10-28,
# plus the CLAUDE_CODE_* family for bench hygiene, plus Pi's own markers).
$BenchMarkerNames = @('CLAUDECODE','AI_AGENT','CLAUDE_PID','PI_CODING_AGENT','PI_SESSION_ID','PI_SESSION_FILE',
  'PI_PROVIDER','PI_MODEL','PI_REASONING_LEVEL','CODEX_SANDBOX','CODEX_THREAD_ID','OPENCODE','OPENCODE_CLIENT')
function Clear-BenchMarkers {
  foreach ($n in $BenchMarkerNames) { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
  Get-ChildItem Env: | Where-Object { $_.Name -like 'CLAUDE_CODE_*' } | ForEach-Object { Remove-Item "Env:$($_.Name)" -ErrorAction SilentlyContinue }
}
Clear-BenchMarkers

$BenchH27 = (Resolve-Path (Join-Path $PSScriptRoot '..\..\H27\pi')).Path
. (Join-Path $BenchH27 'env.ps1')   # sets PI_OFFLINE, dead proxy, H27 vars (overridden below)
. (Join-Path $BenchH27 'lib.ps1')   # New-H27PiProcess, Invoke-H27Pi, Start-H27Mock, Save-H27Run ...

# Point the H27 helpers at this row's scratch. Functions read these variables at call time.
function Set-BenchPi([string]$Root, [string]$PiHome, [int]$Port, [string]$Mock) {
  $script:H27PiRoot = $Root
  $script:H27PiHome = $PiHome
  $script:H27PiPort = $Port
  if ($Mock) { $script:H27PiMock = $Mock }
  $env:PI_CODING_AGENT_DIR = $PiHome
}

function Reset-BenchDir([string]$Path) {
  if (Test-Path $Path) { Remove-Item -Recurse -Force $Path }
  New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

# Every file under $Root: relative path, size, last write (UTC).
function Get-BenchTree([string]$Root) {
  if (-not (Test-Path $Root)) { return @() }
  $rootFull = (Resolve-Path $Root).Path.TrimEnd('\')
  @(Get-ChildItem -Path $rootFull -Recurse -File -Force -ErrorAction SilentlyContinue | ForEach-Object {
    [pscustomobject]@{ Rel = $_.FullName.Substring($rootFull.Length + 1); Size = $_.Length; Mtime = $_.LastWriteTimeUtc.ToString('o') }
  })
}
function Format-BenchTree($Tree) { ($Tree | Sort-Object Rel | ForEach-Object { "{0}`t{1}" -f $_.Rel, $_.Size }) -join "`n" }

# Newest file under the REAL ~\.pi\agent (top level + sessions), to prove it never moved.
function Get-BenchRealPiStamp {
  $real = Join-Path $BenchRealUserProfile '.pi\agent'
  $files = @(Get-ChildItem -Path $real -File -Force -ErrorAction SilentlyContinue) + @(Get-ChildItem -Path (Join-Path $real 'sessions') -Recurse -File -ErrorAction SilentlyContinue)
  $n = $files | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
  if ($n) { '{0:o} {1}' -f $n.LastWriteTimeUtc, $n.FullName } else { 'none' }
}

function Write-BenchLog([string]$Path, [string]$Line) {
  Write-Host $Line
  Add-Content -Path $Path -Value $Line -Encoding UTF8
}

# Session id of the newest session file under a Pi home.
function Get-BenchNewestSession([string]$PiHome) {
  $f = Get-ChildItem -Path (Join-Path $PiHome 'sessions') -Recurse -File -Filter '*.jsonl' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
  if (-not $f) { return $null }
  [pscustomobject]@{ File = $f.FullName; Id = ($f.BaseName -split '_', 2)[1] }
}
