# D19 / Claude Code - per-instance set of plugins, skills and MCP servers (main 5.14, choice 3).
# Builds synthetic fixtures (no Daodan, no network), a "home" per home kind (box and a fake system
# home, each a scratch CLAUDE_CONFIG_DIR; USERPROFILE/HOME/APPDATA/LOCALAPPDATA/TEMP in a decoy),
# then runs one -p mock turn per switch variant and reads the first model request the mock got.
#   home (U): plugin d19-home-plugin (skill d19-home-skill, agent d19-home-agent, MCP d19-homeplug-mcp)
#             installed+enabled from a local marketplace; <config>\skills\d19-user-skill,
#             <config>\agents\d19-user-agent.md, <config>\commands\d19-user-cmd.md; user MCP d19-home-mcp;
#             local-scope MCP d19-local-mcp (kept in the home's .claude.json under projects[<cwd>])
#   decoy profile: <HOME>\.claude\skills\d19-profile-skill
#   set A: --plugin-dir d19-A (plugin d19-a-plugin, skill d19-a-skill, MCP d19-aplug-mcp) + --mcp-config d19-a-mcp
#   set B: same with b
# Also: home files hashed before/after every run (session/transcript/history/backups excluded);
# concurrency (instance A hanging on H27-HANG while instance B completes, then A resumed with B's set).
# Output: %LOCALAPPDATA%\shinatama-verify\D19\claude_code\run\ (summary.txt, <home>\results\*.json)
# Usage: pwsh -File run.ps1 [-Exe <claude.exe>] [-Homes box,sys] [-SysVariants none,A,B] [-SkipConcurrency]
param(
  [string]$Exe = (Join-Path $env:USERPROFILE '.local\bin\claude.exe'),
  [string[]]$Homes = @('box', 'sys'),
  [string[]]$BoxVariants = @('none', 'none-ss', 'A-naive', 'A', 'B', 'A-ss-empty', 'A-settings-off', 'A-bare', 'A-safe', 'A-noslash', 'A-restricted', 'A-adddir',
    'A-nostrict', 'A-nostrict-noai', 'A-nostrict-empty', 'A-final', 'B-final'),
  [string[]]$SysVariants = @('none', 'A', 'B', 'A-nostrict', 'A-final', 'B-final'),
  [switch]$SkipConcurrency,
  [string]$Tag = 'run'   # output subfolder; a different tag keeps an earlier run's output
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\..\H27\claude_code\env.ps1')
. (Join-Path $PSScriptRoot '..\..\P20\claude_code\bench.ps1')
Initialize-Bench -Row D19 -Port 47101 -TrapPort 47102 -Variant $Tag
# Hard rule: no inherited agent markers in any child (env.ps1 drops CLAUDE*/ANTHROPIC*, bench AI_AGENT).
foreach ($n in @(Get-ChildItem Env: | Where-Object { $_.Name -match '^(CLAUDECODE|CLAUDE_CODE_|CLAUDE_PID|CLAUDE_EFFORT|AI_AGENT|GIT_EDITOR|TRACEPARENT)' } | ForEach-Object Name)) { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
$env:CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL = '1'
Remove-Item Env:CLAUDE_CONFIG_DIR -ErrorAction SilentlyContinue   # set per child only

$free = (Get-PSDrive C).Free
if ($free -lt 2GB) { throw "drive C: has $([math]::Round($free / 1GB, 2)) GB free (< 2 GB): stopping" }

$Run = $H27Scratch
if (Test-Path $Run) { Remove-Item -Recurse -Force $Run }
New-Item -ItemType Directory -Force -Path $Run | Out-Null
$node = (Get-Command node).Source
$mcpJs = Join-Path $PSScriptRoot 'd19-mcp.mjs'
$analyze = Join-Path $PSScriptRoot 'analyze.mjs'
$version = ((& $Exe --version 2>&1 | Out-String).Trim())
$Fix = Join-Path $Run 'fixtures'
$McpLog = Join-Path $Run 'mcp-started'

function Write-Json([string]$Path, $Obj) {
  New-Item -ItemType Directory -Force -Path (Split-Path $Path) | Out-Null
  Write-H27Text $Path ($Obj | ConvertTo-Json -Depth 10)
}
function Write-Skill([string]$Dir, [string]$Name) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  Write-H27Text (Join-Path $Dir 'SKILL.md') "---`nname: $Name`ndescription: D19 marker skill $Name. Use only when the user asks about $Name.`n---`n`nD19 marker skill body for $Name.`n"
}
function Write-AgentMd([string]$Path, [string]$Name) {
  New-Item -ItemType Directory -Force -Path (Split-Path $Path) | Out-Null
  Write-H27Text $Path "---`nname: $Name`ndescription: D19 marker agent $Name. Use only when the user asks about $Name.`n---`n`nYou are the D19 marker agent $Name.`n"
}
function Get-McpEntry([string]$Name) { [ordered]@{ type = 'stdio'; command = $node; args = @($mcpJs, $Name, $McpLog) } }
function New-PluginDir([string]$Dir, [string]$Plugin, [string]$Skill, [string]$PlugMcp, [string]$Agent) {
  Write-Json (Join-Path $Dir '.claude-plugin\plugin.json') ([ordered]@{ name = $Plugin; version = '1.0.0'; description = "D19 marker plugin $Plugin" })
  Write-Skill (Join-Path $Dir "skills\$Skill") $Skill
  if ($Agent) { Write-AgentMd (Join-Path $Dir "agents\$Agent.md") $Agent }
  if ($PlugMcp) { Write-Json (Join-Path $Dir '.mcp.json') @{ mcpServers = @{ $PlugMcp = (Get-McpEntry $PlugMcp) } } }
}

# ---------- fixtures shared by both homes ----------
$market = Join-Path $Fix 'd19-market'
Write-Json (Join-Path $market '.claude-plugin\marketplace.json') ([ordered]@{
    name = 'd19-market'; owner = @{ name = 'D19 bench' }
    plugins = @([ordered]@{ name = 'd19-home-plugin'; source = './plugins/d19-home-plugin'; description = 'D19 home plugin'; version = '1.0.0' })
  })
New-PluginDir (Join-Path $market 'plugins\d19-home-plugin') 'd19-home-plugin' 'd19-home-skill' 'd19-homeplug-mcp' 'd19-home-agent'
$plugA = Join-Path $Fix 'd19-A'; New-PluginDir $plugA 'd19-a-plugin' 'd19-a-skill' 'd19-aplug-mcp' $null
$plugB = Join-Path $Fix 'd19-B'; New-PluginDir $plugB 'd19-b-plugin' 'd19-b-skill' 'd19-bplug-mcp' $null
$mcpA = Join-Path $Fix 'mcp-a.json'; Write-Json $mcpA @{ mcpServers = @{ 'd19-a-mcp' = (Get-McpEntry 'd19-a-mcp') } }
$mcpB = Join-Path $Fix 'mcp-b.json'; Write-Json $mcpB @{ mcpServers = @{ 'd19-b-mcp' = (Get-McpEntry 'd19-b-mcp') } }
# Set A's plugin MCP handed over by --mcp-config under the name Claude gives a plugin server (plugin_<plugin>_<server>),
# so that the tool keeps its plugin name (mcp__plugin_d19-a-plugin_d19-aplug-mcp__...) under --strict-mcp-config.
$mcpAplug = Join-Path $Fix 'mcp-a-with-plugin.json'; Write-Json $mcpAplug @{ mcpServers = [ordered]@{ 'd19-a-mcp' = (Get-McpEntry 'd19-a-mcp'); 'plugin_d19-a-plugin_d19-aplug-mcp' = (Get-McpEntry 'd19-aplug-mcp') } }
$addA = Join-Path $Fix 'addA'; Write-Skill (Join-Path $addA '.claude\skills\d19-adda-skill') 'd19-adda-skill'

$setA = @('--plugin-dir', $plugA, '--mcp-config', $mcpA, '--strict-mcp-config')
$setB = @('--plugin-dir', $plugB, '--mcp-config', $mcpB, '--strict-mcp-config')
$noUser = @('--setting-sources', 'project,local')
$VariantArgs = [ordered]@{
  'none'           = @()
  'none-ss'        = $noUser
  'A-naive'        = @('--plugin-dir', $plugA, '--mcp-config', $mcpA)
  'A'              = $noUser + $setA
  'B'              = $noUser + $setB
  'A-ss-empty'     = @('--setting-sources', '') + $setA
  'A-settings-off' = @('--settings', '{"enabledPlugins":{"d19-home-plugin@d19-market":false}}') + $setA
  'A-bare'         = @('--bare') + $setA
  'A-safe'         = @('--safe-mode') + $setA
  'A-noslash'      = $noUser + @('--disable-slash-commands') + $setA
  'A-restricted'   = @('--restricted') + $setA
  'A-adddir'       = $noUser + @('--add-dir', $addA, '--mcp-config', $mcpA, '--strict-mcp-config')
  # without --strict-mcp-config: does the set's own plugin MCP load, do the home's MCP servers stay out?
  'A-nostrict'       = $noUser + @('--plugin-dir', $plugA, '--mcp-config', $mcpA)
  'A-nostrict-noai'  = $noUser + @('--plugin-dir', $plugA, '--mcp-config', $mcpA)
  'A-nostrict-empty' = @('--setting-sources', '') + @('--plugin-dir', $plugA, '--mcp-config', $mcpA)
  # A-final / B-final are set per home below (they carry a copy of the home's user settings via --settings)
}
# Extra child environment per variant.
$VariantEnv = @{ 'A-nostrict-noai' = @{ ENABLE_CLAUDEAI_MCP_SERVERS = 'false' } }
$Expect = @{
  'none' = 'U'; 'none-ss' = '-'; 'A-naive' = 'U+A'; 'A' = 'A'; 'B' = 'B'; 'A-ss-empty' = 'A'; 'A-settings-off' = 'A'
  'A-bare' = 'A'; 'A-safe' = 'A'; 'A-noslash' = 'A'; 'A-restricted' = 'A'; 'A-adddir' = 'adda+a-mcp'
  'A-nostrict' = 'A'; 'A-nostrict-noai' = 'A'; 'A-nostrict-empty' = 'A'; 'A-final' = 'A'; 'B-final' = 'B'; 'A-final-plugmcp' = 'A+aplug'
}

# Hash of every home file except sessions/transcripts/history/backups.
$Volatile = '^(projects|sessions|backups)\\|^history\.jsonl$'
function Get-HomeHashes([string]$Dir) {
  $h = [ordered]@{}
  $full = (Resolve-Path -LiteralPath $Dir).Path
  Get-ChildItem -LiteralPath $full -Recurse -Force -File | Sort-Object FullName | ForEach-Object {
    $rel = $_.FullName.Substring($full.Length).TrimStart('\')
    if ($rel -notmatch $Volatile) { $h[$rel] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.Substring(0, 16) }
  }
  $h
}
# Top-level keys of a JSON file whose value differs between two texts (names only).
function Get-JsonKeyDiff([string]$Before, [string]$After) {
  try { $a = $Before | ConvertFrom-Json; $b = $After | ConvertFrom-Json } catch { return @('<unparsable>') }
  $keys = @($a.PSObject.Properties.Name) + @($b.PSObject.Properties.Name) | Sort-Object -Unique
  @($keys | Where-Object { ($a.$_ | ConvertTo-Json -Depth 20 -Compress) -ne ($b.$_ | ConvertTo-Json -Depth 20 -Compress) })
}
function Compare-Home($Before, $After, [hashtable]$JsonBefore, [string]$HomeDir) {
  $out = @()
  foreach ($k in @($Before.Keys) + @($After.Keys) | Sort-Object -Unique) {
    if (-not $After.Contains($k)) { $out += "deleted $k" }
    elseif (-not $Before.Contains($k)) { $out += "added $k" }
    elseif ($Before[$k] -ne $After[$k]) {
      $line = "changed $k"
      if ($JsonBefore.ContainsKey($k)) { $line += ' keys=[' + ((Get-JsonKeyDiff $JsonBefore[$k] (Get-Content -Raw (Join-Path $HomeDir $k))) -join ',') + ']' }
      $out += $line
    }
  }
  $out
}
function Get-JsonTexts([string]$HomeDir) {
  $t = @{}
  foreach ($rel in '.claude.json', 'settings.json', 'plugins\installed_plugins.json', 'plugins\known_marketplaces.json') {
    $p = Join-Path $HomeDir $rel; if (Test-Path $p) { $t[$rel] = Get-Content -Raw $p }
  }
  $t
}
function Get-CaptureCount { Get-H27LineCount $script:Capture }
function Save-CaptureSlice([int]$From, [string]$Path) {
  $lines = @(if (Test-Path $script:Capture) { Get-Content $script:Capture })
  Set-Content -Path $Path -Value ($lines | Select-Object -Skip $From) -Encoding utf8
}
function Get-McpStarted { @(if (Test-Path $McpLog) { Get-ChildItem $McpLog -File | ForEach-Object Name }) }

# Starts claude without waiting (for the concurrency test). Same env handling as Invoke-Bench.
function Start-D19([string[]]$ClaudeArgs, [string]$Cwd, [string]$OutBase, [hashtable]$Env) {
  $argLine = ($ClaudeArgs | ForEach-Object { ConvertTo-H27Arg $_ }) -join ' '
  $stdin = Join-Path $script:BenchRoot 'empty-stdin.txt'
  if (-not (Test-Path $stdin)) { New-Item -ItemType File -Force -Path $stdin | Out-Null }
  $saved = @{}
  foreach ($k in $Env.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $Env[$k]) }
  try {
    $p = Start-Process -FilePath $Exe -ArgumentList $argLine -WorkingDirectory $Cwd -NoNewWindow -PassThru -RedirectStandardOutput "$OutBase.stdout.txt" -RedirectStandardError "$OutBase.stderr.txt" -RedirectStandardInput $stdin
  } finally { foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) } }
  $null = $p.Handle
  Set-Content -Path "$OutBase.started.txt" -Value "pid=$($p.Id) args=$argLine" -Encoding utf8
  $p
}

$script:Capture = Join-Path $Run 'capture.jsonl'
$trapLog = Join-Path $Run 'trap.jsonl'
$summary = @("D19 claude_code run $(Get-Date -Format s)  exe=$Exe  version=$version", "mock 127.0.0.1:47101  trap 47102  run=$Run", '')
$common = @('--output-format', 'json', '--dangerously-skip-permissions')
$servers = Start-H27Servers $script:Capture $trapLog
$started = @()   # every claude PID this script started
try {
  foreach ($kind in $Homes) {
    $base = Join-Path $Run $kind
    $homeDir = Join-Path $base 'home'; $work = Join-Path $base 'work'; $decoy = Join-Path $base 'decoy'
    $steps = Join-Path $base 'steps'; $res = Join-Path $base 'results'
    New-DecoyRoot $decoy
    New-Item -ItemType Directory -Force -Path $steps, $res | Out-Null
    $script:H27Home = $homeDir; $script:H27Work = $work
    Initialize-H27Home
    $denv = Get-DecoyEnv $decoy
    $denv['CLAUDE_CONFIG_DIR'] = $homeDir
    # ---- the home's own content (U) ----
    Write-Skill (Join-Path $homeDir 'skills\d19-user-skill') 'd19-user-skill'
    Write-AgentMd (Join-Path $homeDir 'agents\d19-user-agent.md') 'd19-user-agent'
    New-Item -ItemType Directory -Force -Path (Join-Path $homeDir 'commands') | Out-Null
    Write-H27Text (Join-Path $homeDir 'commands\d19-user-cmd.md') "---`ndescription: D19 marker command d19-user-cmd`n---`n`nD19 marker command body.`n"
    Write-Skill (Join-Path $denv.HOME '.claude\skills\d19-profile-skill') 'd19-profile-skill'
    $s1 = Invoke-Bench $Exe (@('mcp', 'add', '-s', 'user', 'd19-home-mcp', '--', $node, $mcpJs, 'd19-home-mcp', $McpLog)) $work "$steps\setup-1-mcp-add" $denv
    $s2 = Invoke-Bench $Exe @('plugin', 'marketplace', 'add', $market) $work "$steps\setup-2-marketplace-add" $denv
    $s3 = Invoke-Bench $Exe @('plugin', 'install', 'd19-home-plugin@d19-market') $work "$steps\setup-3-plugin-install" $denv
    $s4 = Invoke-Bench $Exe @('plugin', 'list', '--json') $work "$steps\setup-4-plugin-list" $denv
    $s5 = Invoke-Bench $Exe (@('mcp', 'add', '-s', 'local', 'd19-local-mcp', '--', $node, $mcpJs, 'd19-local-mcp', $McpLog)) $work "$steps\setup-5-mcp-add-local" $denv
    # The home's user settings minus the plugin keys, handed back with --settings when user settings are left out.
    $us = Get-Content -Raw (Join-Path $homeDir 'settings.json') | ConvertFrom-Json
    $us.PSObject.Properties.Remove('enabledPlugins'); $us.PSObject.Properties.Remove('extraKnownMarketplaces')
    $userCopy = Join-Path $base 'user-settings-for-instance.json'; Write-Json $userCopy $us
    $VariantArgs['A-final'] = $noUser + @('--settings', $userCopy) + $setA
    $VariantArgs['B-final'] = $noUser + @('--settings', $userCopy) + $setB
    $VariantArgs['A-final-plugmcp'] = $noUser + @('--settings', $userCopy, '--plugin-dir', $plugA, '--mcp-config', $mcpAplug, '--strict-mcp-config')
    $summary += "[$kind] home=$homeDir"
    $summary += "  setup: mcp add exit=$($s1.exit); marketplace add exit=$($s2.exit); plugin install exit=$($s3.exit); plugin list exit=$($s4.exit); mcp add -s local exit=$($s5.exit)"
    $summary += "  user settings copy for --settings: $userCopy = $((Get-Content -Raw $userCopy) -replace '\s+', ' ')"
    $set = Get-Content -Raw (Join-Path $homeDir 'settings.json') | ConvertFrom-Json
    $summary += "  settings.json enabledPlugins=$($set.enabledPlugins | ConvertTo-Json -Compress)"
    $cj = Get-Content -Raw (Join-Path $homeDir '.claude.json') | ConvertFrom-Json
    $summary += "  .claude.json user mcpServers=$((@($cj.mcpServers.PSObject.Properties.Name)) -join ','); local-scope (projects[cwd]) mcpServers=$((@($cj.projects.PSObject.Properties | ForEach-Object { if ($_.Value.mcpServers) { $_.Value.mcpServers.PSObject.Properties.Name } })) -join ',')"

    $variants = if ($kind -eq 'box') { $BoxVariants } else { $SysVariants }
    foreach ($v in $variants) {
      $hb = Get-HomeHashes $homeDir; $jb = Get-JsonTexts $homeDir
      $c0 = Get-CaptureCount; $m0 = Get-McpStarted; $t0 = Get-H27LineCount $trapLog
      $venv = $denv.Clone(); if ($VariantEnv.ContainsKey($v)) { foreach ($k in $VariantEnv[$v].Keys) { $venv[$k] = $VariantEnv[$v][$k] } }
      $r = Invoke-Bench $Exe (@('-p', "D19-PROBE $kind/$v") + $common + $VariantArgs[$v]) $work "$steps\$v" $venv
      $started += $r.pid
      Start-Sleep -Milliseconds 500
      $slice = Join-Path $res "$v.capture.jsonl"; Save-CaptureSlice $c0 $slice
      $an = & $node $analyze $slice (Join-Path $res "$v.analysis.json") | Out-String | ConvertFrom-Json
      $ha = Get-HomeHashes $homeDir
      $diff = @(Compare-Home $hb $ha $jb $homeDir)
      $mcpNew = @(Get-McpStarted | Where-Object { $_ -notin $m0 } | ForEach-Object { ($_ -split '-\d+\.started$')[0] } | Sort-Object -Unique)
      $err = (Get-Content -Raw "$steps\$v.stderr.txt" -ErrorAction SilentlyContinue)
      $out = (Get-Content -Raw "$steps\$v.stdout.txt" -ErrorAction SilentlyContinue)
      $reply = try { ($out | ConvertFrom-Json).result } catch { ($out -replace '\s+', ' ').Substring(0, [Math]::Min(160, ($out -replace '\s+', ' ').Length)) }
      $f = $an.first
      $summary += "  [$kind/$v] expect=$($Expect[$v]) exit=$($r.exit) requests=$($an.requests) reply=$reply"
      $summary += "     argv: $($r.args)" + $(if ($VariantEnv.ContainsKey($v)) { '   env: ' + (($VariantEnv[$v].GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' ') } else { '' })
      $summary += "     trap proxy attempts during this run: $((Get-H27LineCount $trapLog) - $t0)"
      $summary += "     first request #$($f.n): markers=[$($f.markers -join ', ')] tools=$($f.toolCount) mcpTools=[$($f.mcpTools -join ', ')] attributionReminder=$($f.attributionReminder)"
      if ($f.otherD19.Count) { $summary += "     other d19 strings: $($f.otherD19 -join ', ')" }
      $summary += "     MCP servers spawned: [$($mcpNew -join ', ')]"
      $summary += "     home files changed: " + $(if ($diff.Count) { $diff -join ' | ' } else { 'none' })
      if ($err -and $err.Trim()) { $summary += "     stderr: " + (($err -replace '\s+', ' ').Trim().Substring(0, [Math]::Min(300, ($err -replace '\s+', ' ').Trim().Length))) }
    }

    # ---- two instances at once, one home, sessions shared ----
    if ($kind -eq 'box' -and -not $SkipConcurrency) {
      $summary += "  [$kind/concurrency]"
      $sidA = [guid]::NewGuid().ToString()
      $hb = Get-HomeHashes $homeDir; $jb = Get-JsonTexts $homeDir
      $c0 = Get-CaptureCount
      $pA = Start-D19 (@('-p', '--session-id', $sidA, 'H27-HANG D19-CONC-A') + $common + $VariantArgs['A']) $work "$steps\conc-A" $denv
      $started += $pA.Id
      $deadline = (Get-Date).AddSeconds(90); $seenA = $false
      while ((Get-Date) -lt $deadline -and -not $seenA) {
        Start-Sleep -Milliseconds 500
        if (Test-Path $script:Capture) { $seenA = [bool](Select-String -Path $script:Capture -Pattern 'D19-CONC-A' -SimpleMatch -Quiet) }
      }
      $summary += "     A pid=$($pA.Id) session=$sidA request at mock: $seenA; A running: $(-not $pA.HasExited)"
      $rB = Invoke-Bench $Exe (@('-p', 'D19-CONC-B') + $common + $VariantArgs['B']) $work "$steps\conc-B" $denv
      $started += $rB.pid
      $aAlive = -not $pA.HasExited
      $summary += "     B exit=$($rB.exit) session=$($rB.session_id); A still running after B finished: $aAlive"
      $proj = Get-ChildItem (Join-Path $homeDir 'projects') -Directory | Select-Object -First 1
      $jsonl = @(Get-ChildItem $proj.FullName -Filter *.jsonl | ForEach-Object BaseName)
      $summary += "     transcripts in $($proj.Name) while A runs: A=$($sidA -in $jsonl) B=$($rB.session_id -in $jsonl) (total $($jsonl.Count))"
      $live = @(Get-ChildItem (Join-Path $homeDir 'sessions') -Filter *.json -ErrorAction SilentlyContinue | ForEach-Object Name)
      $summary += "     sessions\ entries while A runs: $($live -join ', ')"
      Save-CaptureSlice $c0 (Join-Path $res 'conc.capture.jsonl')
      foreach ($tag in 'D19-CONC-A', 'D19-CONC-B') {
        $an = & $node $analyze (Join-Path $res 'conc.capture.jsonl') (Join-Path $res "$tag.analysis.json") $tag | Out-String | ConvertFrom-Json
        $summary += "     $tag first request #$($an.first.n): markers=[$($an.first.markers -join ', ')] mcpTools=[$($an.first.mcpTools -join ', ')]"
      }
      # Stop A: only the PID this script started (its tree).
      & taskkill.exe /T /F /PID $pA.Id 2>&1 | Out-Null; $pA.WaitForExit(10000) | Out-Null
      Set-Content -Path "$steps\conc-A.exit.txt" -Value "killed by recorded pid=$($pA.Id)" -Encoding utf8
      # Resume A's session from B's side (B's set) in the same home.
      $c1 = Get-CaptureCount
      $rR = Invoke-Bench $Exe (@('-p', '--resume', $sidA, 'D19-RESUME-A-FROM-B') + $common + $VariantArgs['B']) $work "$steps\conc-resumeA-withB" $denv
      $started += $rR.pid
      Save-CaptureSlice $c1 (Join-Path $res 'resumeA.capture.jsonl')
      $an = & $node $analyze (Join-Path $res 'resumeA.capture.jsonl') (Join-Path $res 'resumeA.analysis.json') 'D19-RESUME-A-FROM-B' | Out-String | ConvertFrom-Json
      $hasA = [bool](Select-String -Path (Join-Path $res 'resumeA.capture.jsonl') -Pattern 'D19-CONC-A' -SimpleMatch -Quiet)
      $summary += "     resume A with B's set: exit=$($rR.exit) session=$($rR.session_id) request carries A's earlier prompt: $hasA; markers=[$($an.first.markers -join ', ')]"
      $ha = Get-HomeHashes $homeDir
      $diff = @(Compare-Home $hb $ha $jb $homeDir)
      $summary += "     home files changed (whole concurrency step): " + $(if ($diff.Count) { $diff -join ' | ' } else { 'none' })
    }
    Set-Content (Join-Path $base 'home-files-final.txt') (Get-Tree $homeDir) -Encoding utf8
    Set-Content (Join-Path $base 'decoy-files.txt') (Get-Tree $decoy) -Encoding utf8
    $summary += "  decoy files after all runs: $(@(Get-Tree $decoy).Count)  (listed in $base\decoy-files.txt)"
    $summary += ''
  }
} finally {
  Stop-H27Servers $servers
}
$summary += "claude PIDs started: $($started -join ' ')"
$summary += "trap proxy attempts: $(Get-H27LineCount $trapLog)   mock requests: $(Get-CaptureCount)"
$summary | Tee-Object -FilePath (Join-Path $Run 'summary.txt')
