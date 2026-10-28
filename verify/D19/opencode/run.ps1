# D19 / OpenCode 2.1.4: per-instance set of plugins, skills and MCP servers in a box.
#   pwsh -NoProfile -File verify\D19\opencode\run.ps1 [-MockPort 47161] [-Only <variant>,...]
# A scratch box (OPENCODE_DB + four XDG_* + OPENCODE_TEST_HOME + TEMP/TMP, decoy USERPROFILE) whose global config holds
# plugin d19-home-plugin, skill d19-home-skill and MCP d19-home-mcp; user-level skills in the box home's .claude\skills
# and .agents\skills (and decoys in the decoy USERPROFILE). Sets A and B are instance config directories with their own
# plugin, skill and MCP. One mock turn per variant (run --standalone); the first model request is searched for d19-*
# names. Then two instances at once (A hangs on H27-HANG while B completes) on the same OPENCODE_DB, sessions listed
# read-only. Home files hashed before/after every switch run (database, logs, cache, session storage excluded).
# Mock model only on 127.0.0.1; dead proxy; only PIDs started here are ever stopped.
# Raw output: %LOCALAPPDATA%\shinatama-verify\D19\opencode\
param([int]$MockPort = 47161, [string[]]$Only)
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot 'lib.ps1')
$Root = Join-Path $env:LOCALAPPDATA 'shinatama-verify\D19\opencode'
$free = (Get-PSDrive C).Free
if ($free -lt 2GB) { throw "C: has only $([math]::Round($free/1GB,2)) GB free; stopping (rule: 2 GB)" }

# Real OpenCode stores: newest write time, read before and after (never opened).
$realStores = @((Join-Path $env:USERPROFILE '.local\share\opencode'), (Join-Path $env:USERPROFILE '.config\opencode'), (Join-Path $env:USERPROFILE '.local\state\opencode'))
function Get-RealNewest { foreach ($s in $realStores) { if (Test-Path $s) { $n = Get-ChildItem $s -Recurse -File -Force -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc | Select-Object -Last 1; "$s newest=$($n.LastWriteTimeUtc.ToString('o'))" } else { "$s absent" } } }
$realBefore = @(Get-RealNewest)

Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
$box = Join-Path $Root 'box'; $decoy = Join-Path $Root 'decoy'; $sets = Join-Path $Root 'sets'; $runs = Join-Path $Root 'runs'; $work = Join-Path $Root 'work'
New-Item -ItemType Directory -Force -Path $box, $decoy, $sets, $runs, $work | Out-Null
$report = New-Object System.Collections.Generic.List[string]
$report.Add("D19 OpenCode $(Get-Date -Format o)  exe=$ShnExe  C: free=$([math]::Round($free/1GB,1)) GB  mockPort=$MockPort")

# ---- Fixtures --------------------------------------------------------------------------------------------------
$base = New-D19BoxEnv -Box $box -Decoy $decoy
$homeCfg = Join-Path $box 'config\opencode'                       # $XDG_CONFIG_HOME/opencode = the home's global config
$homeSettings = Get-D19Settings $MockPort
$homeSettings['mcp'] = Get-D19Mcp 'd19-home-mcp'
Write-D19Json (Join-Path $homeCfg 'opencode.json') $homeSettings
Write-D19Plugin (Join-Path $homeCfg 'plugins\d19-home-plugin') 'd19-home'
Write-D19Skill (Join-Path $homeCfg 'skills') 'd19-home-skill'
Write-D19Skill (Join-Path $base['OPENCODE_TEST_HOME'] '.claude\skills') 'd19-user-claude-skill'
Write-D19Skill (Join-Path $base['OPENCODE_TEST_HOME'] '.agents\skills') 'd19-user-agents-skill'
Write-D19Skill (Join-Path $base['USERPROFILE'] '.claude\skills') 'd19-decoy-claude-skill'
Write-D19Skill (Join-Path $base['USERPROFILE'] '.agents\skills') 'd19-decoy-agents-skill'
# Shinatama-owned settings file (provider + model only, no plugins/skills/MCP), used with OPENCODE_CONFIG_DIR.
$shnSettings = Join-Path $Root 'shinatama\settings.json'
Write-D19Json $shnSettings (Get-D19Settings $MockPort)
$setInfo = @{}
foreach ($s in 'A', 'B') {
  $l = $s.ToLower()
  $dir = Join-Path $sets "$s\config"; $instHome = Join-Path $sets "$s\home"
  New-Item -ItemType Directory -Force -Path $instHome | Out-Null
  $plugin = Join-Path $dir "plugins\d19-$s"
  Write-D19Plugin $plugin "d19-$s"
  Write-D19Skill (Join-Path $dir 'skills') "d19-$l-skill"
  Write-D19Json (Join-Path $dir 'opencode.json') ([ordered]@{ mcp = (Get-D19Mcp "d19-$l-mcp") })
  # The same set as ONE file / inline content (for the OPENCODE_CONFIG and OPENCODE_CONFIG_CONTENT variants).
  $asFile = [ordered]@{ plugins = @($plugin); skills = @((Join-Path $dir 'skills')); mcp = (Get-D19Mcp "d19-$l-mcp") }
  $file = Join-Path $sets "$s\set-as-file.json"; Write-D19Json $file $asFile
  $setInfo[$s] = @{ Dir = $dir; Home = $instHome; File = $file; Content = ($asFile | ConvertTo-Json -Depth 30 -Compress)
    Expect = @("d19-$s-plugin-tool", "d19-$l-skill", "d19-$l-mcp-tool") }
}
$homeExpect = @('d19-home-plugin-tool', 'd19-home-skill', 'd19-home-mcp-tool')

# ---- Variants ---------------------------------------------------------------------------------------------------
# Set = extra variables, Unset = removed variables. Expect = the exact d19-* names the instance should see.
$A = $setInfo['A']; $B = $setInfo['B']
$variants = [ordered]@{
  'none-raw'      = @{ Unset = @('OPENCODE_DISABLE_CLAUDE_CODE'); Set = @{}; Expect = $homeExpect + @('d19-user-claude-skill', 'd19-user-agents-skill'); Note = 'box without OPENCODE_DISABLE_CLAUDE_CODE (warm-up)' }
  'none'          = @{ Set = @{}; Expect = $homeExpect + @('d19-user-agents-skill'); Note = 'box standard (OPENCODE_DISABLE_CLAUDE_CODE=1)' }
  'A-file'        = @{ Set = @{ OPENCODE_CONFIG = $A.File }; Expect = $A.Expect; Note = 'OPENCODE_CONFIG = set A as one file' }
  'A-content'     = @{ Set = @{ OPENCODE_CONFIG_CONTENT = $A.Content }; Expect = $A.Expect; Note = 'OPENCODE_CONFIG_CONTENT = set A inline' }
  'A-dir-only'    = @{ Set = @{ OPENCODE_CONFIG_DIR = $A.Dir }; Expect = $A.Expect; Note = 'OPENCODE_CONFIG_DIR alone (provider settings live only in the home file)' }
  'A-dir'         = @{ Set = @{ OPENCODE_CONFIG_DIR = $A.Dir; OPENCODE_CONFIG = $shnSettings }; Expect = $A.Expect; Note = 'OPENCODE_CONFIG_DIR = set A + OPENCODE_CONFIG = Shinatama settings' }
  'A-dir-homefile' = @{ Set = @{ OPENCODE_CONFIG_DIR = $A.Dir; OPENCODE_CONFIG = (Join-Path $homeCfg 'opencode.json') }; Expect = $A.Expect; Note = 'OPENCODE_CONFIG_DIR = set A + OPENCODE_CONFIG = the home''s own opencode.json' }
  'A-dir-home'    = @{ Set = @{ OPENCODE_CONFIG_DIR = $A.Dir; OPENCODE_CONFIG = $shnSettings; OPENCODE_TEST_HOME = $A.Home }; Expect = $A.Expect; Note = 'A-dir + OPENCODE_TEST_HOME = empty per-instance home' }
  'B-dir-home'    = @{ Set = @{ OPENCODE_CONFIG_DIR = $B.Dir; OPENCODE_CONFIG = $shnSettings; OPENCODE_TEST_HOME = $B.Home }; Expect = $B.Expect; Note = 'same for set B' }
  'none-after'    = @{ Set = @{}; Expect = $homeExpect + @('d19-user-agents-skill'); Note = 'box standard again, after all switch runs' }
}

$homeRoots = @((Join-Path $box 'config'), (Join-Path $box 'data'), (Join-Path $box 'state'), (Join-Path $box 'home'), $decoy)
$capture = Join-Path $runs 'capture.jsonl'
# The first turn makes ONE slow shell call (about 5 s): the turn's first request leaves before the MCP servers have
# connected (about 1.7 s after start), the second request (after the tool result) is built once they have.
$SlowShell = 'SHN-SHELL ping -n 6 127.0.0.1'
$mock = Start-D19Mock $MockPort $capture
$results = [ordered]@{}
try {
  foreach ($name in $variants.Keys) {
    if ($Only -and $name -notin $Only) { continue }
    $v = $variants[$name]
    $dir = Join-Path $runs $name; New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $envV = Copy-D19Env $base -Set $v.Set -Unset $(if ($v.Unset) { $v.Unset } else { @() })
    $before = Get-D19Hashes $homeRoots
    $setBefore = Get-D19Hashes @($A.Dir, $B.Dir, $A.Home, $B.Home)
    $nBefore = @(Get-Content $capture -ErrorAction SilentlyContinue).Count
    $run = Start-D19Process -Env $envV -Arguments @('run', '--standalone', '--model', $ShnModel, '--format', 'json', '--auto', '--title', "D19-$name") -Stdin "D19 probe turn $name $SlowShell" -WorkDir $work
    $res = Wait-D19Process $run -TimeoutSec 150 -OutPrefix (Join-Path $dir 'run')
    Start-Sleep -Milliseconds 500
    $lines = @(Get-Content $capture -ErrorAction SilentlyContinue | Select-Object -Skip $nBefore)
    [IO.File]::WriteAllLines((Join-Path $dir 'capture.jsonl'), [string[]]$lines, $ShnUtf8)
    $first = if ($lines.Count) { $lines[0] } else { '' }
    $firstObj = if ($first) { $first | ConvertFrom-Json } else { $null }
    $tools = if ($firstObj) { @($firstObj.body.tools | ForEach-Object { $_.name }) } else { @() }
    $seenFirst = Get-D19Seen $first
    $seenAll = Get-D19Seen ($lines -join "`n")
    $second = if ($lines.Count -ge 2) { $lines[1] } else { '' }
    $tools2 = if ($second) { @(($second | ConvertFrom-Json).body.tools | ForEach-Object { $_.name }) } else { @() }
    $seenSecond = Get-D19Seen $second
    $diff = @(Compare-D19Hashes $before (Get-D19Hashes $homeRoots))
    $setDiff = @(Compare-D19Hashes $setBefore (Get-D19Hashes @($A.Dir, $B.Dir, $A.Home, $B.Home)))
    $exact = (@($seenSecond | Sort-Object) -join ',') -eq (@($v.Expect | Sort-Object) -join ',')
    $results[$name] = [pscustomobject]@{ Exit = $res.ExitCode; Requests = $lines.Count; SeenFirst = $seenFirst; SeenSecond = $seenSecond; SeenAll = $seenAll; Exact = $exact; HomeDiff = $diff; SetDiff = $setDiff }
    $report.Add("== $name  [$($v.Note)]")
    $report.Add("   switches: " + (($v.Set.Keys | Sort-Object | ForEach-Object { "$_=$(if ($_ -eq 'OPENCODE_CONFIG_CONTENT') { '<set ' + $name.Substring(0,1) + ' json>' } else { $v.Set[$_] })" }) -join '  ') + $(if ($v.Unset) { "  unset: $($v.Unset -join ',')" } else { '' }))
    $report.Add("   exit=$($res.ExitCode) seconds=$($res.Seconds) requests=$($lines.Count) orphansKilled=$($res.OrphansKilled) reply=$([regex]::Match("$($res.Stdout)", 'H27-REPLY: [^"\\]*').Value)")
    $report.Add("   first request d19 names: $($seenFirst -join ', ')")
    $report.Add("   second request (after the slow tool call) d19 names: $($seenSecond -join ', ')")
    if ((@($seenAll) -join ',') -ne (@($seenSecond) -join ',')) { $report.Add("   all requests d19 names: $($seenAll -join ', ')") }
    $report.Add("   expected: $($v.Expect -join ', ')  => EXACT (second request)=$exact")
    $report.Add("   first request tools: $($tools -join ',')")
    $report.Add("   second request tools: $($tools2 -join ',')")
    $report.Add("   home files changed (excl. db/log/cache/storage): $($diff.Count)"); $diff | ForEach-Object { $report.Add("     $_") }
    if ($setDiff.Count) { $report.Add("   instance-set files changed: $($setDiff.Count)"); $setDiff | ForEach-Object { $report.Add("     $_") } }
    $errTail = @("$($res.Stderr)" -split "`n" | Where-Object { $_ -match '(?i)error|warn|fail|lock|busy' } | Select-Object -First 6)
    if ($errTail.Count) { $report.Add("   stderr (error/warn lines):"); $errTail | ForEach-Object { $report.Add("     $($_.Trim())") } }
  }

  # ---- Two instances at once on one OPENCODE_DB -----------------------------------------------------------------
  if (-not $Only -or 'concurrent' -in $Only) {
    $dir = Join-Path $runs 'concurrent'; New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $before = Get-D19Hashes $homeRoots
    $nBefore = @(Get-Content $capture -ErrorAction SilentlyContinue).Count
    $envA = Copy-D19Env $base -Set $variants['A-dir-home'].Set
    $envB = Copy-D19Env $base -Set $variants['B-dir-home'].Set
    $runA = Start-D19Process -Env $envA -Arguments @('run', '--standalone', '--model', $ShnModel, '--format', 'json', '--auto', '--title', 'D19-conc-A') -Stdin 'D19 conc A H27-HANG' -WorkDir $work
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline -and -not (@(Get-Content $capture -ErrorAction SilentlyContinue | Select-Object -Skip $nBefore) -match 'H27-HANG')) { Start-Sleep -Milliseconds 500 }
    $aHanging = [bool](@(Get-Content $capture | Select-Object -Skip $nBefore) -match 'H27-HANG')
    $runB = Start-D19Process -Env $envB -Arguments @('run', '--standalone', '--model', $ShnModel, '--format', 'json', '--auto', '--title', 'D19-conc-B') -Stdin "D19 conc B $SlowShell" -WorkDir $work
    $resB = Wait-D19Process $runB -TimeoutSec 150 -OutPrefix (Join-Path $dir 'B')
    $aStillRunning = -not $runA.Process.HasExited
    $db = & node (Join-Path $PSScriptRoot 'db-sessions.mjs') $base['OPENCODE_DB'] 2>&1 | Out-String
    [IO.File]::WriteAllText((Join-Path $dir 'db-sessions.json'), $db, $ShnUtf8)
    $resA = Wait-D19Process $runA -Kill -OutPrefix (Join-Path $dir 'A')
    $lines = @(Get-Content $capture | Select-Object -Skip $nBefore)
    [IO.File]::WriteAllLines((Join-Path $dir 'capture.jsonl'), [string[]]$lines, $ShnUtf8)
    $reqA = @($lines | Where-Object { $_ -match 'H27-HANG' }) | Select-Object -First 1
    $reqsB = @($lines | Where-Object { $_ -match 'D19 conc B' }); $reqB = $reqsB | Select-Object -Last 1
    $diff = @(Compare-D19Hashes $before (Get-D19Hashes $homeRoots))
    $lockErr = @("$($resA.Stderr)`n$($resB.Stderr)" -split "`n" | Where-Object { $_ -match '(?i)SQLITE_BUSY|database is locked|locked' })
    $report.Add("== concurrent: A (set A, H27-HANG) running while B (set B) runs to completion; same OPENCODE_DB")
    $report.Add("   A pid=$($runA.Pid) hanging-request-seen=$aHanging still-running-when-B-ended=$aStillRunning (then killed by PID)")
    $report.Add("   B pid=$($runB.Pid) exit=$($resB.ExitCode) seconds=$($resB.Seconds) reply=$([regex]::Match("$($resB.Stdout)", 'H27-REPLY: [^"\\]*').Value)")
    $report.Add("   A first request d19 names: $((Get-D19Seen "$reqA") -join ', ')")
    $report.Add("   B last request ($($reqsB.Count) requests) d19 names: $((Get-D19Seen "$reqB") -join ', ')")
    $report.Add("   lock errors in A/B stderr: $($lockErr.Count)"); $lockErr | Select-Object -First 4 | ForEach-Object { $report.Add("     $($_.Trim())") }
    $report.Add("   sessions in the box database (read-only, while A was running):"); ($db -split "`n") | Where-Object { $_ -match '"(id|title|table)"' } | ForEach-Object { $report.Add("     $($_.Trim())") }
    $report.Add("   home files changed: $($diff.Count)"); $diff | ForEach-Object { $report.Add("     $_") }
  }
} finally {
  Stop-D19Tree $mock.Id
  foreach ($p in $D19Started) { if (Get-Process -Id $p -ErrorAction SilentlyContinue) { $report.Add("stopping leftover recorded pid $p"); Stop-D19Tree $p } }
}
$realAfter = @(Get-RealNewest)
$report.Add("== real stores (newest write time) before: $($realBefore -join ' | ')")
$report.Add("   after: $($realAfter -join ' | ')   unchanged=$((($realBefore -join '|') -eq ($realAfter -join '|')))")
$report.Add("== summary")
foreach ($k in $results.Keys) { $r = $results[$k]; $report.Add(("   {0,-15} exit={1} exact={2} homeChanged={3} seen=[{4}]" -f $k, $r.Exit, $r.Exact, $r.HomeDiff.Count, ($r.SeenFirst -join ','))) }
$out = Join-Path $Root $(if ($Only) { "report-only-$($Only -join '-').txt" } else { 'report.txt' })
[IO.File]::WriteAllLines($out, [string[]]$report, $ShnUtf8)
$report
"report: $out"
