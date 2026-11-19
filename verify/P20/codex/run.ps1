# P20, Codex leg: (1) which environment variables Codex adds for the processes its shell tool starts (names; values
# only for an allowlist of markers), with and without the Windows sandbox; (2) the same mock session started
# clean, with Codex's markers inherited (as if launched from inside a Codex shell) and with Claude Code's markers
# inherited; captures, files, stderr and exit codes are written per variant and compared by compare.mjs.
# Mock: verify\P20\codex\server.local.mjs (H27 mock + "P20-ENV" trigger). Rerunnable:
#   pwsh -NoProfile -File verify\P20\codex\run.ps1
$SvRow = 'P20'; $SvPortBase = 4253
. (Join-Path $PSScriptRoot '..\..\D15\codex\lib.ps1')
$H27Mock = Join-Path $PSScriptRoot 'server.local.mjs'
$removed = Remove-SvAgentMarkers
$outRoot = Join-Path $SvScratch 'out'
Remove-Item -Recurse -Force $outRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $outRoot, $H27Logs, $H27Work | Out-Null
$alpha = Join-Path $H27Work 'alpha.txt'; $beta = Join-Path $H27Work 'beta.txt'
Set-Content $alpha 'ALPHA-CONTENT-42' -Encoding ASCII; Set-Content $beta 'BETA-CONTENT-17' -Encoding ASCII
"codex: $H27Codex ($(& $H27Codex --version)); markers removed from this script's env: $($removed -join ', ')"
$baseEnv = @{}; Get-ChildItem Env: | ForEach-Object { $baseEnv[$_.Name] = $_.Value }

function Invoke-P20Variant([string]$Name, [hashtable]$Inject, [string]$Sandbox = 'none') {
    # One private home per variant; $Inject is put into the environment Codex starts with, then taken out again.
    $out = Join-Path $outRoot $Name; New-Item -ItemType Directory -Force -Path $out | Out-Null
    $decoy = Enter-SvDecoy (Join-Path $SvScratch "decoy-$Name")
    $cfg = Write-H27Config -Force
    if ($Sandbox -ne 'none') {
        (Get-Content $cfg) -replace '^sandbox_mode = .*$', 'sandbox_mode = "workspace-write"' | Set-Content $cfg -Encoding ASCII
        Add-Content $cfg "`n[windows]`nsandbox = `"$Sandbox`"" -Encoding ASCII
    }
    foreach ($k in $Inject.Keys) { Set-Item "Env:$k" $Inject[$k] }
    $savedPath = $env:PATH
    # pwsh here is only the Store (WindowsApps) alias, which the restricted-token sandbox cannot start
    # (CreateProcessAsUserW 5); without it on PATH the shell tool falls back to Windows PowerShell.
    if ($Sandbox -ne 'none') { $env:PATH = ($env:PATH -split ';' | Where-Object { $_ -notmatch 'WindowsApps' }) -join ';' }
    Get-ChildItem Env: | ForEach-Object Name | Sort-Object | Set-Content (Join-Path $out 'parent-env-names.txt') -Encoding UTF8
    $common = @('--json', '--skip-git-repo-check'); if ($Sandbox -eq 'none') { $common += '--dangerously-bypass-approvals-and-sandbox' }
    Start-H27Mock -Capture (Join-Path $out 'capture.jsonl') -EgressLog (Join-Path $out 'egress.jsonl') | Out-Null
    try {
        $r = Invoke-H27Codex -ArgList (@('exec') + $common + "H27-TOOLS read $alpha and $beta") -OutPrefix (Join-Path $out 't1')
        $id = Get-H27ThreadId $r.Stdout; $e1 = $r.Exit
        $r = Invoke-H27Codex -ArgList (@('exec', 'resume') + $common + $id + 'P20-ENV dump the environment names') -OutPrefix (Join-Path $out 't2')
        "{0,-16} T1 exit {1}, T2 exit {2}, thread {3}" -f $Name, $e1, $r.Exit, $id
    } finally { Stop-H27Mock; $env:PATH = $savedPath; foreach ($k in $Inject.Keys) { Remove-Item "Env:$k" -ErrorAction SilentlyContinue } }
    Start-Sleep -Milliseconds 500
    Save-SvLines (Join-Path $out 'home-files.txt') (Get-SvTree $decoy.Private | Where-Object { $_ -notmatch '\\skills\\' })
    Save-SvLines (Join-Path $out 'decoy-outside-private.txt') (Get-SvTree $decoy.Root -Exclude $decoy.Private)
    # The child's env lines come back to the model as the function_call_output of the P20-ENV call.
    $child = foreach ($l in Get-Content (Join-Path $out 'capture.jsonl')) {
        $o = $l | ConvertFrom-Json
        foreach ($it in @($o.body.input)) { if ($it.type -eq 'function_call_output' -and "$($it.output)" -match 'P20ENV ') { "$($it.output)" } }
    }
    $lines = (@($child) | Select-Object -Last 1) -split "`r?`n" | Where-Object { $_ -match '^P20ENV ' } | ForEach-Object { $_.Substring(7) }
    Save-SvLines (Join-Path $out 'child-env.txt') $lines
    $parent = Get-Content (Join-Path $out 'parent-env-names.txt')
    $childNames = $lines | ForEach-Object { ($_ -split '=', 2)[0] }
    Save-SvLines (Join-Path $out 'added-by-codex.txt') ($lines | Where-Object { ($_ -split '=', 2)[0] -notin $parent })
    Save-SvLines (Join-Path $out 'removed-by-codex.txt') ($parent | Where-Object { $_ -notin $childNames })
    return $id
}

# (1) env dump, clean, full access and unelevated sandbox
$idClean = Invoke-P20Variant -Name 'clean' -Inject @{}
Invoke-P20Variant -Name 'clean-sandbox' -Inject @{} -Sandbox 'unelevated' | Out-Null

# (2) Codex markers inherited: every CODEX_*/SBX_* variable the clean runs' shell children saw, with its value.
$codexMarkers = @{}
foreach ($f in 'clean', 'clean-sandbox') {
    foreach ($l in Get-Content (Join-Path $outRoot "$f\added-by-codex.txt")) {
        $n, $v = $l -split '=', 2
        if ($n -match '^(CODEX_|SBX_)' -and $null -ne $v) { $codexMarkers[$n] = $v }
    }
}
"codex markers injected: $(($codexMarkers.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' ')"
Invoke-P20Variant -Name 'codex-markers' -Inject $codexMarkers | Out-Null
Invoke-P20Variant -Name 'claude-markers' -Inject @{ CLAUDECODE = '1'; AI_AGENT = 'claude-code'; CLAUDE_CODE_ENTRYPOINT = 'cli' } | Out-Null

& node (Join-Path $PSScriptRoot 'compare.mjs') --root $outRoot --base clean --variants 'codex-markers,claude-markers' | Tee-Object -FilePath (Join-Path $outRoot 'compare.txt')
"raw output: $outRoot"
