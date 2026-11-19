# D19 / Pi - per-instance set of extensions/packages, skills and MCP servers. Rerunnable; resets
# its scratch. PowerShell 5.1-compatible.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\D19\pi\run.ps1 [-Mode box|system|both]
# Raw output: %LOCALAPPDATA%\shinatama-verify\D19\pi\<box|system>\runs\<variant>\ and summary.txt
#
# Method. A synthetic "home" (never the real ~\.pi) already holds, under unique d19-* names:
#   - package d19-home-ext (local path in settings.json "packages"): tool d19_home_tool, skill
#     d19-home-skill, prompt template d19-home-pkg-prompt
#   - settings.json "extensions"/"skills" entries: tool d19_settings_tool, skill d19-settings-skill
#   - <agentDir>\extensions\d19-home-auto.ts (discovered): tool d19_home_auto_tool
#   - <agentDir>\skills\d19-user-skill, <agentDir>\prompts\d19-home-prompt.md
#   - HOME\.agents\skills\d19-profile-skill (user-level Agent Skills folder, read from HOME)
#   - <agentDir>\mcp.json: stdio servers d19-home-mcp (exposure direct) and d19-home-cm-mcp (Pi's
#     default exposure, codemode), each with one tool <name>_tool
#   - <agentDir>\AGENTS.md (d19-home-context); the project has AGENTS.md (d19-project-context)
# Sets: A = extension d19-A (tool d19_a_tool) + skill d19-a-skill + MCP d19-a-mcp registered for
# the session by a set extension (pi.registerMcpServer); B likewise. Each variant runs ONE mock
# turn (-p) and inspect.mjs lists the d19-* names of the first model request.
# Homes: box = PI_CODING_AGENT_DIR set; system = PI_CODING_AGENT_DIR unset, the agent dir is
# HOME\.pi\agent of a decoy USERPROFILE/HOME. Every file under the decoy is hashed before and after
# each run. Then (box): two instances at once (A in RPC mode, mid-turn on H27-HANG, while B runs
# to completion) and B resuming A's session.
param([ValidateSet('box', 'system', 'both')][string]$Mode = 'both')
$ErrorActionPreference = 'Stop'
$BenchRow = 'D19'
. (Join-Path $PSScriptRoot '..\..\D15\pi\bench.ps1')
foreach ($n in 'CLAUDE_EFFORT', 'GIT_EDITOR', 'PI_CODING_AGENT_SESSION_DIR', 'PI_PACKAGE_DIR', 'D19_MCP_LOG') { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
Clear-BenchMarkers

$freeGb = (Get-PSDrive C).Free / 1GB
if ($freeGb -lt 2) { throw ("drive C: has {0:N1} GB free (< 2 GB); stopping" -f $freeGb) }

$Base    = Join-Path $BenchRealLocalAppData 'shinatama-verify\D19\pi'
$Fx      = Join-Path $Base 'fixtures'
$Stub    = (Resolve-Path (Join-Path $PSScriptRoot 'mcp-stub.mjs')).Path
$Inspect = (Resolve-Path (Join-Path $PSScriptRoot 'inspect.mjs')).Path
$MockJs  = (Resolve-Path (Join-Path $PSScriptRoot '..\..\P20\pi\server.p20.mjs')).Path
$NodeExe = $H27NodeExe
$Ports   = @{ box = 47191; system = 47192 }
$Provider = 'h27'; $Model = 'h27-mock'
$Common  = @('--provider', $Provider, '--model', $Model, '--offline')
$Log     = Join-Path $Base 'run.log.txt'
Reset-BenchDir $Base

function ConvertTo-D19JsString([string]$s) { ($s | ConvertTo-Json) }
function New-D19Skill([string]$Dir, [string]$Name, [string]$Desc) {
  Write-H27Utf8 (Join-Path $Dir "$Name\SKILL.md") ("---`nname: $Name`ndescription: $Desc`n---`n# $Name`n`nD19 marker skill.`n")
}
function New-D19ToolExt([string]$File, [string]$Tool, [string]$Desc) {
  $js = @"
// D19 fixture extension: registers one marker tool.
export default function (pi: any) {
  pi.registerTool({
    name: "$Tool",
    label: "$Tool",
    description: "$Desc",
    parameters: { type: "object", properties: {}, additionalProperties: false },
    async execute() { return { content: [{ type: "text", text: "$Tool ok" }], details: undefined }; },
  });
}
"@
  Write-H27Utf8 $File $js
}
function Get-D19McpConfig([string]$Server, [string]$FallbackLog, [string]$Exposure = 'direct') {
  $c = @{ command = $NodeExe; args = @($Stub, $Server, $FallbackLog); env = @{ D19_MCP_LOG = '${D19_MCP_LOG}' } }
  if ($Exposure) { $c.exposure = $Exposure }   # '' = Pi's default (codemode)
  $c
}
function New-D19McpExt([string]$File, [string]$Server, [string]$FallbackLog) {
  $cfg = Get-D19McpConfig $Server $FallbackLog | ConvertTo-Json -Compress -Depth 5
  Write-H27Utf8 $File ("// D19 fixture extension: registers one MCP server for this session only.`nexport default function (pi: any) {`n  pi.registerMcpServer(`"$Server`", $cfg);`n}`n")
}
function New-D19Package([string]$Dir, [string]$Name, [string]$Tool, [string]$Skill, [string]$Prompt) {
  $pi = @{ extensions = @('./extensions'); skills = @('./skills') }
  if ($Prompt) { $pi.prompts = @('./prompts') }
  Write-H27Utf8 (Join-Path $Dir 'package.json') (@{ name = $Name; version = '1.0.0'; pi = $pi } | ConvertTo-Json -Depth 5)
  New-D19ToolExt (Join-Path $Dir "extensions\$Name.ts") $Tool "D19 marker tool of package $Name."
  New-D19Skill (Join-Path $Dir 'skills') $Skill "D19 marker skill of package $Name."
  if ($Prompt) { Write-H27Utf8 (Join-Path $Dir "prompts\$Prompt.md") ("---`ndescription: D19 marker prompt $Prompt`n---`nSay $Prompt.`n") }
}

# Every file (sha256) and folder under a root, keyed by relative path.
function Get-D19Hashes([string]$Root) {
  $h = @{}
  if (-not (Test-Path $Root)) { return $h }
  $rootFull = (Resolve-Path $Root).Path.TrimEnd('\')
  Get-ChildItem -LiteralPath $rootFull -Recurse -Force -ErrorAction SilentlyContinue | ForEach-Object {
    $rel = $_.FullName.Substring($rootFull.Length + 1)
    if ($_.PSIsContainer) { $h[$rel + '\'] = 'dir' } else { $h[$rel] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
  }
  $h
}
function Compare-D19Hashes($Before, $After) {
  $out = @()
  foreach ($k in $After.Keys) { if (-not $Before.ContainsKey($k)) { $out += "ADDED $k" } elseif ($Before[$k] -ne $After[$k]) { $out += "CHANGED $k" } }
  foreach ($k in $Before.Keys) { if (-not $After.ContainsKey($k)) { $out += "REMOVED $k" } }
  @($out | Sort-Object)
}
# Sessions, logs and model caches are expected to change; everything else is a home edit.
function Test-D19Expected([string]$Line) {
  $p = $Line -replace '^\w+ ', ''
  ($p -match '(^|\\)sessions(\\|$)') -or ($p -match '\.log$') -or ($p -match '(^|\\)models-store\.json$')
}

# ---------------------------------------------------------------- set fixtures (shared by both homes)
$McpFallback = Join-Path $Base 'mcp-starts-fallback.log'
$Sets = @{}
foreach ($s in 'A', 'B') {
  $l = $s.ToLower()
  $d = Join-Path $Fx "sets\$s"
  New-D19ToolExt (Join-Path $d "d19-$s.ts") "d19_${l}_tool" "D19 marker tool of set $s."
  New-D19Skill (Join-Path $d 'skills') "d19-$l-skill" "D19 marker skill of set $s."
  New-D19McpExt (Join-Path $d "d19-$l-mcp.ts") "d19-$l-mcp" $McpFallback
  New-D19Package (Join-Path $d 'pkg') "d19-$s" "d19_${l}_tool" "d19-$l-skill" $null
  $Sets[$s] = @{
    Items = @('-e', (Join-Path $d "d19-$s.ts"), '--skill', (Join-Path $d "skills\d19-$l-skill"), '-e', 'builtin:mcp', '-e', (Join-Path $d "d19-$l-mcp.ts"))
    Pkg   = (Join-Path $d 'pkg')
    Expect = @{ skills = @("d19-$l-skill"); extTools = @("d19_${l}_tool"); mcpTools = @("mcp__d19_${l}_mcp__d19_${l}_mcp_tool") }
  }
}
$FxHashBefore = Get-D19Hashes $Fx

$NoHome   = @('-ne', '-ns', '-np')                                  # drop every home resource
$XtHome   = @('--exclude-tools', 'mcp__d19_home_mcp__*,mcp__d19_home_cm_mcp__*')   # hide the home's MCP servers
$Variants = @(
  @{ Name = 'home';      Args = @() }
  @{ Name = 'ne';        Args = @('-ne') }
  @{ Name = 'ns';        Args = @('-ns') }
  @{ Name = 'no-mcp';    Args = @('--no-mcp') }
  @{ Name = 'ne-ns';     Args = @('-ne', '-ns') }
  @{ Name = 'empty';     Args = $NoHome;                  Expect = @{ skills = @(); extTools = @(); mcpTools = @() } }
  @{ Name = 'empty-nc';  Args = $NoHome + @('-nc');       Expect = @{ skills = @(); extTools = @(); mcpTools = @() } }
  @{ Name = 'A';         Args = $NoHome + $Sets.A.Items;  Expect = $Sets.A.Expect }
  @{ Name = 'B';         Args = $NoHome + $Sets.B.Items;  Expect = $Sets.B.Expect }
  @{ Name = 'A-xt';      Args = $NoHome + $Sets.A.Items + $XtHome; Expect = $Sets.A.Expect }
  @{ Name = 'B-xt';      Args = $NoHome + $Sets.B.Items + $XtHome; Expect = $Sets.B.Expect }
  @{ Name = 'A-tools';   Args = $NoHome + $Sets.A.Items + @('--tools', 'read,bash,edit,write,d19_a_tool,mcp__d19_a_mcp__*'); Expect = $Sets.A.Expect }
  @{ Name = 'A-pkg';     Args = $NoHome + @('-e', $Sets.A.Pkg); Expect = @{ skills = @('d19-a-skill'); extTools = @('d19_a_tool'); mcpTools = @() } }
)

function Test-D19Exact($Sum, $Expect) {
  if (-not $Expect) { return '' }
  if (-not $Sum.ok) { return 'NO-REQUEST' }
  $ok = ((@($Sum.skills) -join ',') -eq (@($Expect.skills) -join ',')) -and ((@($Sum.extTools) -join ',') -eq (@($Expect.extTools) -join ',')) -and ((@($Sum.mcpTools) -join ',') -eq (@($Expect.mcpTools) -join ','))
  if ($ok) { 'EXACT' } else { 'NOT-EXACT' }
}

function Invoke-D19Inspect([string]$Capture, [string]$Match, [string]$OutJson) {
  $j = & $NodeExe $Inspect $Capture --match $Match --out $OutJson
  ($j | Select-Object -Last 1) | ConvertFrom-Json
}
function Save-D19CaptureSlice([string]$Capture, [int]$Skip, [string]$To) {
  Write-H27Utf8 $To ((@(Get-Content -LiteralPath $Capture -Encoding UTF8 -ErrorAction SilentlyContinue | Select-Object -Skip $Skip)) -join "`n")
}
function Get-D19McpStarts([string]$File) {
  if (-not (Test-Path $File)) { return '' }
  @(Get-Content $File | Where-Object { $_ -match ' start$' } | ForEach-Object { ($_ -split ' ')[1] } | Sort-Object -Unique) -join ','
}

# RPC helpers for the concurrency run (one reader per process; lines appended to $St.Out).
function Send-D19Rpc($St, [string]$Line) {
  $b = (New-Object Text.UTF8Encoding $false).GetBytes($Line + "`n")
  $St.P.StandardInput.BaseStream.Write($b, 0, $b.Length); $St.P.StandardInput.BaseStream.Flush()
  [void]$St.Out.AppendLine(">> $Line")
}
function Wait-D19Rpc($St, [string]$Regex, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ($true) {
    if (-not $St.Pending) { $St.Pending = $St.P.StandardOutput.ReadLineAsync() }
    while (-not $St.Pending.Wait(250)) { if ((Get-Date) -gt $deadline) { return $null } }
    $line = $St.Pending.Result; $St.Pending = $null
    if ($null -eq $line) { return $null }
    [void]$St.Out.AppendLine($line)
    if ($line -match $Regex) { return $line }
  }
}

Write-BenchLog $Log ("pi cli: {0}" -f $H27PiCli)
Write-BenchLog $Log ("node: {0} {1}; C: free {2:N1} GB" -f $NodeExe, (& $NodeExe --version), $freeGb)
Write-BenchLog $Log ("real ~\.pi\agent newest before: {0}" -f (Get-BenchRealPiStamp))
$summary = New-Object System.Collections.Generic.List[string]
$summary.Add("mode`tvariant`texit`tskills`textTools`tmcpTools`tmcpInSystemPrompt`tcontext`tmcpSpawned`texact`thomeEdits`texpectedChanges")

$modes = if ($Mode -eq 'both') { @('box', 'system') } else { @($Mode) }
foreach ($m in $modes) {
  $MB    = Join-Path $Base $m
  $Decoy = Join-Path $MB 'decoy'
  $DHome = Join-Path $Decoy 'home'
  $Agent = if ($m -eq 'box') { Join-Path $Decoy 'shinatama\agents\pi\home' } else { Join-Path $DHome '.pi\agent' }
  $UPkgs = Join-Path $Decoy 'userpkgs'
  $Work  = Join-Path $MB 'work'
  $Tmp   = Join-Path $MB 'tmp'
  $Port  = $Ports[$m]
  foreach ($d in $DHome, (Join-Path $Decoy 'appdata'), (Join-Path $Decoy 'localappdata'), $Agent, $Work, $Tmp) { New-Item -ItemType Directory -Force -Path $d | Out-Null }

  # ---- the home's own resources
  Write-H27Utf8 (Join-Path $Agent 'models.json') (@{ providers = @{ $Provider = @{ baseUrl = "http://127.0.0.1:$Port"; api = 'anthropic-messages'; apiKey = 'h27-dummy'; models = @(@{ id = $Model; name = 'D19 mock'; reasoning = $true; input = @('text'); contextWindow = 200000; maxTokens = 8192; cost = @{ input = 0; output = 0; cacheRead = 0; cacheWrite = 0 } }) } } } | ConvertTo-Json -Depth 10)
  New-D19Package (Join-Path $UPkgs 'd19-home-ext') 'd19-home-ext' 'd19_home_tool' 'd19-home-skill' 'd19-home-pkg-prompt'
  New-D19ToolExt (Join-Path $UPkgs 'd19-settings-ext.ts') 'd19_settings_tool' 'D19 marker tool listed in settings.json extensions.'
  New-D19Skill (Join-Path $UPkgs 'settings-skills') 'd19-settings-skill' 'D19 marker skill listed in settings.json skills.'
  Write-H27Utf8 (Join-Path $Agent 'settings.json') (@{
      defaultProvider = $Provider; defaultModel = $Model; cacheWarming = 'off'; enableAnalytics = $false; enableInstallTelemetry = $false
      packages = @((Join-Path $UPkgs 'd19-home-ext'))
      extensions = @((Join-Path $UPkgs 'd19-settings-ext.ts'))
      skills = @((Join-Path $UPkgs 'settings-skills\d19-settings-skill'))
    } | ConvertTo-Json -Depth 5)
  New-D19ToolExt (Join-Path $Agent 'extensions\d19-home-auto.ts') 'd19_home_auto_tool' 'D19 marker tool discovered in the agent extensions folder.'
  New-D19Skill (Join-Path $Agent 'skills') 'd19-user-skill' 'D19 marker skill in the agent skills folder.'
  Write-H27Utf8 (Join-Path $Agent 'prompts\d19-home-prompt.md') "---`ndescription: D19 marker prompt d19-home-prompt`n---`nSay d19-home-prompt.`n"
  Write-H27Utf8 (Join-Path $Agent 'mcp.json') (@{ mcpServers = @{ 'd19-home-mcp' = (Get-D19McpConfig 'd19-home-mcp' $McpFallback); 'd19-home-cm-mcp' = (Get-D19McpConfig 'd19-home-cm-mcp' $McpFallback '') } } | ConvertTo-Json -Depth 6)
  Write-H27Utf8 (Join-Path $Agent 'AGENTS.md') "# Home instructions`n`nMarker: d19-home-context`n"
  New-D19Skill (Join-Path $DHome '.agents\skills') 'd19-profile-skill' 'D19 marker skill in HOME\.agents\skills.'
  # Project: its own AGENTS.md; a git root so Pi's ancestor .agents\skills walk stops here.
  Write-H27Utf8 (Join-Path $Work 'AGENTS.md') "# Project instructions`n`nMarker: d19-project-context`n"
  New-Item -ItemType Directory -Force -Path (Join-Path $Work '.git') | Out-Null   # Pi only tests that .git exists
  Write-H27Utf8 (Join-Path $MB 'home-files-initial.txt') (Format-BenchTree (Get-BenchTree $Decoy))

  Set-BenchPi -Root $MB -PiHome $Agent -Port $Port -Mock $MockJs
  $Capture = Join-Path $MB 'capture.jsonl'
  $mock = Start-H27Mock $Capture
  Write-BenchLog $Log ("[{0}] mock pid={1} port={2}; agent dir={3}" -f $m, $mock.Id, $Port, $Agent)

  # Decoy environment for every Pi child of this mode.
  $env:USERPROFILE = $DHome; $env:HOME = $DHome; $env:APPDATA = (Join-Path $Decoy 'appdata'); $env:LOCALAPPDATA = (Join-Path $Decoy 'localappdata')
  $env:TEMP = $Tmp; $env:TMP = $Tmp; $env:NODE_COMPILE_CACHE = Join-Path $MB 'node-compile-cache'
  if ($m -eq 'box') { $env:PI_CODING_AGENT_DIR = $Agent } else { Remove-Item Env:PI_CODING_AGENT_DIR -ErrorAction SilentlyContinue }
  try {
    $r = Invoke-H27Pi @('--help') $Work 120; Save-H27Run $r (Join-Path $MB 'pi-help')
    $sessionOf = @{}
    foreach ($v in $Variants) {
      $rdir = Join-Path $MB ("runs\" + $v.Name)
      New-Item -ItemType Directory -Force -Path $rdir | Out-Null
      $env:D19_MCP_LOG = Join-Path $rdir 'mcp-starts.log'
      $prompt = "D19-PROBE $m/$($v.Name) say hello"
      $before = Get-D19Hashes $Decoy
      $capN = @(Get-Content $Capture -ErrorAction SilentlyContinue).Count
      $r = Invoke-H27Pi (@('-p') + $Common + $v.Args + @($prompt)) $Work 300
      Save-H27Run $r (Join-Path $rdir 'pi')
      $diff = Compare-D19Hashes $before (Get-D19Hashes $Decoy)
      Write-H27Utf8 (Join-Path $rdir 'home-diff.txt') ($diff -join "`n")
      Save-D19CaptureSlice $Capture $capN (Join-Path $rdir 'capture.jsonl')
      $sum = Invoke-D19Inspect (Join-Path $rdir 'capture.jsonl') $prompt (Join-Path $rdir 'summary.json')
      $edits = @($diff | Where-Object { -not (Test-D19Expected $_) })
      $expected = @($diff | Where-Object { Test-D19Expected $_ })
      $s = Get-BenchNewestSession $Agent
      $sessionOf[$v.Name] = $s
      $row = "{0}`t{1}`t{2}`t{3}`t{4}`t{5}`t{6}`t{7}`t{8}`t{9}`t{10}`t{11}" -f $m, $v.Name, $r.ExitCode, (@($sum.skills) -join ','), (@($sum.extTools) -join ','), (@($sum.mcpTools) -join ','), (@($sum.systemMcp) -join ','), (@($sum.contextMarkers) -join ','), (Get-D19McpStarts $env:D19_MCP_LOG), (Test-D19Exact $sum $v.Expect), ($edits -join ' | '), $expected.Count
      $summary.Add($row)
      Write-BenchLog $Log ("[{0}] {1}: exit={2} args={3}" -f $m, $v.Name, $r.ExitCode, ($v.Args -join ' '))
      Write-BenchLog $Log ("    " + $row)
      if ($r.Stderr.Trim()) { Write-BenchLog $Log ("    stderr: " + ($r.Stderr.Trim() -replace "`r?`n", ' / ')) }
    }

    # ---- sessions shared: instance B resumes the session instance A-xt wrote, same home
    $rdir = Join-Path $MB 'runs\B-resumes-A'
    New-Item -ItemType Directory -Force -Path $rdir | Out-Null
    $env:D19_MCP_LOG = Join-Path $rdir 'mcp-starts.log'
    $aSess = $sessionOf['A-xt']
    $prompt = "D19-RESUME $m B continues A"
    $before = Get-D19Hashes $Decoy
    $capN = @(Get-Content $Capture).Count
    $r = Invoke-H27Pi (@('-p') + $Common + $NoHome + $Sets.B.Items + $XtHome + @('--session', $aSess.Id, $prompt)) $Work 300
    Save-H27Run $r (Join-Path $rdir 'pi')
    $diff = Compare-D19Hashes $before (Get-D19Hashes $Decoy)
    Write-H27Utf8 (Join-Path $rdir 'home-diff.txt') ($diff -join "`n")
    Save-D19CaptureSlice $Capture $capN (Join-Path $rdir 'capture.jsonl')
    $sum = Invoke-D19Inspect (Join-Path $rdir 'capture.jsonl') $prompt (Join-Path $rdir 'summary.json')
    $after = Get-BenchNewestSession $Agent
    $edits = @($diff | Where-Object { -not (Test-D19Expected $_) })
    $summary.Add(("{0}`tB-resumes-A`t{1}`t{2}`t{3}`t{4}`t{5}`t{6}`t{7}`t{8}`t{9}`t{10}" -f $m, $r.ExitCode, (@($sum.skills) -join ','), (@($sum.extTools) -join ','), (@($sum.mcpTools) -join ','), (@($sum.systemMcp) -join ','), (@($sum.contextMarkers) -join ','), (Get-D19McpStarts $env:D19_MCP_LOG), (Test-D19Exact $sum $Sets.B.Expect), ($edits -join ' | '), ($diff.Count - $edits.Count)))
    Write-BenchLog $Log ("[{0}] B-resumes-A: exit={1} A session={2} newest after={3} same file={4}; prior user turns in request: {5}" -f $m, $r.ExitCode, $aSess.Id, $after.Id, ($after.File -eq $aSess.File), (@($sum.priorUserTurns) -join ' || '))

    # ---- two instances at once (box only): A in RPC mode, mid-turn on H27-HANG, while B runs
    if ($m -eq 'box') {
      $cdir = Join-Path $MB 'runs\concurrency'
      New-Item -ItemType Directory -Force -Path $cdir | Out-Null
      $env:D19_MCP_LOG = Join-Path $cdir 'mcp-starts.log'
      $before = Get-D19Hashes $Decoy
      $capN = @(Get-Content $Capture).Count
      $sessBefore = @(Get-ChildItem (Join-Path $Agent 'sessions') -Recurse -File -Filter '*.jsonl' | ForEach-Object { $_.FullName })
      $pa = New-H27PiProcess (@('--mode', 'rpc') + $Common + $NoHome + $Sets.A.Items + $XtHome) $Work -Stdin
      $aPid = $pa.Id
      $errA = $pa.StandardError.ReadToEndAsync()
      $st = @{ P = $pa; Pending = $null; Out = (New-Object Text.StringBuilder) }
      $notes = New-Object System.Collections.Generic.List[string]
      $notes.Add("instance A pid=$aPid (rpc)")
      try {
        Send-D19Rpc $st '{"id":"a1","type":"prompt","message":"D19-CONC-A1 first turn of instance A"}'
        $notes.Add("A1 agent_end: " + [bool](Wait-D19Rpc $st '"type":"agent_end"' 120))
        Send-D19Rpc $st '{"id":"a2","type":"prompt","message":"H27-HANG D19-CONC-A2 instance A turn left in flight"}'
        $notes.Add("A2 streaming (H27-PARTIAL seen): " + [bool](Wait-D19Rpc $st 'H27-PARTIAL' 120))
        $notes.Add("A alive before B: " + (-not $pa.HasExited))
        $rb = Invoke-H27Pi (@('-p') + $Common + $NoHome + $Sets.B.Items + $XtHome + @('D19-CONC-B instance B while A is mid-turn')) $Work 300
        Save-H27Run $rb (Join-Path $cdir 'instanceB')
        $notes.Add("B exit=" + $rb.ExitCode + " stdout=" + $rb.Stdout.Trim() + " seconds=" + $rb.Seconds)
        $notes.Add("A alive after B: " + (-not $pa.HasExited))
        Send-D19Rpc $st '{"id":"x1","type":"abort"}'
        $notes.Add("A abort response: " + [bool](Wait-D19Rpc $st '"command":"abort"' 60))
        Send-D19Rpc $st '{"id":"a3","type":"prompt","message":"D19-CONC-A3 instance A after B finished"}'
        $notes.Add("A3 agent_end: " + [bool](Wait-D19Rpc $st '"type":"agent_end"' 120))
        Send-D19Rpc $st '{"id":"s1","type":"get_state"}'
        $state = Wait-D19Rpc $st '"id":"s1"' 30
        $notes.Add("A get_state: " + $state)
      }
      finally {
        try { $pa.StandardInput.Close() } catch { }
        if (-not $pa.WaitForExit(60000)) { Stop-H27Tree $aPid; $notes.Add("A killed by recorded pid $aPid") }
        if ($st.Pending) { [void]$st.Pending.Wait(5000) }
        [void]$errA.Wait(10000)
        Write-H27Utf8 (Join-Path $cdir 'instanceA.rpc.txt') ($st.Out.ToString() + $pa.StandardOutput.ReadToEnd())
        Write-H27Utf8 (Join-Path $cdir 'instanceA.stderr.txt') ([string]$errA.Result)
      }
      $diff = Compare-D19Hashes $before (Get-D19Hashes $Decoy)
      Write-H27Utf8 (Join-Path $cdir 'home-diff.txt') ($diff -join "`n")
      Save-D19CaptureSlice $Capture $capN (Join-Path $cdir 'capture.jsonl')
      foreach ($tag in 'D19-CONC-A1', 'D19-CONC-B', 'D19-CONC-A3') {
        $cs = Invoke-D19Inspect (Join-Path $cdir 'capture.jsonl') $tag (Join-Path $cdir "summary-$tag.json")
        $notes.Add(("{0}: skills={1} extTools={2} mcpTools={3}" -f $tag, (@($cs.skills) -join ','), (@($cs.extTools) -join ','), (@($cs.mcpTools) -join ',')))
      }
      $new = @(Get-ChildItem (Join-Path $Agent 'sessions') -Recurse -File -Filter '*.jsonl' | Where-Object { $sessBefore -notcontains $_.FullName })
      foreach ($f in $new) {
        $tags = @(Select-String -Path $f.FullName -Pattern 'D19-CONC-[A-Z0-9]+' -AllMatches | ForEach-Object { $_.Matches } | ForEach-Object { $_.Value } | Sort-Object -Unique) -join ','
        $notes.Add(("new session file {0}: {1}" -f $f.FullName.Substring($Agent.Length + 1), $tags))
      }
      $edits = @($diff | Where-Object { -not (Test-D19Expected $_) })
      $notes.Add("home edits during concurrency: " + ($edits -join ' | ') + "; expected changes: " + ($diff.Count - $edits.Count))
      Write-H27Utf8 (Join-Path $cdir 'notes.txt') ($notes -join "`n")
      foreach ($n in $notes) { Write-BenchLog $Log ("[box] concurrency: " + $n) }
    }
  }
  finally {
    $env:USERPROFILE = $BenchRealUserProfile; $env:HOME = $BenchRealHome; $env:APPDATA = $BenchRealAppData; $env:LOCALAPPDATA = $BenchRealLocalAppData
    $env:TEMP = $BenchRealTemp; $env:TMP = $BenchRealTemp
    Remove-Item Env:NODE_COMPILE_CACHE, Env:D19_MCP_LOG, Env:PI_CODING_AGENT_DIR -ErrorAction SilentlyContinue
    Stop-H27Mock $mock
  }
  Write-H27Utf8 (Join-Path $MB 'home-files-final.txt') (Format-BenchTree (Get-BenchTree $Decoy))
  $hosts = @(Get-Content $Capture | ForEach-Object { ($_ | ConvertFrom-Json).headers.host } | Sort-Object -Unique)
  Write-BenchLog $Log ("[{0}] mock requests: {1}; hosts: {2}" -f $m, @(Get-Content $Capture).Count, ($hosts -join ','))
}

$fxDiff = Compare-D19Hashes $FxHashBefore (Get-D19Hashes $Fx)
Write-BenchLog $Log ("set fixtures changed: {0}" -f (@($fxDiff) -join ' | '))
Write-BenchLog $Log ("real ~\.pi\agent newest after: {0}" -f (Get-BenchRealPiStamp))
Write-H27Utf8 (Join-Path $Base 'summary.txt') ($summary -join "`n")
Write-Host ""
$summary | ForEach-Object { Write-Host $_ }
Write-BenchLog $Log ("raw output: {0}" -f $Base)
