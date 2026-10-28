# D15, Codex leg: does Codex started with ONLY CODEX_HOME pointing at a private home write anywhere else?
# Decoy-home method: USERPROFILE/HOME, APPDATA, LOCALAPPDATA point at an empty decoy; the private home sits
# under the decoy LOCALAPPDATA (where Shinatama would keep it); mock model + egress sink from the H27 bench.
# Session: T1 plain turn (exec), T2 H27-TOOLS turn (exec resume), T3 resume turn (exec resume), app-server
# thread/list, `login status`, `app-server daemon version` (which daemon socket does a private home look for?).
# Then lists every file under the decoy (pass = all inside the private home) and real-dir entries changed in
# the window (names only). Rerunnable:  pwsh -NoProfile -File verify\D15\codex\run.ps1 [-Sandbox unelevated]
param([ValidateSet('none', 'unelevated')][string]$Sandbox = 'none')
$SvRow = 'D15'; $SvPortBase = 4251
. (Join-Path $PSScriptRoot 'lib.ps1')

$tag    = if ($Sandbox -eq 'none') { 'full-access' } else { "sandbox-$Sandbox" }
$outDir = Join-Path $SvScratch "out\$tag"
Remove-Item -Recurse -Force $outDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $outDir, $H27Logs, $H27Work | Out-Null
$removed = Remove-SvAgentMarkers
$decoy = Enter-SvDecoy (Join-Path $SvScratch "decoy-$tag")
$cfg = Write-H27Config -Force
if ($Sandbox -ne 'none') {
    # Same config, but the real Windows sandbox instead of danger-full-access (unelevated: no admin, no OS users).
    (Get-Content $cfg) -replace '^sandbox_mode = .*$', 'sandbox_mode = "workspace-write"' | Set-Content $cfg -Encoding ASCII
    Add-Content $cfg "`n[windows]`nsandbox = `"$Sandbox`"" -Encoding ASCII
}
$alpha = Join-Path $H27Work 'alpha.txt'; $beta = Join-Path $H27Work 'beta.txt'
Set-Content $alpha 'ALPHA-CONTENT-42' -Encoding ASCII; Set-Content $beta 'BETA-CONTENT-17' -Encoding ASCII

"codex: $H27Codex ($(& $H27Codex --version))"
"markers removed: $($removed -join ', ')"
"decoy: $($decoy.Root)"; "CODEX_HOME (private): $env:CODEX_HOME"; "sandbox: $Sandbox"
$capture = Join-Path $outDir 'capture.jsonl'; $egress = Join-Path $outDir 'egress.jsonl'
$t0 = Get-Date
Start-Sleep -Milliseconds 1100
$common = @('--json', '--skip-git-repo-check')
if ($Sandbox -eq 'none') { $common += '--dangerously-bypass-approvals-and-sandbox' }  # else config: workspace-write, approval never
Start-H27Mock -Capture $capture -EgressLog $egress
try {
    $r = Invoke-H27Codex -ArgList (@('exec') + $common + 'D15-T1 plain hello') -OutPrefix (Join-Path $outDir 't1')
    $id = Get-H27ThreadId $r.Stdout
    "T1 exec: exit $($r.Exit), thread $id"
    if (-not $id) { throw "T1 printed no thread id; see $($r.Stderr)" }
    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + "H27-TOOLS read $alpha and $beta") -OutPrefix (Join-Path $outDir 't2')
    "T2 exec resume H27-TOOLS: exit $($r.Exit)"
    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + 'D15-T3 resumed turn') -OutPrefix (Join-Path $outDir 't3')
    "T3 exec resume: exit $($r.Exit)"
    $ErrorActionPreference = 'Continue'
    & node (Join-Path $SvRepo 'verify\H27\codex\app-server-drive.mjs') --codex $H27Codex --cwd $H27Work --log (Join-Path $outDir 'app-server-list.jsonl') --action list 2>&1 |
        ForEach-Object { "$_" } | Set-Content (Join-Path $outDir 'app-server-list.out.txt') -Encoding UTF8
    "app-server thread/list: exit $LASTEXITCODE"
    $r = Invoke-H27Codex -ArgList @('login', 'status') -OutPrefix (Join-Path $outDir 'login-status') -TimeoutSec 60
    "login status: exit $($r.Exit): $((Get-Content $r.Stdout, $r.Stderr -ErrorAction SilentlyContinue) -join ' | ')"
    $r = Invoke-H27Codex -ArgList @('app-server', 'daemon', 'version') -OutPrefix (Join-Path $outDir 'daemon-version') -TimeoutSec 60
    "app-server daemon version: exit $($r.Exit): $((Get-Content $r.Stdout, $r.Stderr -ErrorAction SilentlyContinue) -join ' | ')"
    $ErrorActionPreference = 'Stop'
} finally {
    Stop-H27Mock
}
Start-Sleep -Milliseconds 800

$all     = Get-SvTree $decoy.Root
$inside  = Get-SvTree $decoy.Private
$outside = Get-SvTree $decoy.Root -Exclude $decoy.Private
$real    = Get-SvRealChanges $t0
Save-SvLines (Join-Path $outDir 'decoy-all.txt') $all
Save-SvLines (Join-Path $outDir 'private-home.txt') $inside
Save-SvLines (Join-Path $outDir 'decoy-outside-private.txt') $outside
Save-SvLines (Join-Path $outDir 'real-changed-in-window.txt') $real
$egressCount = @(Get-Content $egress -ErrorAction SilentlyContinue).Count
""
"files under decoy: $(@($all).Count); inside private home: $(@($inside).Count); OUTSIDE private home: $(@($outside).Count)"
if (@($outside).Count) { "LEAK(S):"; $outside }
"egress attempts refused: $egressCount; model requests: $(@(Get-Content $capture).Count)"
"real-dir entries changed in the window (names; attribute before blaming Codex): $(@($real).Count)"
$real | ForEach-Object { "  $_" }
"raw output: $outDir"
