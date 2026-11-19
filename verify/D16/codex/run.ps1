# D16, Codex leg: obtain Codex into an arbitrary folder, side by side, with self-update off, and roll back.
#  1. npm install --prefix <D16>\versions\<ver> @openai/codex@<ver> (cache and logs kept inside the D16 folder);
#     real user dirs checked for entries changed during each install (names only).
#  2. 0.164.0 runs a turn, then a hanging turn stays running while 0.163.0 is installed and runs a turn on the
#     SAME private home (rollback onto a home last written by the newer version).
#  3. Update check: the TUI of 0.163.0 started (hidden console, killed after N s) with check_for_update_on_startup
#     true and false; egress sink log + version.json show whether a check was attempted. `codex doctor` for both.
#  4. `codex update` through the npm wrapper with update-on-startup off: npm pointed at a scratch prefix and at the
#     sink as registry, so nothing can be installed anywhere real. The standalone route (install.ps1) is NOT run.
# Rerunnable:  pwsh -NoProfile -File verify\D16\codex\run.ps1 [-Versions 0.164.0,0.163.0] [-TuiSec 15]
param([string[]]$Versions = @('0.164.0', '0.163.0'), [int]$TuiSec = 15)
$SvRow = 'D16'; $SvPortBase = 4252
. (Join-Path $PSScriptRoot '..\..\D15\codex\lib.ps1')
$removed = Remove-SvAgentMarkers
$out = Join-Path $SvScratch 'out'
Remove-Item -Recurse -Force $out, (Join-Path $SvScratch 'versions'), (Join-Path $SvScratch 'npm-cache') -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $out, $H27Logs, $H27Work | Out-Null
$proxyVars = 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY'
$saved = @{}; foreach ($k in $proxyVars) { $saved[$k] = (Get-Item "Env:$k" -ErrorAction SilentlyContinue).Value }
"markers removed: $($removed -join ', ')"

function Install-D16([string]$Ver) {
    # npm registry only: proxy variables off for this step, back on afterwards.
    $dir = Join-Path $SvScratch "versions\$Ver"; New-Item -ItemType Directory -Force -Path $dir | Out-Null
    foreach ($k in $proxyVars) { Remove-Item "Env:$k" -ErrorAction SilentlyContinue }
    $t0 = Get-Date; Start-Sleep -Milliseconds 1100
    $env:USERPROFILE = $SvRealProfile; $env:APPDATA = $SvRealAppData; $env:LOCALAPPDATA = $SvRealLocal
    $log = Join-Path $out "install-$Ver.txt"
    $ErrorActionPreference = 'Continue'
    & npm.cmd install --prefix $dir --cache (Join-Path $SvScratch 'npm-cache') --no-audit --no-fund --loglevel=http "@openai/codex@$Ver" *> $log
    $code = $LASTEXITCODE; $ErrorActionPreference = 'Stop'
    $sec = [int]((Get-Date) - $t0).TotalSeconds
    foreach ($k in $proxyVars) { if ($saved[$k]) { Set-Item "Env:$k" $saved[$k] } }
    Save-SvLines (Join-Path $out "install-$Ver.real-changed.txt") (Get-SvRealChanges $t0)
    Save-SvLines (Join-Path $out "install-$Ver.tree.txt") (Get-SvTree $dir)
    $exe = Get-ChildItem $dir -Recurse -Filter codex.exe | Select-Object -First 1
    Write-Host "install $Ver -> $dir : npm exit $code, ${sec}s, native exe $($exe.FullName.Substring($dir.Length + 1)), real-dir entries changed: $(@(Get-Content (Join-Path $out "install-$Ver.real-changed.txt")).Count)"
    return [pscustomobject]@{ Ver = $Ver; Dir = $dir; Exe = $exe.FullName; Js = (Join-Path $dir 'node_modules\@openai\codex\bin\codex.js') }
}
function Use-Home([string]$Name, [bool]$Check) {
    $d = Enter-SvDecoy (Join-Path $SvScratch "decoy-$Name")
    $cfg = Write-H27Config -Force
    if ($Check) { (Get-Content $cfg) -replace '^check_for_update_on_startup = .*$', 'check_for_update_on_startup = true' | Set-Content $cfg -Encoding ASCII }
    return $d
}

$new = Install-D16 $Versions[0]
$common = @('--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox')
$homeA = Use-Home 'shared' $false
Start-H27Mock -Capture (Join-Path $out 'capture-side-by-side.jsonl') -EgressLog (Join-Path $out 'egress-side-by-side.jsonl') | Out-Null
try {
    $H27Codex = $new.Exe
    "$($new.Ver) --version: $(& $new.Exe --version)"
    $r = Invoke-H27Codex -ArgList (@('exec') + $common + "D16-T1 on $($new.Ver)") -OutPrefix (Join-Path $out 't1-new')
    $id = Get-H27ThreadId $r.Stdout; "T1 $($new.Ver) exec: exit $($r.Exit), thread $id"
    # A hanging turn of the newer version stays running during the second install.
    $hang = Start-Process -FilePath $new.Exe -ArgumentList (ConvertTo-H27ArgLine (@('exec') + $common + 'H27-HANG keep running')) -WorkingDirectory $H27Work `
        -PassThru -NoNewWindow -RedirectStandardInput (Join-Path $H27Logs 'empty-stdin.txt') `
        -RedirectStandardOutput (Join-Path $out 'hang-new.stdout.jsonl') -RedirectStandardError (Join-Path $out 'hang-new.stderr.txt')
    Start-Sleep -Seconds 3
    "running $($new.Ver) pid $($hang.Id) (exited: $($hang.HasExited))"
    $old = Install-D16 $Versions[1]
    $env:USERPROFILE = $homeA.Home; $env:APPDATA = $homeA.AppData; $env:LOCALAPPDATA = $homeA.Local
    "$($old.Ver) --version: $(& $old.Exe --version)"
    $H27Codex = $old.Exe
    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + "D16-T2 rollback to $($old.Ver)") -OutPrefix (Join-Path $out 't2-old-resume')
    "T2 $($old.Ver) exec resume of the $($new.Ver) thread (same home): exit $($r.Exit); last: $((Get-Content $r.Stdout | Select-Object -Last 1))"
    $r = Invoke-H27Codex -ArgList (@('exec') + $common + "D16-T3 new thread on $($old.Ver)") -OutPrefix (Join-Path $out 't3-old-new')
    "T3 $($old.Ver) exec new thread: exit $($r.Exit)"
    "$($new.Ver) pid $($hang.Id) still running after the other install and runs: $(-not $hang.HasExited)"
    & taskkill.exe /T /F /PID $hang.Id | Out-Null
    $H27Codex = $new.Exe
    $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + "D16-T4 back on $($new.Ver)") -OutPrefix (Join-Path $out 't4-new-again')
    "T4 $($new.Ver) again on the same thread: exit $($r.Exit)"
} finally { Stop-H27Mock }

# Update check on startup (TUI only), on the older version so that a newer one exists.
foreach ($check in $true, $false) {
    $name = "update-check-$check"
    $d = Use-Home $name $check
    $egress = Join-Path $out "$name.egress.jsonl"
    Start-H27Mock -Capture (Join-Path $out "$name.capture.jsonl") -EgressLog $egress | Out-Null
    try {
        $p = Start-Process -FilePath $old.Exe -WorkingDirectory $H27Work -PassThru -WindowStyle Hidden
        Start-Sleep -Seconds $TuiSec
        $alive = -not $p.HasExited
        & taskkill.exe /T /F /PID $p.Id 2>$null | Out-Null
        $ErrorActionPreference = 'Continue'
        & $old.Exe doctor *> (Join-Path $out "$name.doctor.txt")
        $ErrorActionPreference = 'Stop'
    } finally { Stop-H27Mock }
    $vj = Test-Path (Join-Path $d.Private 'version.json')
    $hits = @(Get-Content $egress -ErrorAction SilentlyContinue)
    "TUI $($old.Ver), check_for_update_on_startup=$($check.ToString().ToLower()): alive after ${TuiSec}s $alive; egress attempts $($hits.Count) $($hits -join ' '); version.json written: $vj; outside private home: $(@(Get-SvTree $d.Root -Exclude $d.Private).Count)"
    "  doctor: $((Select-String -Path (Join-Path $out "$name.doctor.txt") -Pattern 'update|latest version' | ForEach-Object { $_.Line.Trim() }) -join ' | ')"
}

# `codex update` via the npm wrapper (sets CODEX_MANAGED_BY_NPM), update-on-startup off, npm fenced to scratch + sink.
$d = Use-Home 'codex-update' $false
$egress = Join-Path $out 'codex-update.egress.jsonl'
Start-H27Mock -Capture (Join-Path $out 'codex-update.capture.jsonl') -EgressLog $egress | Out-Null
try {
    $env:npm_config_prefix = Join-Path $SvScratch 'npm-global-fence'
    $env:npm_config_cache = Join-Path $SvScratch 'npm-cache-fence'
    $env:npm_config_registry = "http://127.0.0.1:$H27Sink/"
    $p = Start-Process -FilePath node -ArgumentList "`"$($old.Js)`" update" -WorkingDirectory $H27Work -PassThru -NoNewWindow `
        -RedirectStandardInput (Join-Path $H27Logs 'empty-stdin.txt') -RedirectStandardOutput (Join-Path $out 'codex-update.stdout.txt') -RedirectStandardError (Join-Path $out 'codex-update.stderr.txt')
    if (-not $p.WaitForExit(120000)) { & taskkill.exe /T /F /PID $p.Id | Out-Null }
    "codex update ($($old.Ver), npm wrapper, check_for_update_on_startup=false): exit $($p.ExitCode)"
    "  stdout: $((Get-Content (Join-Path $out 'codex-update.stdout.txt')) -join ' | ')"
    "  stderr: $((Get-Content (Join-Path $out 'codex-update.stderr.txt') | Select-Object -First 8) -join ' | ')"
    "  egress: $((Get-Content $egress) -join ' ')"
    "  fenced npm prefix contents: $(@(Get-SvTree $env:npm_config_prefix).Count) file(s)"
} finally {
    Stop-H27Mock
    foreach ($k in 'npm_config_prefix', 'npm_config_cache', 'npm_config_registry') { Remove-Item "Env:$k" -ErrorAction SilentlyContinue }
}
"raw output: $out"
