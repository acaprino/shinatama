# D19 / OpenCode helpers. Dot-source me. Builds on verify\D15\opencode\lib.ps1 (environment, command line) but
# never kills by name or port owner: every process is started here, its PID recorded, and only that PID tree is
# stopped (other workers run D19 at the same time).
. (Join-Path $PSScriptRoot '..\..\D15\opencode\lib.ps1')

$D19Fixture = Join-Path $PSScriptRoot 'mcp-server.mjs'
$D19Started = New-Object System.Collections.Generic.List[int]   # every PID this run started

# Base environment for a boxed OpenCode: D15's cleaned environment (agent markers, provider keys, proxies removed),
# plus GIT_EDITOR removed, the box variables, a decoy user profile, a dead proxy, and the H27 dummy key.
function New-D19BoxEnv {
  param([string]$Box, [string]$Decoy, [switch]$AllowClaudeDir)
  $e = Get-ShnBaseEnv
  foreach ($n in @($e.Keys | Where-Object { $_ -match '^(GIT_EDITOR|CLAUDE.*|AI_AGENT)$' })) { $e.Remove($n) | Out-Null }
  $e['USERPROFILE'] = Join-Path $Decoy 'home'; $e['HOME'] = $e['USERPROFILE']
  $e['HOMEDRIVE'] = Split-Path -Qualifier $e['USERPROFILE']; $e['HOMEPATH'] = (Split-Path -NoQualifier $e['USERPROFILE'])
  $e['APPDATA'] = Join-Path $Decoy 'appdata'; $e['LOCALAPPDATA'] = Join-Path $Decoy 'localappdata'
  $e['TEMP'] = Join-Path $Box 'tmp'; $e['TMP'] = $e['TEMP']
  $e['OPENCODE_DB'] = Join-Path $Box 'data\opencode\opencode.db'
  $e['XDG_DATA_HOME'] = Join-Path $Box 'data'; $e['XDG_CONFIG_HOME'] = Join-Path $Box 'config'
  $e['XDG_STATE_HOME'] = Join-Path $Box 'state'; $e['XDG_CACHE_HOME'] = Join-Path $Box 'cache'
  $e['OPENCODE_TEST_HOME'] = Join-Path $Box 'home'
  $e['ANTHROPIC_API_KEY'] = 'h27-dummy'
  $e['OPENCODE_DISABLE_MODELS_FETCH'] = '1'
  $e['OPENCODE_DISABLE_PROJECT_CONFIG'] = '1'
  $e['OPENCODE_DISABLE_AUTOUPDATE'] = '1'
  $e['OPENCODE_PASSWORD'] = $ShnPassword
  if (-not $AllowClaudeDir) { $e['OPENCODE_DISABLE_CLAUDE_CODE'] = '1' }
  foreach ($n in 'HTTP_PROXY', 'HTTPS_PROXY', 'http_proxy', 'https_proxy') { $e[$n] = 'http://127.0.0.1:9' }
  $e['NO_PROXY'] = '127.0.0.1,localhost'; $e['no_proxy'] = $e['NO_PROXY']
  New-Item -ItemType Directory -Force -Path $e['TEMP'], $e['USERPROFILE'], $e['APPDATA'], $e['LOCALAPPDATA'], $e['OPENCODE_TEST_HOME'], (Split-Path $e['OPENCODE_DB']) | Out-Null
  $e
}

function Copy-D19Env($Base, [hashtable]$Set = @{}, [string[]]$Unset = @()) {
  $e = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([StringComparer]::OrdinalIgnoreCase)
  foreach ($k in $Base.Keys) { $e[$k] = $Base[$k] }
  foreach ($k in $Unset) { $e.Remove($k) | Out-Null }
  foreach ($k in $Set.Keys) { $e[$k] = [string]$Set[$k] }
  $e
}

function Write-D19Json([string]$Path, $Object) {
  New-Item -ItemType Directory -Force -Path (Split-Path $Path) | Out-Null
  [IO.File]::WriteAllText($Path, ($Object | ConvertTo-Json -Depth 30), $ShnUtf8)
}

# Provider + model settings that point OpenCode at the mock (same shape as D15's Write-ShnConfig).
function Get-D19Settings([int]$MockPort) {
  [ordered]@{
    model = $ShnModel; update = 'disable'; share = 'disabled'; snapshots = $false
    providers = [ordered]@{ anthropic = [ordered]@{
        settings = [ordered]@{ baseURL = "http://127.0.0.1:$MockPort/v1"; apiKey = 'h27-dummy' }
        models   = [ordered]@{ 'h27-mock' = [ordered]@{ name = 'H27 mock'; capabilities = [ordered]@{ tools = $true; reasoning = [ordered]@{ supported = $true } }; limit = [ordered]@{ context = 200000; output = 8192 } } }
      } }
  }
}

# Native 2.x MCP entry (goes under mcp.servers). codemode=false puts the server's tools in the request's tool list
# (with Code Mode on they hide behind the single `execute` tool).
function Get-D19McpEntry([string]$Name) { [ordered]@{ type = 'local'; command = @('node', $D19Fixture, $Name); codemode = $false } }
function Get-D19Mcp([string]$Name) { [ordered]@{ servers = [ordered]@{ $Name = (Get-D19McpEntry $Name) } } }

# A local plugin (a directory with index.js; a configured `plugins` path must be a directory) that registers one tool
# "<Name>-plugin-tool". $Path is the plugin directory.
function Write-D19Plugin([string]$Path, [string]$Name) {
  $js = @"
// D19 fixture plugin ${Name}: registers one tool so the model request shows the plugin was loaded.
export default {
  id: "$Name",
  async setup(ctx) {
    await ctx.tool.transform((editor) =>
      editor.add({
        name: "$Name-plugin-tool",
        description: "D19 probe tool of plugin $Name",
        input: { type: "object", properties: {}, additionalProperties: false },
        options: { codemode: false },
        execute: async () => ({ output: "$Name ok" }),
      }),
    )
  },
}
"@
  New-Item -ItemType Directory -Force -Path $Path | Out-Null
  [IO.File]::WriteAllText((Join-Path $Path 'index.js'), $js, $ShnUtf8)
}

function Write-D19Skill([string]$SkillsDir, [string]$Name) {
  $d = Join-Path $SkillsDir $Name; New-Item -ItemType Directory -Force -Path $d | Out-Null
  [IO.File]::WriteAllText((Join-Path $d 'SKILL.md'), "---`nname: $Name`ndescription: D19 marker skill $Name`n---`nD19 body of $Name.`n", $ShnUtf8)
}

# Starts $Exe with exactly $Env, writes $Stdin and closes it; returns immediately. The PID is recorded.
function Start-D19Process {
  param([string]$Exe = $ShnExe, $Env, [string[]]$Arguments, [string]$Stdin = '', [string]$WorkDir)
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe; $psi.Arguments = ConvertTo-ShnCommandLine $Arguments; $psi.WorkingDirectory = $WorkDir
  $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
  $psi.StandardOutputEncoding = $ShnUtf8; $psi.StandardErrorEncoding = $ShnUtf8
  $psi.EnvironmentVariables.Clear()
  foreach ($k in $Env.Keys) { $psi.EnvironmentVariables[$k] = $Env[$k] }
  $p = [System.Diagnostics.Process]::Start($psi)
  $D19Started.Add($p.Id)
  $o = $p.StandardOutput.ReadToEndAsync(); $er = $p.StandardError.ReadToEndAsync()
  $b = $ShnUtf8.GetBytes($Stdin); $p.StandardInput.BaseStream.Write($b, 0, $b.Length); $p.StandardInput.Close()
  [pscustomobject]@{ Process = $p; Pid = $p.Id; Out = $o; Err = $er; Started = (Get-Date); CommandLine = "$Exe $($psi.Arguments)"; Env = $Env; WorkDir = $WorkDir; Stdin = $Stdin }
}

# Children of a recorded PID (by ParentProcessId only).
function Get-D19Children([int]$ParentId) { @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $ParentId" -ErrorAction SilentlyContinue) }

function Stop-D19Tree([int]$ProcessId) {
  $kids = Get-D19Children $ProcessId
  if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) { & taskkill.exe /PID $ProcessId /T /F 2>&1 | Out-Null }
  foreach ($k in $kids) { if (Get-Process -Id $k.ProcessId -ErrorAction SilentlyContinue) { & taskkill.exe /PID $k.ProcessId /T /F 2>&1 | Out-Null } }
}

function Wait-D19Process($Run, [int]$TimeoutSec = 180, [string]$OutPrefix, [switch]$Kill) {
  $p = $Run.Process
  $kidsBefore = Get-D19Children $Run.Pid
  $timedOut = $false
  if ($Kill) { $timedOut = $true } else { $timedOut = -not $p.WaitForExit($TimeoutSec * 1000) }
  if ($timedOut) { Stop-D19Tree $Run.Pid; $p.WaitForExit(15000) | Out-Null } else { $p.WaitForExit() }
  # MCP servers etc. that outlived their parent (found by the recorded parent PID only).
  $orphans = @($kidsBefore | Where-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue })
  foreach ($k in $orphans) { & taskkill.exe /PID $k.ProcessId /T /F 2>&1 | Out-Null }
  $null = $Run.Out.Wait(15000); $null = $Run.Err.Wait(15000)
  $r = [pscustomobject]@{ ExitCode = $(if ($p.HasExited) { $p.ExitCode } else { 'running' }); TimedOut = $timedOut; Killed = [bool]$Kill
    Seconds = [math]::Round(((Get-Date) - $Run.Started).TotalSeconds, 1); Stdout = $Run.Out.Result; Stderr = $Run.Err.Result; Pid = $Run.Pid
    OrphansKilled = $orphans.Count }
  if ($OutPrefix) {
    [IO.File]::WriteAllText("$OutPrefix.stdout.txt", "$($r.Stdout)", $ShnUtf8)
    [IO.File]::WriteAllText("$OutPrefix.stderr.txt", "$($r.Stderr)", $ShnUtf8)
    $names = ($Run.Env.Keys | Where-Object { $_ -match '^(OPENCODE_|XDG_|USERPROFILE$|HOME$|TEMP$|TMP$)' } | Sort-Object | ForEach-Object { "$_=$($Run.Env[$_])" }) -join "`n  "
    [IO.File]::WriteAllText("$OutPrefix.exit.txt", "pid=$($Run.Pid)`nexit=$($r.ExitCode)`ntimedOut=$timedOut`nkilled=$Kill`nseconds=$($r.Seconds)`norphansKilled=$($orphans.Count)`ncwd=$($Run.WorkDir)`ncommand=$($Run.CommandLine)`nstdin=$($Run.Stdin)`nenv (box/switch names):`n  $names`n", $ShnUtf8)
  }
  $r
}

# Mock model on a port that must be free (never kills an existing holder).
function Start-D19Mock([int]$Port, [string]$Capture) {
  if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { throw "port $Port already in use; not touching it" }
  $base = $Capture -replace '\.jsonl$', ''
  $p = Start-Process node -ArgumentList @("`"$ShnMock`"", '--port', $Port, '--capture', "`"$Capture`"") -PassThru -WindowStyle Hidden -RedirectStandardOutput "$base.mock-stdout.txt" -RedirectStandardError "$base.mock-stderr.txt"
  $D19Started.Add($p.Id)
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline) {
    $l = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    if ($l -and @($l | Where-Object { $_.OwningProcess -eq $p.Id }).Count) { return $p }
    Start-Sleep -Milliseconds 300
  }
  throw "mock did not listen on $Port"
}

# SHA-256 of every file under the given roots, minus volatile stores (database, logs, cache, session storage).
$D19Volatile = '(?i)(\\opencode\.db(-wal|-shm|-journal)?$|\\log\\|\\logs\\|\\cache\\|\\storage\\|\\snapshot\\|\\tool-output\\|\.lock$|\\locks?\\)'
function Get-D19Hashes([string[]]$Roots) {
  $h = [ordered]@{}
  foreach ($r in $Roots) {
    if (-not (Test-Path $r)) { continue }
    foreach ($f in Get-ChildItem -LiteralPath $r -Recurse -File -Force -ErrorAction SilentlyContinue | Sort-Object FullName) {
      if ($f.FullName -match $D19Volatile) { continue }
      try { $h[$f.FullName] = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256 -ErrorAction Stop).Hash } catch { $h[$f.FullName] = "unreadable" }
    }
  }
  $h
}
function Compare-D19Hashes($Before, $After) {
  $out = @()
  foreach ($k in $After.Keys) { if (-not $Before.Contains($k)) { $out += "added    $k" } elseif ($Before[$k] -ne $After[$k]) { $out += "changed  $k" } }
  foreach ($k in $Before.Keys) { if (-not $After.Contains($k)) { $out += "removed  $k" } }
  $out
}

$D19Markers = @(
  'd19-home-plugin-tool', 'd19-home-skill', 'd19-home-mcp-tool',
  'd19-user-claude-skill', 'd19-user-agents-skill', 'd19-decoy-claude-skill', 'd19-decoy-agents-skill',
  'd19-A-plugin-tool', 'd19-a-skill', 'd19-a-mcp-tool',
  'd19-B-plugin-tool', 'd19-b-skill', 'd19-b-mcp-tool')
function Get-D19Seen([string]$Text) { @($D19Markers | Where-Object { $Text.Contains($_) }) }
