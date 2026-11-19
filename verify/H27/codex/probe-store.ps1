# H27 codex leg - which files Codex needs to list and resume a thread, and which store wins on resume.
# Works on copies of one session of the scratch home, each in its own probe home under the scratch root
# (never the main scratch home, never a real one):
#   P1 rollout-only:  config.toml + the rollout at its sessions\YYYY\MM\DD path, nothing else
#                     -> thread/list, exec resume (capture), thread/list again.
#   P2 other-date:    the same rollout alone under sessions\2020\01\01 -> exec resume (capture).
#   P3 db-path-wins:  home A = full copy of the main home, its threads.rollout_path pointed at A's rollout;
#                     home B = full copy of A with "H27-T2 say hello" rewritten in B's rollout only
#                     -> exec resume in B: which rollout is read and appended to, B's own or A's?
#   P4 projection:    full copy C, rollout_path pointed at C's rollout, the same rewrite in C's rollout
#                     -> exec resume: does the model see the rollout or thread_history_1.sqlite?
#   P5 dangling:      full copy D, rollout_path pointed at a file that does not exist -> exec resume.
#   P6 message-only:  rollout-only home whose `compacted` record keeps only {message} (no
#                     replacement_history) -> exec resume: how Codex rebuilds history without it.
# Writes verify\H27\out\<date>\codex\probe\. Rerunnable without edits:
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\codex\probe-store.ps1 -SessionId <id>
param([Parameter(Mandatory)][string]$SessionId)
. (Join-Path $PSScriptRoot 'env.ps1')

$mainHome = $H27Home
$rollout = Find-H27Rollout $SessionId
if (-not $rollout) { throw "no rollout for $SessionId under $mainHome\sessions (run gen-source.ps1 first)" }
$rel = $rollout.FullName.Substring($mainHome.Length + 1)
$out = Join-Path $H27Out 'probe'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $out | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding($false)
$ErrorActionPreference = 'Continue'  # PS 5.1 turns native stderr (node's SQLite warning) into errors

function Use-ProbeHome([string]$Name) {
    $root = Join-Path $H27Scratch "probe-$Name"
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
    $script:H27Home = Join-Path $root 'home'
    $env:CODEX_HOME = $script:H27Home
    New-Item -ItemType Directory -Force -Path $script:H27Home | Out-Null
    $script:H27Home
}

function Copy-Home([string]$From, [string]$To) {
    Copy-Item (Join-Path $From '*') $To -Recurse -Force
    Get-ChildItem $To -Recurse -File -Filter '*.lock' | Remove-Item -Force
}

function Set-RolloutPath([string]$HomeDir, [string]$Path) {
    & node --no-warnings (Join-Path $H27Dir 'set-rollout-path.mjs') --db (Join-Path $HomeDir 'state_5.sqlite') --thread $SessionId --path $Path 2>&1 | ForEach-Object { "  $_" }
}

function Edit-Rollout([string]$Path) {
    [IO.File]::WriteAllText($Path, [IO.File]::ReadAllText($Path).Replace('H27-T2 say hello', 'H27-T2 EDITED-IN-ROLLOUT'), $utf8)
}

function Show-Markers([string]$Label, [string]$Path) {
    $t = if (Test-Path $Path) { [IO.File]::ReadAllText($Path) } else { '' }
    $probes = ([regex]::Matches($t, 'H27-PROBE [a-z0-9-]+') | ForEach-Object { $_.Value } | Sort-Object -Unique) -join ', '
    "  $Label rollout: T2 original=$($t.Contains('H27-T2 say hello')) edited=$($t.Contains('EDITED-IN-ROLLOUT')); probe turns appended: [$probes]"
}

function Invoke-Probe([string]$Name, [string]$Prompt) {
    Start-H27Mock -Capture (Join-Path $out "$Name.capture.jsonl") -EgressLog (Join-Path $out "$Name.egress.jsonl") | Out-Null
    try {
        $r = Invoke-H27Codex -ArgList @('exec', 'resume', '--json', '--skip-git-repo-check',
            '--dangerously-bypass-approvals-and-sandbox', $SessionId, $Prompt) -OutPrefix (Join-Path $out $Name) -TimeoutSec 120
    } finally { Stop-H27Mock }
    $cap = Join-Path $out "$Name.capture.jsonl"
    $sent = if ((Test-Path $cap) -and (Get-Item $cap).Length) {
        $t = [IO.File]::ReadAllText($cap)
        "requests $(@(Get-Content $cap).Count); T2 text the model saw: original=$($t.Contains('H27-T2 say hello')) edited=$($t.Contains('EDITED-IN-ROLLOUT'))"
    } else { 'no request reached the mock' }
    "  $Name exec resume: exit $($r.Exit); $sent"
    $err = (Get-Content (Join-Path $out "$Name.stderr.txt") -ErrorAction SilentlyContinue | Where-Object { $_ -notmatch 'Reading additional input' } | Select-Object -First 3) -join ' | '
    if ($err) { "  stderr: $err" }
}

function Invoke-List([string]$Name) {
    $lines = & node (Join-Path $H27Dir 'app-server-drive.mjs') --codex $H27Codex --cwd $H27Work --action list `
        --log (Join-Path $out "$Name.app-server.jsonl") 2>&1 | ForEach-Object { "  $_" }
    $lines | Set-Content -Path (Join-Path $out "$Name.out.txt") -Encoding UTF8  # Tee-Object would write UTF-16
    $lines
}

function Save-HomeFiles([string]$Name) {
    Get-ChildItem $H27Home -Recurse -File | Where-Object { $_.FullName -notmatch '\\(skills|tmp)\\' } |
        ForEach-Object { '{0,10}  {1}' -f $_.Length, $_.FullName.Substring($H27Home.Length + 1) } |
        Set-Content -Path (Join-Path $out "$Name.home-files.txt") -Encoding ASCII
}

"source session $SessionId, rollout $($rollout.FullName)"

"== P1 rollout-only"
$h = Use-ProbeHome 'p1-rollout-only'
Write-H27Config -Force | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path (Join-Path $h $rel)) | Out-Null
Copy-Item $rollout.FullName (Join-Path $h $rel)
Invoke-List 'p1-list-before'
Invoke-Probe 'p1-resume' 'H27-PROBE p1-rollout-only'
Show-Markers 'P1' (Join-Path $h $rel)
Invoke-List 'p1-list-after'
Save-HomeFiles 'p1-after'

"== P2 other-date"
$h = Use-ProbeHome 'p2-other-date'
Write-H27Config -Force | Out-Null
$other = Join-Path $h ('sessions\2020\01\01\' + $rollout.Name)
New-Item -ItemType Directory -Force -Path (Split-Path $other) | Out-Null
Copy-Item $rollout.FullName $other
Invoke-Probe 'p2-resume' 'H27-PROBE p2-other-date'
Show-Markers 'P2' $other

"== P3 db-path-wins"
$a = Use-ProbeHome 'p3-a'
Copy-Home $mainHome $a
Set-RolloutPath $a (Join-Path $a $rel)
$b = Use-ProbeHome 'p3-b'
Copy-Home $a $b
Edit-Rollout (Join-Path $b $rel)
Invoke-Probe 'p3-resume-in-b' 'H27-PROBE p3-db-path-wins'
Show-Markers 'P3 home A' (Join-Path $a $rel)
Show-Markers 'P3 home B' (Join-Path $b $rel)

"== P4 rollout vs thread_history projection"
$c = Use-ProbeHome 'p4-projection'
Copy-Home $mainHome $c
Set-RolloutPath $c (Join-Path $c $rel)
Edit-Rollout (Join-Path $c $rel)
Invoke-Probe 'p4-resume' 'H27-PROBE p4-projection'
Show-Markers 'P4' (Join-Path $c $rel)

"== P5 dangling rollout_path"
$d = Use-ProbeHome 'p5-dangling'
Copy-Home $mainHome $d
Set-RolloutPath $d (Join-Path $d 'sessions\missing\rollout-missing.jsonl')
Invoke-Probe 'p5-resume' 'H27-PROBE p5-dangling'
Show-Markers 'P5' (Join-Path $d $rel)
Save-HomeFiles 'p5-after'

"== P6 compacted record reduced to {message} (rollout-only home)"
$h = Use-ProbeHome 'p6-compacted-message-only'
Write-H27Config -Force | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path (Join-Path $h $rel)) | Out-Null
& node (Join-Path $H27Dir 'strip-compacted.mjs') --in $rollout.FullName --out (Join-Path $h $rel) 2>&1 | ForEach-Object { "  $_" }
Invoke-Probe 'p6-resume' 'H27-PROBE p6-compacted-message-only'

"main home rollout untouched by the probes:"
Show-Markers 'main' $rollout.FullName
"wrote: $out"
