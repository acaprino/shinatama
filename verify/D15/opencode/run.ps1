# D15 / OpenCode: does a private home leak? Decoy-home method.
#   pwsh -NoProfile -File verify\D15\opencode\run.ps1 [-Sets full,narrow]
# Per variable set (see lib.ps1): a fresh decoy root (USERPROFILE/HOME, APPDATA, LOCALAPPDATA, TEMP all inside it) and a
# fresh private home; one session with a plain turn, an H27-TOOLS turn and a resumed turn (mock on 127.0.0.1:4135,
# --standalone); then `opencode plugin add <daodan export>` into the private home. Lists every file under the decoy and
# the private home (relative path + size). Raw output: %LOCALAPPDATA%\shinatama-verify\D15\opencode\<set>\
param([string[]]$Sets = @('full', 'narrow'), [int]$MockPort = 4135, [string]$Daodan = 'D:\Projects\daodan\exports\opencode')
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot 'lib.ps1')
$OutRoot = Join-Path $env:LOCALAPPDATA 'shinatama-verify\D15\opencode'
foreach ($set in $Sets) {
  $root = Join-Path $OutRoot $set
  Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
  $decoy = Join-Path $root 'decoy'; $private = Join-Path $root 'private'; $work = Join-Path $root 'work'; $logs = Join-Path $root 'logs'
  New-Item -ItemType Directory -Force -Path $work, $logs | Out-Null
  [IO.File]::WriteAllText((Join-Path $work 'alpha.txt'), 'ALPHA-CONTENT-42', $ShnUtf8)
  [IO.File]::WriteAllText((Join-Path $work 'beta.txt'), 'BETA-CONTENT-17', $ShnUtf8)
  $x = New-ShnOpenCodeEnv -Set $set -Private $private -MockPort $MockPort -Decoy $decoy
  $mock = Start-ShnMock $MockPort (Join-Path $logs 'capture.jsonl')
  try {
    $common = @('--standalone', '--model', $ShnModel, '--format', 'json', '--auto')
    $t1 = Invoke-ShnProcess -Env $x.Env -Arguments (@('run') + $common + @('--title', "D15-$set")) -Stdin "D15-T1 plain turn ($set)" -WorkDir $work -OutPrefix (Join-Path $logs 't1')
    $sid = Get-ShnSessionId $t1.Stdout
    $tools = "H27-TOOLS read $(Join-Path $work 'alpha.txt') and $(Join-Path $work 'beta.txt')"
    $t2 = Invoke-ShnProcess -Env $x.Env -Arguments (@('run') + $common + @('--session', $sid)) -Stdin $tools -WorkDir $work -OutPrefix (Join-Path $logs 't2')
    $t3 = Invoke-ShnProcess -Env $x.Env -Arguments (@('run') + $common + @('--session', $sid)) -Stdin 'D15-T3 resumed turn' -WorkDir $work -OutPrefix (Join-Path $logs 't3')
    $exp = Invoke-ShnProcess -Env $x.Env -Arguments @('session', 'export', $sid, '--standalone') -WorkDir $work -OutPrefix (Join-Path $logs 'export')
    $help = Invoke-ShnProcess -Env $x.Env -Arguments @('plugin', 'add', '--help') -WorkDir $work -OutPrefix (Join-Path $logs 'plugin-add-help')
    $pa = Invoke-ShnProcess -Env $x.Env -Arguments @('plugin', 'add', $Daodan) -WorkDir $work -TimeoutSec 180 -OutPrefix (Join-Path $logs 'plugin-add')
    $pl = Invoke-ShnProcess -Env $x.Env -Arguments @('plugin', 'list') -WorkDir $work -TimeoutSec 120 -OutPrefix (Join-Path $logs 'plugin-list')
  } finally { Stop-ShnProcess $mock }
  # `plugin list` (no --standalone) auto-starts a background service from the private env (serve --service) that outlives
  # the CLI; it is ours when its retained image (cache\opencode\opencode-service-<pid>-*.exe) is in the private home.
  $svc = @(Get-ChildItem (Join-Path $private 'cache\opencode') -Filter 'opencode-service-*' -ErrorAction SilentlyContinue | ForEach-Object { [int]($_.Name -split '-')[2] })
  foreach ($sp in $svc) { if (Get-Process -Id $sp -ErrorAction SilentlyContinue) { & taskkill.exe /PID $sp /T /F 2>&1 | Out-Null } }
  $caps = @(Get-Content (Join-Path $logs 'capture.jsonl') -ErrorAction SilentlyContinue).Count
  $decoyTree = @(Get-ShnTree $decoy); $privateTree = @(Get-ShnTree $private)
  $outsideTemp = @($decoyTree | Where-Object { $_ -notmatch '^localappdata\\Temp\\' })
  $report = @(
    "set=$set  session=$sid  mockRequests=$caps"
    "t1 exit=$($t1.ExitCode) t2 exit=$($t2.ExitCode) t3 exit=$($t3.ExitCode) export exit=$($exp.ExitCode) (export bytes $($exp.Stdout.Length))"
    "t2 has H27-FINAL: $($t2.Stdout -match 'H27-FINAL')   t3 reply: $([regex]::Match($t3.Stdout, 'H27-REPLY: [^`"]*').Value)"
    "plugin add exit=$($pa.ExitCode) timedOut=$($pa.TimedOut)   plugin list exit=$($pl.ExitCode)   background services started (pids, killed): $($svc -join ',')"
    "env names given to opencode: $(($x.Env.Keys | Where-Object { $_ -match '^(XDG_|OPENCODE_|USERPROFILE|HOME|APPDATA|LOCALAPPDATA|TEMP|TMP)' } | Sort-Object) -join ',')"
    "--- decoy files ($($decoyTree.Count); outside Temp: $($outsideTemp.Count)):"; $decoyTree
    "--- private files ($($privateTree.Count)):"; $privateTree
    "--- config after plugin add:"; (Get-Content (Join-Path $x.ConfigDir 'opencode.json') -Raw)
    "--- plugin add stdout/stderr (tail):"; ($pa.Stdout + $pa.Stderr) -split "`n" | Select-Object -Last 15
    "--- plugin list (tail):"; ($pl.Stdout + $pl.Stderr) -split "`n" | Select-Object -Last 15
    "PASS(no decoy writes outside Temp) = $($outsideTemp.Count -eq 0)"
  )
  [IO.File]::WriteAllLines((Join-Path $root 'report.txt'), [string[]]$report, $ShnUtf8)
  $report | Select-Object -First 8
  "report: $(Join-Path $root 'report.txt')"
}
