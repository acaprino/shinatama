# D16 / Claude Code - versions in private folders: run from folder, self-update off, side by
# side with an instance running, in-place replacement of a running binary, rollback.
# Needs install.ps1 first (versions\2.1.311 and versions\2.1.312). All Claude runs use the H27
# mock + trap proxy, a scratch CLAUDE_CONFIG_DIR and a decoy USERPROFILE/APPDATA/LOCALAPPDATA/
# TEMP, and npm_config_prefix/cache inside the D16 folder, so a self-update attempt (npm -g or
# native ~\.local\share\claude) could land only in scratch folders and the trap logs it.
# Output: %LOCALAPPDATA%\shinatama-verify\D16\claude_code\run\summary.txt (+ steps\, trap-*.jsonl)
# Usage: pwsh -File run.ps1 [-Old 2.1.311] [-New 2.1.312] [-HangSec 60]
# -VersionsRoot: where install.ps1 put the versions (default <row folder>\versions); the npm cache
# for the side-by-side installs sits next to it.
param([string]$Old = '2.1.311', [string]$New = '2.1.312', [int]$HangSec = 60, [string]$VersionsRoot = '')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\..\H27\claude_code\env.ps1')
. (Join-Path $PSScriptRoot '..\..\P20\claude_code\bench.ps1')
Initialize-Bench -Row D16 -Port 47171 -TrapPort 47197 -Variant run
$Versions = if ($VersionsRoot) { $VersionsRoot } else { Join-Path $BenchRoot 'versions' }
$NpmCache = Join-Path (Split-Path $Versions) 'npm-cache'
$exeOf = { param($dir) Join-Path $dir 'node_modules\@anthropic-ai\claude-code\bin\claude.exe' }
$OldExe = & $exeOf (Join-Path $Versions $Old)
$NewExe = & $exeOf (Join-Path $Versions $New)
$npm = (Get-Command npm.cmd).Source
if (Test-Path $H27Scratch) { Remove-Item -Recurse -Force $H27Scratch }
$Steps = Join-Path $H27Scratch 'steps'; $Decoy = Join-Path $H27Scratch 'decoy'
New-Item -ItemType Directory -Force -Path $Steps | Out-Null
New-DecoyRoot $Decoy
Initialize-H27Home
# No autoUpdates key anywhere: the env variable alone decides in the comparison below.
Write-H27Text (Join-Path $H27Home 'settings.json') '{}'
$cfg = Get-Content (Join-Path $H27Home '.claude.json') -Raw | ConvertFrom-Json
$cfg.PSObject.Properties.Remove('autoUpdates'); Write-H27Text (Join-Path $H27Home '.claude.json') ($cfg | ConvertTo-Json -Depth 6)
$base = (Get-DecoyEnv $Decoy) + @{
  npm_config_prefix = (Join-Path $H27Scratch 'npm-global'); npm_config_cache = $NpmCache
}
# env.ps1 sets both; here they are decided per child.
Remove-Item Env:DISABLE_AUTOUPDATER, Env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC -ErrorAction SilentlyContinue
$log = @()

# 1. Each runs from its own folder.
foreach ($e in $OldExe, $NewExe) { $log += "--version $e -> $((& $e --version 2>&1 | Out-String).Trim())" }

# 2. Self-update: interactive console turn that hangs for $HangSec, with and without the flag.
foreach ($phase in @(@{ name = 'autoupdater-off'; env = @{ DISABLE_AUTOUPDATER = '1' } }, @{ name = 'autoupdater-default'; env = @{} })) {
  $trap = Join-Path $H27Scratch "trap-$($phase.name).jsonl"
  Wait-BenchPortsFree; $servers = Start-H27Servers (Join-Path $H27Scratch "capture-$($phase.name).jsonl") $trap
  try {
    $envs = $base.Clone(); foreach ($k in $phase.env.Keys) { $envs[$k] = $phase.env[$k] }
    $r = Invoke-Bench $NewExe @('--dangerously-skip-permissions', 'H27-HANG') $H27Work "$Steps\$($phase.name)" $envs -KillAfterSec $HangSec -Console
    $log += "$($phase.name): interactive $New ran $HangSec s exit=$($r.exit); trap attempts=$(Get-H27LineCount $trap); mock requests=$(Get-H27LineCount (Join-Path $H27Scratch "capture-$($phase.name).jsonl"))"
  } finally { Stop-H27Servers $servers }
}
$log += "npm-global prefix files after self-update phases: $(@(Get-Tree (Join-Path $H27Scratch 'npm-global')).Count); decoy files: $(@(Get-Tree $Decoy).Count)"

# 3. Side by side: old version keeps running while new copies are installed and started.
Wait-BenchPortsFree; $servers = Start-H27Servers (Join-Path $H27Scratch 'capture-side.jsonl') (Join-Path $H27Scratch 'trap-side.jsonl')
try {
  $saved = @{}; foreach ($k in $base.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $base[$k]) }
  $env:DISABLE_AUTOUPDATER = '1'
  $running = Start-Process -FilePath $OldExe -ArgumentList '--dangerously-skip-permissions H27-HANG' -WorkingDirectory $H27Work -WindowStyle Hidden -PassThru
  # in-place target: a copy of the old version folder whose binary is also running
  $inplace = Join-Path $Versions 'inplace-test'
  if (Test-Path $inplace) { Remove-Item -Recurse -Force $inplace }
  Copy-Item -Recurse (Join-Path $Versions $Old) $inplace
  $inplaceExe = & $exeOf $inplace
  $running2 = Start-Process -FilePath $inplaceExe -ArgumentList '--dangerously-skip-permissions H27-HANG' -WorkingDirectory $H27Work -WindowStyle Hidden -PassThru
  foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }; Remove-Item Env:DISABLE_AUTOUPDATER -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 8
  $side = Join-Path $Versions "$New-side"
  if (Test-Path $side) { Remove-Item -Recurse -Force $side }
  # npm itself goes to the registry directly (allowed for installs), not through the trap.
  $trapProxy = $env:HTTPS_PROXY; Remove-Item Env:HTTPS_PROXY
  & $npm install --prefix $side --cache $NpmCache --no-audit --no-fund "@anthropic-ai/claude-code@$New" *> "$Steps\side-install.log"
  $log += "side-by-side: npm install $New into $side while $Old (pid $($running.Id)) runs: exit=$LASTEXITCODE; --version=$(if (Test-Path (& $exeOf $side)) { (& (& $exeOf $side) --version 2>&1 | Out-String).Trim() } else { 'missing' })"
  $r = Invoke-Bench (& $exeOf $side) @('-p', 'D16 side-by-side turn', '--output-format', 'json', '--dangerously-skip-permissions') $H27Work "$Steps\side-turn" ($base + @{ DISABLE_AUTOUPDATER = '1'; HTTPS_PROXY = $trapProxy })
  $log += "side-by-side: -p turn on $New exit=$($r.exit); $Old still running: $(-not $running.HasExited)"
  & $npm install --prefix $inplace --cache $NpmCache --no-audit --no-fund "@anthropic-ai/claude-code@$New" *> "$Steps\inplace-install.log"
  $inplaceCode = $LASTEXITCODE
  $env:HTTPS_PROXY = $trapProxy
  $log += "in-place: npm install $New over the folder of a RUNNING $Old (pid $($running2.Id)): exit=$inplaceCode; binary now --version=$((& $inplaceExe --version 2>&1 | Out-String).Trim()); running instance alive: $(-not $running2.HasExited)"
  $log += "in-place npm errors: " + ((Select-String -Path "$Steps\inplace-install.log" -Pattern 'EBUSY|EPERM|EACCES|ERR!' | Select-Object -First 4 | ForEach-Object { $_.Line.Trim() }) -join ' | ')
} finally {
  foreach ($p in $running, $running2) { if ($p -and -not $p.HasExited) { & taskkill.exe /T /F /PID $p.Id 2>&1 | Out-Null } }
  Stop-H27Servers $servers
}

# 4. Rollback: the old version starts again and works after the new one ran.
Wait-BenchPortsFree; $servers = Start-H27Servers (Join-Path $H27Scratch 'capture-rollback.jsonl') (Join-Path $H27Scratch 'trap-rollback.jsonl')
try {
  $r = Invoke-Bench $OldExe @('-p', 'D16 rollback turn', '--output-format', 'json', '--dangerously-skip-permissions') $H27Work "$Steps\rollback-turn" ($base + @{ DISABLE_AUTOUPDATER = '1' })
  $log += "rollback: $Old -p turn exit=$($r.exit) result=$(try { ((Get-Content "$Steps\rollback-turn.stdout.txt" -Raw) | ConvertFrom-Json).result } catch { 'n/a' })"
} finally { Stop-H27Servers $servers }
$ua = foreach ($f in Get-ChildItem $H27Scratch -Filter 'capture-*.jsonl') { Get-Content $f | ForEach-Object { "$($f.BaseName): $((($_ | ConvertFrom-Json).headers).'user-agent')" } }
$log += "user-agents seen by the mock: " + (($ua | Sort-Object -Unique) -join ' | ')
$decoyFiles = Get-Tree $Decoy
Set-Content (Join-Path $H27Scratch 'decoy-files.txt') $decoyFiles -Encoding utf8
$log += "decoy files (writes outside the version folders + private home during all runs): $($decoyFiles.Count)"
$log | Tee-Object -FilePath (Join-Path $H27Scratch 'summary.txt')
