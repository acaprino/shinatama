# D15, Codex leg, Daodan part: install Daodan from the LOCAL checkout into a private CODEX_HOME (decoy-home
# method, egress sink, plugins feature on because `codex plugin` needs it) and check it lands only there.
#   codex plugin marketplace add <checkout>; codex plugin add <plugin>@daodan; codex plugin list; one exec turn.
# Rerunnable:  pwsh -NoProfile -File verify\D15\codex\daodan.ps1 [-Checkout D:\Projects\daodan] [-Plugin project-delivery]
param([string]$Checkout = 'D:\Projects\daodan', [string]$Plugin = 'project-delivery')
$SvRow = 'D15'; $SvPortBase = 4254
. (Join-Path $PSScriptRoot 'lib.ps1')
$out = Join-Path $SvScratch 'out\daodan'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $out, $H27Logs, $H27Work | Out-Null
$removed = Remove-SvAgentMarkers
$decoy = Enter-SvDecoy (Join-Path $SvScratch 'decoy-daodan')
$cfg = Write-H27Config -Force
(Get-Content $cfg) -replace '^plugins = false$', 'plugins = true' | Set-Content $cfg -Encoding ASCII
"codex: $H27Codex ($(& $H27Codex --version)); checkout: $Checkout ($(git -C $Checkout log -1 --format='%h %ci'))"
"CODEX_HOME (private): $env:CODEX_HOME"
$t0 = Get-Date; Start-Sleep -Milliseconds 1100
$egress = Join-Path $out 'egress.jsonl'
Start-H27Mock -Capture (Join-Path $out 'capture.jsonl') -EgressLog $egress | Out-Null
try {
    $steps = @(
        @{ n = 'marketplace-add'; a = @('plugin', 'marketplace', 'add', $Checkout) },
        @{ n = 'plugin-add';      a = @('plugin', 'add', "$Plugin@daodan") },
        @{ n = 'plugin-list';     a = @('plugin', 'list') },
        @{ n = 'exec';            a = @('exec', '--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox', 'D15-DAODAN hello') }
    )
    foreach ($s in $steps) {
        $r = Invoke-H27Codex -ArgList $s.a -OutPrefix (Join-Path $out $s.n) -TimeoutSec 180
        $txt = ((Get-Content $r.Stdout, $r.Stderr -ErrorAction SilentlyContinue) | Where-Object { $_ -and $_ -notmatch '^\{' } | Select-Object -First 6) -join ' | '
        "{0}: exit {1}: {2}" -f $s.n, $r.Exit, ($txt.Substring(0, [Math]::Min(400, $txt.Length)))
    }
} finally { Stop-H27Mock }
Start-Sleep -Milliseconds 500
$inside = Get-SvTree $decoy.Private | Where-Object { $_ -notmatch '\\skills\\\.system\\' }
$outside = Get-SvTree $decoy.Root -Exclude $decoy.Private
Save-SvLines (Join-Path $out 'private-home.txt') $inside
Save-SvLines (Join-Path $out 'decoy-outside-private.txt') $outside
Save-SvLines (Join-Path $out 'real-changed-in-window.txt') (Get-SvRealChanges $t0)
Copy-Item $cfg (Join-Path $out 'config.toml.after')
"private home files (system skills excluded): $(@($inside).Count); under plugins\: $(@($inside | Where-Object { $_ -match '\splugins\\' }).Count)"
"outside private home: $(@($outside).Count) $($outside -join '; ')"
"egress attempts refused: $(@(Get-Content $egress -ErrorAction SilentlyContinue).Count): $((Get-Content $egress -ErrorAction SilentlyContinue | ForEach-Object { ($_ | ConvertFrom-Json).url }) -join ' ')"
"git status of the checkout (must be unchanged): $((git -C $Checkout status --porcelain | Measure-Object).Count) changed path(s)"
"raw output: $out"
