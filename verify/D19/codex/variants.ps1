# D19, Codex leg: one mock turn per switch combination in the SAME synthetic home (setup.ps1 first). For each
# variant: hash the home + decoy profile, start the mock (fresh capture) and the egress sink, run
#   codex exec --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox <switches> "D19-<variant> hello"
# stop them, hash again, then list the d19-* names in the FIRST model request (inspect.mjs).
# Rerunnable:  pwsh -NoProfile -File verify\D19\codex\variants.ps1 [-Only none,a-final,...]
param([string[]]$Only = @())
$D19PortBase = 47133
. (Join-Path $PSScriptRoot 'lib.ps1')
if (-not (Test-Path (Join-Path $D19Home 'config.toml'))) { throw "run setup.ps1 first ($D19Home has no config.toml)" }
$removed = Enter-D19Env
$H27Port = $D19PortBase; $H27Sink = $D19PortBase + 1
$cfg = Join-Path $D19Home 'config.toml'

$q = { param($s) ConvertTo-D19TomlStr $s }
$userSkill   = Join-Path $D19Home 'skills\d19-user-skill\SKILL.md'
$agentsSkill = Join-Path $D19Prof '.agents\skills\d19-agents-skill\SKILL.md'
$looseA      = Join-Path $D19Fx 'loose\d19-a-loose\SKILL.md'
# The mock lives on this script's own port: point the home's provider there for the launch (the home file says 47131).
$provider = @('-c', "model_providers.h27.base_url=`"http://127.0.0.1:$H27Port/v1`"")
# Exclusion of everything the home itself carries (built by Shinatama from a read-only look at the home).
$excludeHome = @('-c', 'plugins.d19-home-plugin@d19.enabled=false',
                 '-c', 'mcp_servers.d19-home-mcp.enabled=false',
                 '-c', "skills.config=[{path=$(& $q $userSkill),enabled=false},{path=$(& $q $agentsSkill),enabled=false}]")

$variants = [ordered]@{
    'none'                 = @()
    # --- can a plugin that is NOT installed in the home be loaded for one launch? ---
    'p1-enable-uninst'     = @('-c', 'plugins.d19-a@d19.enabled=true')
    'p1q-enable-uninst'    = @('-c', 'plugins."d19-a@d19".enabled=true')
    'p2-set-marketplace'   = @('-c', 'marketplaces.d19a.source_type="local"', '-c', "marketplaces.d19a.source=$(& $q (Join-Path $D19Fx 'mkta'))",
                               '-c', 'plugins.d19-a@d19a.enabled=true')
    'p3-skills-cfg-path'   = @('-c', "skills.config=[{path=$(& $q $looseA),enabled=true}]")
    # --- per-launch exclusion of the home's own items ---
    'x-disable-home'       = $excludeHome
    'x-empty-tables'       = @('-c', 'plugins={}', '-c', 'mcp_servers={}', '-c', 'skills.config=[]')
    'x-features-off'       = @('--disable', 'plugins')
    'x-skills-instr-off'   = @('-c', 'skills.include_instructions=false')
    'x-bundled-off'        = @('-c', 'skills.bundled.enabled=false')
    # --- MCP defined for one launch only ---
    'm-a-mcp'              = (Get-D19McpArgs 'd19-a-mcp')
}

$sum = New-Object System.Collections.Generic.List[string]
"codex $(& $H27Codex --version); CODEX_HOME $D19Home; markers removed: $($removed -join ', ')"
foreach ($name in $variants.Keys) {
    if ($Only.Count -and $name -notin $Only) { continue }
    $out = Join-Path $D19Out "v\$name"; Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $out | Out-Null
    $marker = "D19-$name"
    $before = Get-D19HomeHashes
    $cap = Join-Path $out 'capture.jsonl'; $egr = Join-Path $out 'egress.jsonl'
    Start-H27Mock -Capture $cap -EgressLog $egr | Out-Null
    try {
        $argv = @('exec', '--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox') + $provider + $variants[$name] + "$marker hello"
        $r = Invoke-H27Codex -ArgList $argv -OutPrefix (Join-Path $out 'exec') -Cwd $H27Work -TimeoutSec 120
    } finally { Stop-H27Mock }
    Start-Sleep -Milliseconds 300
    $diff = @(Compare-D19Hashes $before (Get-D19HomeHashes))
    Save-SvLines (Join-Path $out 'home-diff.txt') $diff
    $i = Invoke-D19Inspect $cap $marker (Join-Path $out 'first-request.json')
    $err = ((Get-Content $r.Stderr -ErrorAction SilentlyContinue) | Where-Object { $_ -match 'error|warn|invalid|unknown' } | Select-Object -First 3) -join ' | '
    $cfgDiff = @($diff | Where-Object { $_ -match ' cfg ' })
    $line = '{0,-20} exit {1,-3} d19: [{2}] mcp-tools: [{3}] home cfg changes: {4} {5}' -f $name, $r.Exit, ($i.d19_all -join ' '),
        ((@($i.tool_names) | Where-Object { $_ -match 'd19' }) -join ' '), $cfgDiff.Count, ($cfgDiff -join '; ')
    $sum.Add($line); $line
    if ($err) { "    stderr: $($err.Substring(0, [Math]::Min(400, $err.Length)))" }
    if (-not $i.found) { "    no request with marker; requests captured: $($i.requests)" }
}
Save-SvLines (Join-Path $D19Out "variants-summary$(if ($Only.Count) { '-' + ($Only -join '+') }).txt") $sum
"raw output: $D19Out\v"
