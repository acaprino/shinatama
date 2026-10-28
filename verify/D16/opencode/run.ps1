# D16 / OpenCode: private install per version, self-update off, side by side with a running instance, rollback.
#   pwsh -NoProfile -File verify\D16\opencode\run.ps1 [-New 2.1.4] [-Old 2.1.3]
# Installs with `npm install --prefix <versions\<ver>> @opencode/cli@<ver>` (the package the user's global install uses;
# network: npm registry only). Every OpenCode start uses a private home (full set, lib.ps1) with the dead proxy.
# Raw output: %LOCALAPPDATA%\shinatama-verify\D16\opencode\
param([string]$New = '2.1.4', [string]$Old = '2.1.3', [int]$PortNew = 43161, [int]$PortOld = 43162)
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot '..\..\D15\opencode\lib.ps1')
$Root = Join-Path $env:LOCALAPPDATA 'shinatama-verify\D16\opencode'
$Versions = Join-Path $Root 'versions'; $Logs = Join-Path $Root 'logs'
foreach ($p in @(Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*$Versions\*" -and $_.CommandLine -like '*@opencode*' })) { & taskkill.exe /PID $p.ProcessId /T /F 2>&1 | Out-Null }
Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Versions, $Logs | Out-Null
$report = New-Object System.Collections.Generic.List[string]
$npmEnv = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([StringComparer]::OrdinalIgnoreCase)
foreach ($e in [Environment]::GetEnvironmentVariables().GetEnumerator()) { if ([string]$e.Key -notmatch $ShnMarkerRegex) { $npmEnv[[string]$e.Key] = [string]$e.Value } }
# npm runs @opencode/cli's postinstall, which starts `opencode --version`; with the user's environment that start appends
# "cli starting" to %USERPROFILE%\.local\share\opencode\log\opencode.log (seen 2026-10-28). So npm gets the private-home
# variables too (proxies of the calling session are kept: npm needs the registry).
$verEnv = (New-ShnOpenCodeEnv -Set full -Private (Join-Path $Root 'home-install') -MockPort 4138).Env
foreach ($k in @($verEnv.Keys)) { if ($k -match '^(XDG_|OPENCODE_)' -or $k -in 'TEMP', 'TMP') { $npmEnv[$k] = $verEnv[$k] } }
$npmCli = Join-Path (Split-Path (Get-Command node).Source) 'node_modules\npm\bin\npm-cli.js'
$npmCache = (& npm config get cache).Trim()
$globalPkg = Join-Path $env:APPDATA 'npm\node_modules\@opencode\cli\package.json'

function Get-Outside {
  [pscustomobject]@{
    CacheFiles = @(Get-ChildItem (Join-Path $npmCache '_cacache') -Recurse -File -ErrorAction SilentlyContinue).Count
    CacheLogs  = @(Get-ChildItem (Join-Path $npmCache '_logs') -File -ErrorAction SilentlyContinue).Count
    TempInstall = @(Get-ChildItem $env:TEMP -Directory -Filter 'opencode-install-*' -ErrorAction SilentlyContinue).Count
    Global     = (Get-Item $globalPkg).LastWriteTimeUtc.ToString('o') + ' ' + ((Get-Content $globalPkg -Raw | ConvertFrom-Json).version)
    NpmRc      = $(if (Test-Path "$env:USERPROFILE\.npmrc") { (Get-Item "$env:USERPROFILE\.npmrc").LastWriteTimeUtc.ToString('o') } else { 'absent' })
  }
}
function Install-Version([string]$Ver, [string]$Dir, [string]$Tag) {
  $r = Invoke-ShnProcess -Exe (Get-Command node).Source -Env $npmEnv -Arguments @($npmCli, 'install', '--prefix', $Dir, "@opencode/cli@$Ver", '--no-fund', '--no-audit', '--loglevel=warn') -WorkDir $Root -TimeoutSec 600 -OutPrefix (Join-Path $Logs "install-$Tag")
  $report.Add("== npm install $Ver into $($Dir.Replace($Root,'<D16>')): exit=$($r.ExitCode) $($r.Seconds)s; stderr tail: $((($r.Stderr -split "`n") | Where-Object { $_.Trim() } | Select-Object -Last 3) -join ' / ')")
  $r
}
function Exe([string]$Dir) { Join-Path $Dir 'node_modules\@opencode\cli\bin\opencode.exe' }
function Start-Serve([string]$Dir, [int]$Port, [string]$Tag) {
  $x = New-ShnOpenCodeEnv -Set full -Private (Join-Path $Root "home-$Tag") -MockPort 4138
  $psi = New-Object System.Diagnostics.ProcessStartInfo (Exe $Dir), "serve --hostname 127.0.0.1 --port $Port"
  $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true; $psi.WorkingDirectory = $Root
  $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
  $psi.EnvironmentVariables.Clear(); foreach ($k in $x.Env.Keys) { $psi.EnvironmentVariables[$k] = $x.Env[$k] }
  $p = [Diagnostics.Process]::Start($psi)
  $deadline = (Get-Date).AddSeconds(60); $ok = $false
  while ((Get-Date) -lt $deadline -and -not $ok) { Start-Sleep -Milliseconds 700; $ok = [bool](Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) }
  $report.Add("   serve $Tag (pid $($p.Id)) from $((Exe $Dir).Replace($Root,'<D16>')) listening on ${Port}: $ok")
  $p
}
function Test-Serve([int]$Port) {
  $auth = 'Authorization: Basic ' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("opencode:$ShnPassword"))
  (& curl.exe -s --noproxy '*' -m 10 -o NUL -w '%{http_code}' -H $auth "http://127.0.0.1:$Port/api/session") 2>$null
}

$before = Get-Outside
$dNew = Join-Path $Versions $New; $dOld = Join-Path $Versions $Old
$null = Install-Version $New $dNew 'new'
foreach ($d in $dNew) { $v = Invoke-ShnProcess -Exe (Exe $d) -Env $verEnv -Arguments @('--version') -WorkDir $Root -TimeoutSec 60; $report.Add("   $((Exe $d).Replace($Root,'<D16>')) --version -> $($v.Stdout.Trim())") }
# Side by side: the new version keeps serving while the old one is installed and started.
$pNew = Start-Serve $dNew $PortNew 'new'
$null = Install-Version $Old $dOld 'old-while-new-runs'
$v = Invoke-ShnProcess -Exe (Exe $dOld) -Env $verEnv -Arguments @('--version') -WorkDir $Root -TimeoutSec 60; $report.Add("   $((Exe $dOld).Replace($Root,'<D16>')) --version -> $($v.Stdout.Trim())")
$pOld = Start-Serve $dOld $PortOld 'old'
$report.Add("   both running: new HTTP $(Test-Serve $PortNew), old HTTP $(Test-Serve $PortOld)")
# Locked executable: replacing the RUNNING old version in place (same folder) while it serves.
$null = Install-Version $New $dOld 'in-place-over-running-old'
$v = Invoke-ShnProcess -Exe (Exe $dOld) -Env $verEnv -Arguments @('--version') -WorkDir $Root -TimeoutSec 60
$report.Add("   after in-place attempt: folder $Old reports --version $($v.Stdout.Trim()); package.json says $(((Get-Content (Join-Path $dOld 'node_modules\@opencode\cli\package.json') -Raw) | ConvertFrom-Json).version); old serve still answers HTTP $(Test-Serve $PortOld)")
Stop-ShnProcess $pOld; Stop-ShnProcess $pNew
# Rollback: put the old version back in its folder and start it again.
$null = Install-Version $Old $dOld 'rollback-reinstall-old'
$pOld2 = Start-Serve $dOld $PortOld 'old-again'
$v = Invoke-ShnProcess -Exe (Exe $dOld) -Env $verEnv -Arguments @('--version') -WorkDir $Root -TimeoutSec 60
$report.Add("   rollback: $Old --version -> $($v.Stdout.Trim()); HTTP $(Test-Serve $PortOld)")
Stop-ShnProcess $pOld2
$after = Get-Outside
$report.Add("== outside the version folders: before $($before | ConvertTo-Json -Compress)")
$report.Add("                                 after  $($after | ConvertTo-Json -Compress)")

# Self-update: the update check runs only in the interactive TUI (default command). Start it hidden for 25 s with a private
# home and read its log for the updater's decision. Case A: OPENCODE_DISABLE_AUTOUPDATE=1 (+ config update:disable);
# case B: config update:"disable" only. (A case with neither is not run: it would fetch opencode.ai.)
foreach ($case in 'A-env', 'B-config') {
  $x = New-ShnOpenCodeEnv -Set full -Private (Join-Path $Root "tui-$case") -MockPort 4138
  if ($case -eq 'B-config') { $x.Env.Remove('OPENCODE_DISABLE_AUTOUPDATE') | Out-Null }
  $psi = New-Object System.Diagnostics.ProcessStartInfo (Exe $dNew), '--log-level debug'
  $psi.UseShellExecute = $false; $psi.WorkingDirectory = $Root
  $psi.EnvironmentVariables.Clear(); foreach ($k in $x.Env.Keys) { $psi.EnvironmentVariables[$k] = $x.Env[$k] }
  $psi.CreateNoWindow = $true
  $p = [Diagnostics.Process]::Start($psi); Start-Sleep -Seconds 25
  & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null
  foreach ($s in @(Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*serve --service*' -and (Test-Path (Join-Path $Root "tui-$case\cache\opencode")) -and @(Get-ChildItem (Join-Path $Root "tui-$case\cache\opencode") -Filter "opencode-service-$($_.ProcessId)-*" -ErrorAction SilentlyContinue).Count })) { & taskkill.exe /PID $s.ProcessId /T /F 2>&1 | Out-Null; $report.Add("   killed private background service pid $($s.ProcessId) started by the TUI") }
  $log = Get-ChildItem (Join-Path $Root "tui-$case\data\opencode\log") -File -ErrorAction SilentlyContinue | Get-Content -Raw
  $lines = @(($log -split "`n") | Where-Object { $_ -match 'update|upgrade|latest|opencode\.ai' } | ForEach-Object { $_.Trim().Substring(0, [Math]::Min(220, $_.Trim().Length)) } | Select-Object -First 6)
  $report.Add("== TUI $case (log $(([string]$log).Length) chars): $($lines.Count) update-related lines"); $lines | ForEach-Object { $report.Add("     $_") }
}
[IO.File]::WriteAllLines((Join-Path $Root 'report.txt'), [string[]]$report, $ShnUtf8)
$report
"report: $(Join-Path $Root 'report.txt')"
