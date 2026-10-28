# P20 / OpenCode: the agent markers OpenCode sets for its children, and what OpenCode does when it inherits markers.
#   pwsh -NoProfile -File verify\P20\opencode\run.ps1
# 1. Marker names in the binary (strings grep). 2. Same mock session (SHN-SHELL env dump through OpenCode's shell tool)
# under: clean (markers removed), opencode-markers inherited, claude-markers inherited. The Claude runs leave
# OPENCODE_DISABLE_CLAUDE_CODE unset and put a probe skill in <OPENCODE_TEST_HOME>\.claude\skills, so a change in
# ~/.claude skill loading would show in the requests. Raw output: %LOCALAPPDATA%\shinatama-verify\P20\opencode\
param([int]$MockPort = 4137, [string[]]$Only)
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot '..\..\D15\opencode\lib.ps1')
$Root = Join-Path $env:LOCALAPPDATA 'shinatama-verify\P20\opencode'
if (-not $Only) { Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue }
$work = Join-Path $Root 'work'; New-Item -ItemType Directory -Force -Path $work | Out-Null
$report = New-Object System.Collections.Generic.List[string]

# 1. Strings in the binary.
# Git's grep.exe: reading the 247 MB binary into a PowerShell string is far too slow.
$grep = Join-Path (Split-Path (Split-Path (Get-Command git).Source)) 'usr\bin\grep.exe'
$pat = '\b(OPENCODE(_[A-Z0-9_]+)?|AI_AGENT|CLAUDECODE|CLAUDE_CODE_[A-Z_]+|CODEX_[A-Z_]+|PI_CODING_AGENT[A-Z_]*|GEMINI_CLI|CURSOR_AGENT|COPILOT_CLI|JCODE_[A-Z_]+)\b'
$found = @(& $grep -a -o -E $pat $ShnExe | Sort-Object -Unique)
$report.Add("== binary $ShnExe ($((Get-Item $ShnExe).Length) bytes): $($found.Count) distinct marker-like names")
$report.Add("   names: " + ($found -join ','))
$ctx = @(& $grep -a -o -E '.{0,70}(env\.AGENT="1"|AI_AGENT\|\|=|OPENCODE_TERMINAL:"1"|CLAUDE_CODE_SSE_PORT\|\||CLAUDECODECLAUDE_CODE).{0,90}' $ShnExe | ForEach-Object { $_ -replace '[^\x20-\x7e]', '.' } | Select-Object -Unique -First 8)
$report.Add("   sites in the binary:"); $ctx | ForEach-Object { $report.Add("     $_") }
[IO.File]::WriteAllLines((Join-Path $Root 'binary-names.txt'), [string[]]$found, $ShnUtf8)

# 2. Mock sessions.
$dump = ConvertTo-ShnSlash $ShnDump
$cases = [ordered]@{
  'clean'           = @{}
  'opencode-inherit' = @{ OPENCODE = '1'; OPENCODE_TERMINAL = '1'; OPENCODE_SESSION_ID = 'ses_parentP20xxxxxxxxxxxxxxx'; AGENT = '1'; AI_AGENT = 'opencode' }
  'claude-clean'    = @{}
  'claude-inherit'  = @{ CLAUDECODE = '1'; AI_AGENT = 'claude-code'; CLAUDE_CODE_ENTRYPOINT = 'cli'; CLAUDE_CODE_CHILD_SESSION = '1' }
}
$summary = @{}
foreach ($name in $cases.Keys) {
  $dir = Join-Path $Root $name
  if ($Only -and $name -notin $Only) { if (Test-Path (Join-Path $dir 'dump.json')) { $summary[$name] = Get-Content (Join-Path $dir 'dump.json') -Raw | ConvertFrom-Json }; continue }
  Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $claudeCase = $name -like 'claude-*'
  $x = New-ShnOpenCodeEnv -Set full -Private (Join-Path $dir 'private') -MockPort $MockPort -Add $cases[$name] -AllowClaudeDir:$claudeCase
  if ($claudeCase) {
    $sk = Join-Path $x.Env['OPENCODE_TEST_HOME'] '.claude\skills\shn-probe'; New-Item -ItemType Directory -Force -Path $sk | Out-Null
    [IO.File]::WriteAllText((Join-Path $sk 'SKILL.md'), "---`nname: shn-probe`ndescription: SHN-PROBE-SKILL marker for P20`n---`nbody`n", $ShnUtf8)
  }
  $cap = Join-Path $dir 'capture.jsonl'
  $mock = Start-ShnMock $MockPort $cap
  try {
    $res = Invoke-ShnProcess -Env $x.Env -Arguments @('run', '--standalone', '--model', $ShnModel, '--format', 'json', '--auto', '--title', "P20-$name") `
      -Stdin "SHN-SHELL node $dump $(ConvertTo-ShnSlash (Join-Path $dir 'dump.json'))" -WorkDir $work -OutPrefix (Join-Path $dir 'run')
  } finally { Stop-ShnProcess $mock }
  $reqs = @(Get-Content $cap -ErrorAction SilentlyContinue | ForEach-Object { $_ | ConvertFrom-Json })
  $first = $reqs | Select-Object -First 1
  $sys = if ($first) { ($first.body.system | ForEach-Object { $_.text }) -join "`n" } else { '' }
  $sha = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes(($sys -replace 'ses_[A-Za-z0-9]+', 'SES' -replace '\d{4}-\d{2}-\d{2}', 'DATE' -replace [regex]::Escape($Root), 'ROOT' -replace [regex]::Escape($name), 'CASE')))).Replace('-', '').Substring(0, 12)
  $all = Get-Content $cap -Raw -ErrorAction SilentlyContinue
  $d = if (Test-Path (Join-Path $dir 'dump.json')) { Get-Content (Join-Path $dir 'dump.json') -Raw | ConvertFrom-Json }
  $privTree = @(Get-ShnTree (Join-Path $dir 'private'))
  $summary[$name] = $d
  $report.Add("== $name exit=$($res.ExitCode) requests=$($reqs.Count) systemSha=$sha tools=$(($first.body.tools | ForEach-Object { $_.name }) -join ',')")
  $report.Add("   probe skill in requests: $($all -match 'SHN-PROBE-SKILL')   'claude' mentions in first request: $(([regex]::Matches(($first | ConvertTo-Json -Depth 50 -Compress), '(?i)claude')).Count)")
  $report.Add("   stderr lines: $(@($res.Stderr -split "`n" | Where-Object { $_.Trim() }).Count)   private files: $($privTree.Count)")
  if ($d) {
    $report.Add("   child markers: " + (($d.values.PSObject.Properties | Where-Object { $_.Name -match '^(OPENCODE$|OPENCODE_TERMINAL|OPENCODE_SESSION_ID|OPENCODE_CLIENT|AGENT|AI_AGENT|CLAUDECODE|CLAUDE_CODE_ENTRYPOINT|TERM)$' } | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join '; '))
    $report.Add("   child names with CLAUDE/OPENCODE/AGENT: " + ((@($d.names) | Where-Object { $_ -match 'CLAUDE|OPENCODE|AGENT' }) -join ','))
  }
}
$c = $summary['clean']
foreach ($n in 'opencode-inherit', 'claude-inherit') {
  $d = $summary[$n]; if (-not ($c -and $d)) { continue }
  $report.Add("== child names added in $n vs clean: " + ((@($d.names) | Where-Object { $_ -notin @($c.names) }) -join ','))
}
[IO.File]::WriteAllLines((Join-Path $Root $(if ($Only) { "report-only-$($Only -join '-').txt" } else { 'report.txt' })), [string[]]$report, $ShnUtf8)
$report
"report: $(Join-Path $Root 'report.txt')"
