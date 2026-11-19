# P20 / Claude Code - inherited agent markers. Runs the same mock session twice, each in a
# fresh scratch home: 'clean' (no marker variables) and 'markers' (the variables an outer Claude
# Code hands to its children, with invented values; the messaging socket is a dummy named pipe
# with a logging listener). In both variants every child process Claude starts records the
# NAMES of its environment: hooks (SessionStart, UserPromptSubmit, PreToolUse, Stop), an MCP
# server (--mcp-config), and the Bash tool (mock trigger P20-ENV::<command>).
# Sequence per variant: t1 -p new session / t2 -p --resume P20-ENV (Bash tool) / t3 interactive
# turn in a hidden console (killed after 30 s) / t4 -p --resume plain.
# Output: %LOCALAPPDATA%\shinatama-verify\P20\claude_code\<variant>\ ; compare with compare.mjs.
# Usage: pwsh -File run.ps1 [-Exe <claude.exe>]
param([string]$Exe = (Join-Path $env:USERPROFILE '.local\bin\claude.exe'), [string[]]$Variants = @('clean', 'markers'))
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\..\H27\claude_code\env.ps1')
. (Join-Path $PSScriptRoot 'bench.ps1')
$node = (Get-Command node).Source
$version = ((& $Exe --version 2>&1 | Out-String).Trim() -split ' ')[0]

foreach ($variant in $Variants) {
  Initialize-Bench -Row P20 -Port 47161 -TrapPort 47196 -Variant $variant
  if (Test-Path $H27Scratch) { Remove-Item -Recurse -Force $H27Scratch }
  $steps = Join-Path $H27Scratch 'steps'
  $dump = Join-Path $H27Scratch 'envdump'
  New-Item -ItemType Directory -Force -Path $steps, $dump | Out-Null
  Initialize-H27Home
  $dumpJs = (Join-Path $PSScriptRoot 'dump-env.mjs') -replace '\\', '/'
  $dumpDir = $dump -replace '\\', '/'
  $hook = { param($label) @{ hooks = @(@{ type = 'command'; command = "node `"$dumpJs`" `"$dumpDir`" hook-$label" }) } }
  $settings = [ordered]@{
    autoUpdates = $false; includeCoAuthoredBy = $false
    hooks = [ordered]@{
      SessionStart     = @(& $hook 'SessionStart')
      UserPromptSubmit = @(& $hook 'UserPromptSubmit')
      PreToolUse       = @((& $hook 'PreToolUse') + @{ matcher = '*' })
      Stop             = @(& $hook 'Stop')
    }
  }
  Write-H27Text (Join-Path $H27Home 'settings.json') ($settings | ConvertTo-Json -Depth 8)
  $mcpCfg = Join-Path $H27Scratch 'mcp.json'
  Write-H27Text $mcpCfg (@{ mcpServers = @{ p20env = @{ type = 'stdio'; command = $node; args = @((Join-Path $PSScriptRoot 'mcp-env.mjs'), $dump) } } } | ConvertTo-Json -Depth 6)

  $markers = @{}
  $listener = $null
  if ($variant -eq 'markers') {
    $pipe = "\\.\pipe\LOCAL\p20-dummy-$([guid]::NewGuid().ToString('N').Substring(0, 12))"
    $pipeLog = Join-Path $H27Scratch 'pipe.jsonl'
    $listener = Start-Process -FilePath $node -ArgumentList "`"$(Join-Path $PSScriptRoot 'pipe-listener.mjs')`" `"$pipe`" `"$pipeLog`"" -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $H27Scratch 'pipe.stdout.txt') -RedirectStandardError (Join-Path $H27Scratch 'pipe.stderr.txt')
    Start-Sleep -Milliseconds 800
    # The names an outer Claude Code session hands to its Bash tool / hooks (observed in this
    # agent's own environment); values invented.
    $markers = @{
      CLAUDECODE = '1'; CLAUDE_CODE_CHILD_SESSION = '1'; CLAUDE_CODE_SESSION_ID = '00000000-0000-4000-8000-0000000p2020'.Replace('p', 'a')
      CLAUDE_CODE_SESSION_ATTENDED = '1'; CLAUDE_PID = "$PID"; CLAUDE_CODE_ENTRYPOINT = 'cli'; CLAUDE_CODE_EXECPATH = $Exe
      CLAUDE_CODE_MESSAGING_SOCKET = $pipe; CLAUDE_CODE_MESSAGING_TOKEN = 'p20-invented-token'
      AI_AGENT = "claude-code_$($version -replace '\.', '-')_agent"; CLAUDE_EFFORT = 'high'
    }
    $markers.Keys | Sort-Object | Set-Content (Join-Path $H27Scratch 'markers-set.txt') -Encoding utf8
  }

  # names of the environment claude is launched with (markers included), to tell what Claude adds
  & $node (Join-Path $PSScriptRoot 'dump-env.mjs') (Join-Path $BenchRoot 'launcher-env') $variant | Out-Null
  $capture = Join-Path $H27Scratch 'capture.jsonl'
  $trapLog = Join-Path $H27Scratch 'trap.jsonl'
  Wait-BenchPortsFree; $servers = Start-H27Servers $capture $trapLog
  $common = @('--output-format', 'json', '--dangerously-skip-permissions', '--mcp-config', $mcpCfg)
  $log = @()
  try {
    $r1 = Invoke-Bench $Exe (@('-p', 'P20-T1 plain turn') + $common) $H27Work "$steps\t1" $markers
    $sid = $r1.session_id; $log += "t1 exit=$($r1.exit) sid=$sid"
    $cmd = "P20-ENV::node '$dumpJs' '$dumpDir' bash-tool && compgen -e | LC_ALL=C sort | tr '\n' ' '"
    $r2 = Invoke-Bench $Exe (@('-p', '--resume', $sid, $cmd) + $common) $H27Work "$steps\t2" $markers
    $log += "t2 exit=$($r2.exit) sid=$($r2.session_id)"
    $r3 = Invoke-Bench $Exe @('P20-T3 interactive turn', '--dangerously-skip-permissions', '--mcp-config', $mcpCfg) $H27Work "$steps\t3" $markers -KillAfterSec 30 -Console
    $log += "t3 (interactive, hidden console, killed after 30 s) exit=$($r3.exit)"
    $r4 = Invoke-Bench $Exe (@('-p', '--resume', $sid, 'P20-T4 resumed plain turn') + $common) $H27Work "$steps\t4" $markers
    $log += "t4 exit=$($r4.exit) sid=$($r4.session_id)"
  } finally {
    Stop-H27Servers $servers
    if ($listener -and -not $listener.HasExited) { Stop-Process -Id $listener.Id -Force }
  }
  Set-Content (Join-Path $H27Scratch 'home-files.txt') (Get-Tree $H27Home) -Encoding utf8
  Set-Content (Join-Path $H27Scratch 'work-files.txt') (Get-Tree $H27Work) -Encoding utf8
  $sessions = @(Get-ChildItem -Recurse -Filter *.jsonl -Path (Join-Path $H27Home 'projects') -ErrorAction SilentlyContinue)
  $summary = @(
    "P20 claude_code variant=$variant $(Get-Date -Format s) exe=$Exe version=$version"
    $log
    "transcripts (*.jsonl under projects\): $($sessions.Count) -> " + (($sessions | ForEach-Object { "$($_.Name)=$((Get-Content $_.FullName).Count) lines" }) -join ', ')
    "history.jsonl present: $(Test-Path (Join-Path $H27Home 'history.jsonl'))"
    "envdump files: " + ((Get-ChildItem $dump -File | ForEach-Object { $_.Name -replace '-\d+\.txt$', '' } | Group-Object | ForEach-Object { "$($_.Name)x$($_.Count)" }) -join ' ')
    "pipe listener events: " + $(if (Test-Path (Join-Path $H27Scratch 'pipe.jsonl')) { (Get-Content (Join-Path $H27Scratch 'pipe.jsonl') | ForEach-Object { ($_ | ConvertFrom-Json).event }) -join ',' } else { 'n/a' })
    "trap proxy attempts: $(Get-H27LineCount $trapLog)"
    "mock requests: $(Get-H27LineCount $capture)"
  )
  $summary | Tee-Object -FilePath (Join-Path $H27Scratch 'summary.txt')
}
