# D17 / OpenCode: what do the programs a private OpenCode starts see? And which variables isolate OpenCode narrowly?
#   pwsh -NoProfile -File verify\D17\opencode\run.ps1
# Uses the REAL user profile (USERPROFILE/HOME untouched, so git finds the user's own config) with OpenCode's stores in
# a private home (full set, proven leak-free by D15). The mock's "SHN-SHELL <cmd>" trigger makes OpenCode's own shell tool
# run dump-env.mjs, which records env variable NAMES, `git config --show-origin --list --name-only` origins and the npm
# config paths. Compared with the same script started from a plain child ("fresh terminal").
# Runs: fresh, agent-full (H27 set), agent-full-plugin (H27 set + a private-config plugin restoring XDG_* for the shell tool).
# Also `debug paths` under the full / narrow / dbonly sets. Raw output: %LOCALAPPDATA%\shinatama-verify\D17\opencode\
param([int]$MockPort = 4136)
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot '..\..\D15\opencode\lib.ps1')
$Root = Join-Path $env:LOCALAPPDATA 'shinatama-verify\D17\opencode'
Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
$work = Join-Path $Root 'work'; New-Item -ItemType Directory -Force -Path $work | Out-Null
$dump = ConvertTo-ShnSlash $ShnDump
$report = New-Object System.Collections.Generic.List[string]

# 1. Paths each set gives OpenCode (no session; decoy home so nothing real is touched).
foreach ($set in 'full', 'narrow', 'dbonly') {
  $x = New-ShnOpenCodeEnv -Set $set -Private (Join-Path $Root "paths-$set\private") -MockPort $MockPort -Decoy (Join-Path $Root "paths-$set\decoy")
  $res = Invoke-ShnProcess -Env $x.Env -Arguments @('debug', 'paths') -WorkDir $work -TimeoutSec 60
  $report.Add("== debug paths, set=$set (USERPROFILE=<decoy>\home)"); ($res.Stdout -split "`r?`n" | Where-Object { $_ }) | ForEach-Object { $report.Add($_.Replace($Root, '<D17>')) }
}

# 2. Fresh terminal: dump-env from a plain child (markers removed, no XDG_*, real profile).
$fresh = Join-Path $Root 'fresh.json'
$base = Get-ShnBaseEnv
$null = Invoke-ShnProcess -Exe (Get-Command node).Source -Env $base -Arguments @($ShnDump, $fresh) -WorkDir $work -TimeoutSec 60 -OutPrefix (Join-Path $Root 'fresh')

# 3. Inside OpenCode's shell tool.
function Invoke-D17Agent([string]$Name, [switch]$Plugin) {
  $dir = Join-Path $Root $Name; New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $x = New-ShnOpenCodeEnv -Set full -Private (Join-Path $dir 'private') -MockPort $MockPort
  if ($Plugin) {
    $pdir = Join-Path $x.ConfigDir 'plugins'; New-Item -ItemType Directory -Force -Path $pdir | Out-Null
    Copy-Item (Join-Path $PSScriptRoot 'shn-restore-env.js') $pdir
  }
  $mock = Start-ShnMock $MockPort (Join-Path $dir 'capture.jsonl')
  try {
    $out = Join-Path $dir 'dump.json'
    $res = Invoke-ShnProcess -Env $x.Env -Arguments @('run', '--standalone', '--model', $ShnModel, '--format', 'json', '--auto', '--title', "D17-$Name") `
      -Stdin "SHN-SHELL node $dump $(ConvertTo-ShnSlash $out)" -WorkDir $work -OutPrefix (Join-Path $dir 'run')
  } finally { Stop-ShnProcess $mock }
  [pscustomobject]@{ Name = $Name; Dump = $out; Exit = $res.ExitCode; Env = $x.Env }
}
$agents = @((Invoke-D17Agent 'agent-full'), (Invoke-D17Agent 'agent-full-plugin' -Plugin))

function Read-Dump([string]$f) { if (Test-Path $f) { Get-Content $f -Raw | ConvertFrom-Json } }
$f = Read-Dump $fresh
$report.Add("== fresh terminal: names=$($f.nameCount) git origins:")
$f.git.originCounts.PSObject.Properties | ForEach-Object { $report.Add("   $($_.Name)  keys=$($_.Value)") }
$report.Add("   npm userconfig=$($f.npm.userconfig) cache=$($f.npm.cache)")
foreach ($a in $agents) {
  $d = Read-Dump $a.Dump
  $report.Add("== $($a.Name): opencode exit=$($a.Exit) dump=$([bool]$d)")
  if (-not $d) { continue }
  $report.Add("   names=$($d.nameCount); added vs fresh: $((@($d.names) | Where-Object { $_ -notin @($f.names) }) -join ',')")
  $report.Add("   removed vs fresh: $((@($f.names) | Where-Object { $_ -notin @($d.names) }) -join ',')")
  $report.Add("   values (allowlist): $(($d.values.PSObject.Properties | Where-Object { $_.Name -match '^(XDG_|OPENCODE|AGENT|AI_AGENT|TERM$|HOME$|USERPROFILE$)' } | ForEach-Object { "$($_.Name)=$($_.Value.Replace($Root, '<D17>'))" }) -join '; ')")
  $report.Add("   git origins:"); $d.git.originCounts.PSObject.Properties | ForEach-Object { $report.Add("     $($_.Name.Replace($Root, '<D17>'))  keys=$($_.Value)") }
  $same = ($d.git.originCounts | ConvertTo-Json -Compress) -eq ($f.git.originCounts | ConvertTo-Json -Compress)
  $report.Add("   git config identical to fresh terminal (origins+key counts): $same")
  $report.Add("   npm userconfig=$($d.npm.userconfig) cache=$($d.npm.cache)")
}
# 4. A user whose git config lives at the XDG location (%USERPROFILE%\.config\git\config): decoy home only, same child
#    environment OpenCode gives its shell tool (XDG_* passed through verbatim, step 3), with and without XDG_CONFIG_HOME.
$dh = Join-Path $Root 'xdg-git-decoy\home'; New-Item -ItemType Directory -Force -Path (Join-Path $dh '.config\git') | Out-Null
[IO.File]::WriteAllText((Join-Path $dh '.gitconfig'), "[shn]`n`thomeprobe = 1`n", $ShnUtf8)
[IO.File]::WriteAllText((Join-Path $dh '.config\git\config'), "[shn]`n`txdgprobe = 1`n", $ShnUtf8)
foreach ($case in 'no-xdg', 'private-xdg') {
  $e = Get-ShnBaseEnv; $e['USERPROFILE'] = $dh; $e['HOME'] = $dh
  if ($case -eq 'private-xdg') { $e['XDG_CONFIG_HOME'] = Join-Path $Root 'agent-full\private\config' }
  $o = Join-Path $Root "xdg-git-$case.json"
  $null = Invoke-ShnProcess -Exe (Get-Command node).Source -Env $e -Arguments @($ShnDump, $o) -WorkDir $work -TimeoutSec 60
  $d = Read-Dump $o
  $report.Add("== git with config at the XDG location, ${case}: " + (($d.git.keysByOrigin.PSObject.Properties | Where-Object { $_.Name -notmatch 'Program Files' } | ForEach-Object { "$($_.Name.Replace($Root, '<D17>')) -> $($_.Value -join ',')" }) -join ' | '))
}
[IO.File]::WriteAllLines((Join-Path $Root 'report.txt'), [string[]]$report, $ShnUtf8)
$report
"report: $(Join-Path $Root 'report.txt')"
