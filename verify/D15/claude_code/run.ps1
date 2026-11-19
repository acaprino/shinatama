# D15 / Claude Code - private home, no leaks (decoy-home method).
# The agent runs with CLAUDE_CONFIG_DIR at a private home and with
# USERPROFILE/HOME/APPDATA/LOCALAPPDATA/TEMP/TMP pointed at empty decoy folders. Afterwards every
# file under the decoy is a write outside the private home. Mock model + trap proxy from H27.
# Sequence: -p new session (plain) / --resume H27-TOOLS (Read x2) / --resume P20-ENV (shell tool) /
# --resume plain / one interactive turn in a hidden console (30 s) / Daodan marketplace add +
# install from the local checkout D:\Projects\daodan / plugin list.
# Output: %LOCALAPPDATA%\shinatama-verify\D15\claude_code\run\ (summary.txt, *-files.txt, steps\).
# Usage: pwsh -File run.ps1 [-Exe <claude.exe>] [-DaodanRepo D:\Projects\daodan]
param([string]$Exe = (Join-Path $env:USERPROFILE '.local\bin\claude.exe'), [string]$DaodanRepo = 'D:\Projects\daodan')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\..\H27\claude_code\env.ps1')
. (Join-Path $PSScriptRoot '..\..\P20\claude_code\bench.ps1')
Initialize-Bench -Row D15 -Port 47151 -TrapPort 47195 -Variant run

if (Test-Path $H27Scratch) { Remove-Item -Recurse -Force $H27Scratch }
$Decoy = Join-Path $H27Scratch 'decoy'
$Steps = Join-Path $H27Scratch 'steps'
New-DecoyRoot $Decoy
New-Item -ItemType Directory -Force -Path $Steps | Out-Null
Initialize-H27Home
Write-H27Text (Join-Path $H27Work 'alpha.txt') "D15 alpha file`n"
Write-H27Text (Join-Path $H27Work 'beta.txt') "D15 beta file`n"
$capture = Join-Path $H27Scratch 'capture.jsonl'
$trapLog = Join-Path $H27Scratch 'trap.jsonl'
$homeBefore = Get-Tree $H27Home
$servers = Start-H27Servers $capture $trapLog
$denv = Get-DecoyEnv $Decoy
$common = @('--output-format', 'json', '--dangerously-skip-permissions')
$log = @()
try {
  $r1 = Invoke-Bench $Exe (@('-p', 'D15-T1 plain turn') + $common) $H27Work "$Steps\t1" $denv
  $sid = $r1.session_id; $log += "t1 exit=$($r1.exit) sid=$sid"
  $r2 = Invoke-Bench $Exe (@('-p', '--resume', $sid, "H27-TOOLS read $H27Work\alpha.txt and $H27Work\beta.txt") + $common) $H27Work "$Steps\t2" $denv
  $log += "t2 exit=$($r2.exit) sid=$($r2.session_id)"
  $r3 = Invoke-Bench $Exe (@('-p', '--resume', $sid, "P20-ENV::compgen -e | LC_ALL=C sort | tr '\n' ' '") + $common) $H27Work "$Steps\t3" $denv
  $log += "t3 exit=$($r3.exit) sid=$($r3.session_id)"
  $r4 = Invoke-Bench $Exe (@('-p', '--resume', $sid, 'D15-T4 resumed plain turn') + $common) $H27Work "$Steps\t4" $denv
  $log += "t4 exit=$($r4.exit) sid=$($r4.session_id)"
  $r5 = Invoke-Bench $Exe @('--dangerously-skip-permissions', '--resume', $sid, 'D15-T5 interactive turn') $H27Work "$Steps\t5" $denv -KillAfterSec 30 -Console
  $log += "t5 (interactive console, killed after 30 s) exit=$($r5.exit)"
  # Daodan into the private home, from the local checkout (no network needed for path sources).
  $d1 = Invoke-Bench $Exe @('plugin', 'marketplace', 'add', $DaodanRepo) $H27Work "$Steps\daodan-1-marketplace-add" $denv
  $d2 = Invoke-Bench $Exe @('plugin', 'install', 'daodan@daodan') $H27Work "$Steps\daodan-2-install" $denv -TimeoutSec 300
  # daodan@daodan needs foreign marketplaces (GitHub sources, network): also install one plugin
  # of the same marketplace without dependencies, to see where a local install lands.
  $d2b = Invoke-Bench $Exe @('plugin', 'install', 'text-humanizer@daodan') $H27Work "$Steps\daodan-2b-install-text-humanizer" $denv -TimeoutSec 300
  $d3 = Invoke-Bench $Exe @('plugin', 'list', '--json') $H27Work "$Steps\daodan-3-list" $denv
  $log += "daodan marketplace add exit=$($d1.exit); install daodan@daodan exit=$($d2.exit); install text-humanizer@daodan exit=$($d2b.exit); list exit=$($d3.exit)"
} finally {
  Stop-H27Servers $servers
}
$decoyFiles = Get-Tree $Decoy
$homeFiles = Get-Tree $H27Home
$workFiles = Get-Tree $H27Work
Set-Content (Join-Path $H27Scratch 'decoy-files.txt') $decoyFiles -Encoding utf8
Set-Content (Join-Path $H27Scratch 'home-files.txt') $homeFiles -Encoding utf8
Set-Content (Join-Path $H27Scratch 'work-files.txt') $workFiles -Encoding utf8
$summary = @(
  "D15 claude_code run $(Get-Date -Format s)  exe=$Exe  version=$((& $Exe --version 2>&1 | Out-String).Trim())"
  $log
  "decoy files (writes outside the private home): $($decoyFiles.Count)"
  "  by root: " + (($decoyFiles | ForEach-Object { ($_ -split '\\')[0] } | Group-Object | ForEach-Object { "$($_.Name)=$($_.Count)" }) -join ' ')
  "private home files: before=$($homeBefore.Count) after=$($homeFiles.Count)"
  "work dir files: $($workFiles.Count)"
  "trap proxy attempts: $(Get-H27LineCount $trapLog)"
  "mock requests: $(Get-H27LineCount $capture)"
)
$summary | Tee-Object -FilePath (Join-Path $H27Scratch 'summary.txt')
