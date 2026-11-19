# D19, Codex leg, part 2 (after setup.ps1 and variants.ps1): the fallback "installed in the home, disabled by
# default, enabled per launch with -c". Installs d19-a and d19-b into the home (codex plugin add), then sets their
# config entries to enabled = false (Shinatama's one-time install step, done BEFORE the hashes are taken).
# Then, per instance, one mock turn and the d19-* names of its first request:
#   none        no switches (the home's own items)
#   A / B       exclude the home's items + enable the set's plugin + define the set's MCP server (all -c)
#   empty       exclude the home's items only
#   A-noexcl    enable A without excluding the home's items
#   A-prof-abs  -p <absolute path outside the home> (does --profile accept a file outside CODEX_HOME?)
#   A-prof-home -p d19setA with d19setA.config.toml written into the home beforehand (a home file per set)
#   A-ignore    exec --ignore-user-config + provider/features via -c + A
# Then concurrency in the one home: A on H27-HANG (kept running) while B runs to completion; thread/list over
# `codex app-server` (stdio) while A still runs; a plain instance resumes B's thread; A killed by its recorded PID.
# Rerunnable:  pwsh -NoProfile -File verify\D19\codex\sets.ps1
$D19PortBase = 47137
. (Join-Path $PSScriptRoot 'lib.ps1')
if (-not (Test-Path (Join-Path $D19Home 'config.toml'))) { throw "run setup.ps1 first" }
$removed = Enter-D19Env
$H27Port = $D19PortBase; $H27Sink = $D19PortBase + 1
$cfg = Join-Path $D19Home 'config.toml'
$q = { param($s) ConvertTo-D19TomlStr $s }
$outRoot = Join-Path $D19Out 'sets'; Remove-Item -Recurse -Force $outRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $outRoot | Out-Null
"codex $(& $H27Codex --version); CODEX_HOME $D19Home"

# ---- one-time install of the set plugins into the home, disabled by default ----
Start-H27Mock -Capture (Join-Path $outRoot 'install.capture.jsonl') -EgressLog (Join-Path $outRoot 'install.egress.jsonl') | Out-Null
try {
    foreach ($p in 'd19-a', 'd19-b') {
        $r = Invoke-H27Codex -ArgList @('plugin', 'add', "$p@d19") -OutPrefix (Join-Path $outRoot "install-$p") -Cwd $H27Work -TimeoutSec 120
        "install ${p}: exit $($r.Exit): $((Get-Content $r.Stdout -ErrorAction SilentlyContinue) -join ' | ')"
    }
} finally { Stop-H27Mock }
$t = Get-Content $cfg -Raw
foreach ($p in 'd19-a', 'd19-b') {
    $t = $t -replace "(?m)(\[plugins\.`"$p@d19`"\]\r?\nenabled = )true", '${1}false'
}
Set-Content -Path $cfg -Value $t -Encoding ASCII -NoNewline
"config.toml plugin entries now: $((Select-String -Path $cfg -Pattern '^\[plugins|^enabled' | ForEach-Object Line) -join ' ')"

# ---- the switches ----
$userSkill   = Join-Path $D19Home 'skills\d19-user-skill\SKILL.md'
$agentsSkill = Join-Path $D19Prof '.agents\skills\d19-agents-skill\SKILL.md'
$provider    = @('-c', "model_providers.h27.base_url=`"http://127.0.0.1:$H27Port/v1`"")
$excludeHome = @('-c', 'plugins.d19-home-plugin@d19.enabled=false',
                 '-c', 'mcp_servers.d19-home-mcp.enabled=false',
                 '-c', "skills.config=[{path=$(& $q $userSkill),enabled=false},{path=$(& $q $agentsSkill),enabled=false}]",
                 '-c', 'skills.bundled.enabled=false')   # Codex's own system skills (imagegen, openai-docs, skill-*)
$setA = @('-c', 'plugins.d19-a@d19.enabled=true') + (Get-D19McpArgs 'd19-a-mcp')
$setB = @('-c', 'plugins.d19-b@d19.enabled=true') + (Get-D19McpArgs 'd19-b-mcp')

# Profile files for set A: one outside the home, one inside (a file per set, written before the hashes).
$profBody = @"
[plugins."d19-home-plugin@d19"]
enabled = false
[plugins."d19-a@d19"]
enabled = true
[mcp_servers.d19-home-mcp]
enabled = false
[mcp_servers.d19-a-mcp]
command = $(& $q $D19Node)
args = [$(& $q $D19Stub), 'd19-a-mcp']
[skills.bundled]
enabled = false
[[skills.config]]
path = $(& $q $userSkill)
enabled = false
"@
$profAbs = Join-Path $D19Fx 'profA'
Set-Content -Path "$profAbs.config.toml" -Value $profBody -Encoding ASCII
Set-Content -Path (Join-Path $D19Home 'd19setA.config.toml') -Value $profBody -Encoding ASCII
$ignoreBase = @('--ignore-user-config', '-c', 'model="h27-mock"', '-c', 'model_provider="h27"', '-c', 'model_providers.h27.name="H27 mock"',
                '-c', "model_providers.h27.base_url=`"http://127.0.0.1:$H27Port/v1`"", '-c', 'model_providers.h27.wire_api="responses"',
                '-c', 'model_providers.h27.env_key="H27_API_KEY"', '-c', 'features.plugins=true', '-c', 'analytics.enabled=false',
                '-c', "skills.config=[{path=$(& $q $userSkill),enabled=false}]")

$matrix = [ordered]@{
    'none'        = @()
    'A'           = $excludeHome + $setA
    'B'           = $excludeHome + $setB
    'empty'       = $excludeHome
    'A-noexcl'    = $setA
    'A-prof-abs'  = @('-p', $profAbs)
    'A-prof-home' = @('-p', 'd19setA')
    'A-ignore'    = $ignoreBase + $setA
}
$common = @('exec', '--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox')
$rows = New-Object System.Collections.Generic.List[string]
foreach ($name in $matrix.Keys) {
    $out = Join-Path $outRoot $name; New-Item -ItemType Directory -Force -Path $out | Out-Null
    $marker = "D19-SET-$name"
    $argv = $common + $(if ($name -eq 'A-ignore') { @() } else { $provider }) + $matrix[$name] + "$marker hello"
    $before = Get-D19HomeHashes
    Start-H27Mock -Capture (Join-Path $out 'capture.jsonl') -EgressLog (Join-Path $out 'egress.jsonl') | Out-Null
    try { $r = Invoke-H27Codex -ArgList $argv -OutPrefix (Join-Path $out 'exec') -Cwd $H27Work -TimeoutSec 120 } finally { Stop-H27Mock }
    Start-Sleep -Milliseconds 300
    $diff = @(Compare-D19Hashes $before (Get-D19HomeHashes)); Save-SvLines (Join-Path $out 'home-diff.txt') $diff
    $i = Invoke-D19Inspect (Join-Path $out 'capture.jsonl') $marker (Join-Path $out 'first-request.json')
    $cfgDiff = @($diff | Where-Object { $_ -match ' cfg ' })
    $row = '{0,-12} exit {1,-3} first request d19: [{2}] | mcp tools: [{3}] | home cfg changes: {4} {5}' -f $name, $r.Exit,
        ($i.d19_all -join ' '), ((@($i.tool_names) | Where-Object { $_ -match 'd19' }) -join ' '), $cfgDiff.Count, ($cfgDiff -join '; ')
    $rows.Add($row); $row
    if (-not $i.found) {
        $e = ((Get-Content $r.Stderr, $r.Stdout -ErrorAction SilentlyContinue) | Where-Object { $_ } | Select-Object -Last 3) -join ' | '
        "    no request with marker; codex said: $($e.Substring(0, [Math]::Min(500, $e.Length)))"
    }
}
Save-SvLines (Join-Path $outRoot 'matrix-summary.txt') $rows

# ---- two instances of the one home at once ----
$out = Join-Path $outRoot 'concurrent'; New-Item -ItemType Directory -Force -Path $out | Out-Null
$cap = Join-Path $out 'capture.jsonl'
$before = Get-D19HomeHashes
Start-H27Mock -Capture $cap -EgressLog (Join-Path $out 'egress.jsonl') | Out-Null
$pA = $null
try {
    $argA = ConvertTo-H27ArgLine ($common + $provider + $excludeHome + $setA + 'D19-CONC-A H27-HANG keep streaming')
    $pA = Start-Process -FilePath $H27Codex -ArgumentList $argA -WorkingDirectory $H27Work -PassThru -NoNewWindow `
        -RedirectStandardInput (Join-Path $H27Logs 'empty-stdin.txt') -RedirectStandardOutput (Join-Path $out 'A.stdout.jsonl') -RedirectStandardError (Join-Path $out 'A.stderr.txt')
    "instance A started: PID $($pA.Id) (recorded; the only process this step kills)"
    $pA.Id | Set-Content (Join-Path $out 'A.pid.txt')
    for ($k = 0; $k -lt 120 -and -not ((Test-Path $cap) -and (Select-String -Path $cap -Pattern 'D19-CONC-A' -Quiet)); $k++) { Start-Sleep -Milliseconds 500 }
    "A's request reached the mock: $([bool](Select-String -Path $cap -Pattern 'D19-CONC-A' -Quiet)); A still running: $(-not $pA.HasExited)"
    $rB = Invoke-H27Codex -ArgList ($common + $provider + $excludeHome + $setB + 'D19-CONC-B hello') -OutPrefix (Join-Path $out 'B') -Cwd $H27Work -TimeoutSec 120
    $idA = Get-H27ThreadId (Join-Path $out 'A.stdout.jsonl'); $idB = Get-H27ThreadId $rB.Stdout
    "instance B: exit $($rB.Exit), thread $idB; A thread $idA, A still running: $(-not $pA.HasExited)"
    $ErrorActionPreference = 'Continue'
    & node (Join-Path $SvRepo 'verify\H27\codex\app-server-drive.mjs') --codex $H27Codex --cwd $H27Work --log (Join-Path $out 'thread-list.jsonl') --action list 2>&1 |
        ForEach-Object { "$_" } | Tee-Object -FilePath (Join-Path $out 'thread-list.out.txt') | Select-Object -First 12 | ForEach-Object { "  thread/list: $_" }
    $ErrorActionPreference = 'Stop'
    "rollouts in the home: A $([bool](Find-H27Rollout $idA)), B $([bool](Find-H27Rollout $idB)); A still running during the list: $(-not $pA.HasExited)"
    $rR = Invoke-H27Codex -ArgList (@($common[0], 'resume') + $common[1..3] + $provider + @($idB, 'D19-RESUME-B-PLAIN hello')) -OutPrefix (Join-Path $out 'resumeB-plain') -Cwd $H27Work -TimeoutSec 120
    "plain instance (no set switches) resumes B's thread: exit $($rR.Exit)"
} finally {
    if ($pA -and -not $pA.HasExited) { & taskkill.exe /T /F /PID $pA.Id | Out-Null; $pA.WaitForExit(10000) | Out-Null; "instance A (PID $($pA.Id)) killed by its recorded PID" }
    Stop-H27Mock
}
Start-Sleep -Milliseconds 300
$diff = @(Compare-D19Hashes $before (Get-D19HomeHashes)); Save-SvLines (Join-Path $out 'home-diff.txt') $diff
foreach ($m in 'D19-CONC-A', 'D19-CONC-B', 'D19-RESUME-B-PLAIN') {
    $i = Invoke-D19Inspect $cap $m (Join-Path $out "first-request.$m.json")
    '{0,-20} first request d19: [{1}] | mcp tools: [{2}]' -f $m, ($i.d19_all -join ' '), ((@($i.tool_names) | Where-Object { $_ -match 'd19' }) -join ' ')
}
"concurrent run home cfg changes: $(@($diff | Where-Object { $_ -match ' cfg ' }).Count) $(($diff | Where-Object { $_ -match ' cfg ' }) -join '; ')"
"concurrent run state changes (excluded class): $(@($diff | Where-Object { $_ -match ' state ' }).Count)"
"raw output: $outRoot"
