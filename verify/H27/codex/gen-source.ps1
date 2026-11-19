# H27 codex leg - generate one authentic Codex session against the mock, non-interactively:
#   T1  `codex exec` H27-TOOLS (two parallel shell reads), T2 `codex exec resume`,
#   compaction: Codex's own, over `codex app-server` (thread/compact/start), T3 `exec resume`,
#   T4a H27-HANG over app-server, interrupted gracefully after 15 s (turn/interrupt, what Esc does),
#   T4b H27-HANG with `exec resume`, process tree killed after 15 s (the interrupted last turn).
# Resets the scratch home first; copies the rollout, capture and step logs to
# verify\H27\out\<date>\codex\source\. Rerunnable without edits:
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\codex\gen-source.ps1
param([int]$HangSec = 15)
. (Join-Path $PSScriptRoot 'env.ps1')

function Invoke-H27Drive([string]$Thread, [string]$Name, [string[]]$Extra) {
    # app-server-drive.mjs with its protocol log and console output beside the other step logs.
    $ErrorActionPreference = 'Continue'  # PS 5.1 turns redirected native stderr into errors
    $lines = & node (Join-Path $H27Dir 'app-server-drive.mjs') --codex $H27Codex --thread $Thread --cwd $H27Work `
        --log (Join-Path $steps "$Name.app-server.jsonl") @Extra 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $lines | Set-Content -Path (Join-Path $steps "$Name.out.txt") -Encoding UTF8  # Tee-Object would write UTF-16
    $lines | Out-Host
    return $code
}

$realBefore = Get-H27RealStoreStamp
Remove-Item -Recurse -Force $H27Scratch -ErrorAction SilentlyContinue
$cfg = Write-H27Config -Force
$alpha = Join-Path $H27Work 'alpha.txt'
$beta  = Join-Path $H27Work 'beta.txt'
Set-Content -Path $alpha -Value 'ALPHA-CONTENT-42' -Encoding ASCII
Set-Content -Path $beta  -Value 'BETA-CONTENT-17' -Encoding ASCII
$src   = Join-Path $H27Out 'source'
$steps = Join-Path $src 'steps'
Remove-Item -Recurse -Force $src -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $steps | Out-Null
$capture = Join-Path $H27Scratch 'capture-gen.jsonl'
$egress  = Join-Path $H27Scratch 'egress-gen.jsonl'
"codex: $H27Codex ($(& $H27Codex --version))"
"CODEX_HOME: $env:CODEX_HOME (config $cfg)"
"work: $H27Work"

$common = @('--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox')
Start-H27Mock -Capture $capture -EgressLog $egress
try {
    $r = Invoke-H27Codex -ArgList (@('exec') + $common + "H27-TOOLS read $alpha and $beta") -OutPrefix (Join-Path $steps 't1')
    $id = Get-H27ThreadId $r.Stdout
    "T1 exec: exit $($r.Exit), thread $id"
    if (-not $id) { throw "T1 printed no thread id; see $($r.Stdout) and $($r.Stderr)" }

    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + 'H27-T2 say hello') -OutPrefix (Join-Path $steps 't2')
    "T2 exec resume: exit $($r.Exit)"

    $code = Invoke-H27Drive -Thread $id -Name 'compact' -Extra @('--action', 'compact')
    "compaction (app-server thread/compact/start): exit $code"

    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + 'H27-T3 after compaction') -OutPrefix (Join-Path $steps 't3')
    "T3 exec resume: exit $($r.Exit)"

    $code = Invoke-H27Drive -Thread $id -Name 't4a-interrupt' -Extra @('--action', 'interrupt', '--prompt', 'H27-HANG T4a', '--after', "$HangSec")
    "T4a app-server turn/start H27-HANG + turn/interrupt after ${HangSec}s: exit $code"

    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + 'H27-HANG T4b') -OutPrefix (Join-Path $steps 't4b-kill') -KillAfterSec $HangSec
    "T4b exec resume H27-HANG, process tree killed: $($r.Exit)"
} finally {
    Stop-H27Mock
}
Start-Sleep -Milliseconds 500

$rollout = Find-H27Rollout $id
if (-not $rollout) { throw "no rollout for $id under $H27Home\sessions" }
Copy-Item $rollout.FullName, $capture, $egress, $cfg -Destination $src -ErrorAction SilentlyContinue
& node (Join-Path $H27Dir 'dump-state.mjs') --home $H27Home --thread $id --out (Join-Path $src 'state-dump.json')
Get-ChildItem $H27Home -Recurse -File | Where-Object { $_.FullName -notmatch '\\skills\\' } |
    ForEach-Object { '{0,10}  {1}' -f $_.Length, $_.FullName.Substring($H27Home.Length + 1) } |
    Set-Content -Path (Join-Path $src 'home-files.txt') -Encoding UTF8

$realAfter = Get-H27RealStoreStamp
$egressCount = if (Test-Path $egress) { @(Get-Content $egress).Count } else { 0 }
""
"session id: $id"
"rollout:    $($rollout.FullName)"
"copied to:  $src"
"capture:    $(Join-Path $src 'capture-gen.jsonl') ($(@(Get-Content $capture).Count) requests)"
"egress attempts refused by the sink: $egressCount (log $(Join-Path $src 'egress-gen.jsonl'))"
"real store before: index $($realBefore.IndexMtime), today $($realBefore.TodayCount) rollouts, newest $($realBefore.Newest)"
"real store after:  index $($realAfter.IndexMtime), today $($realAfter.TodayCount) rollouts, newest $($realAfter.Newest)"
"scratch root: $H27Scratch"
Get-ChildItem $H27Scratch | ForEach-Object { "  $($_.Name)" }
