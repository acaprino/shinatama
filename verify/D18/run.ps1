# D18 - system mode: find each agent's installed binary, its owner and its home (Windows part).
# Read-only. Prints paths, versions and names only; never reads file contents of a home.
# Usage: pwsh -File verify\D18\run.ps1 [-Out <file>]
param([string]$Out = (Join-Path $env:LOCALAPPDATA 'shinatama-verify\D18\discovery.txt'))

New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$lines = New-Object System.Collections.Generic.List[string]
function Say($s) { $lines.Add($s); Write-Output $s }

# Children must not inherit an outer agent's markers (technical design 3).
foreach ($n in 'CLAUDECODE','AI_AGENT','CLAUDE_CODE_SESSION_ID','CLAUDE_CODE_CHILD_SESSION','CLAUDE_CODE_SESSION_ATTENDED','CLAUDE_CODE_ENTRYPOINT','CLAUDE_CODE_EXECPATH','CLAUDE_PID','CLAUDE_CODE_MESSAGING_SOCKET','CLAUDE_CODE_MESSAGING_TOKEN','CLAUDE_EFFORT') {
  Remove-Item -LiteralPath "env:$n" -ErrorAction SilentlyContinue
}

$npmPrefix = (& npm prefix -g 2>$null)
Say "date: $(Get-Date -Format s)"
Say "os: $([System.Environment]::OSVersion.VersionString)"
Say "npm global prefix: $npmPrefix"
Say "PATHEXT: $env:PATHEXT"

function Owner([string]$path) {
  $p = $path.ToLowerInvariant()
  if ($npmPrefix -and $p.StartsWith($npmPrefix.ToLowerInvariant())) { return 'npm (global prefix)' }
  if ($p -like '*\.local\bin\claude*') { return 'Claude native installer (~\.local\bin, versions in ~\.local\share\claude\versions)' }
  if ($p -like '*\appdata\local\programs\openai\codex\*') { return 'Codex native installer (%LOCALAPPDATA%\Programs\OpenAI\Codex)' }
  if ($p -like '*\winget\*' -or $p -like '*\windowsapps\*') { return 'winget / Store' }
  if ($p -like '*\scoop\*') { return 'scoop' }
  if ($p -like '*\chocolatey\*') { return 'chocolatey' }
  return 'unknown'
}

$agents = @(
  @{ n='claude';   v='--version'; homes=@("$env:USERPROFILE\.claude", "$env:USERPROFILE\.claude.json") },
  @{ n='codex';    v='--version'; homes=@("$env:USERPROFILE\.codex") },
  @{ n='opencode'; v='--version'; homes=@("$env:USERPROFILE\.local\share\opencode", "$env:USERPROFILE\.config\opencode", "$env:USERPROFILE\.cache\opencode", "$env:USERPROFILE\.local\state\opencode") },
  @{ n='pi';       v='--version'; homes=@("$env:USERPROFILE\.pi") }
)

foreach ($a in $agents) {
  Say ""
  Say "== $($a.n)"
  $cmds = @(Get-Command $a.n -All -ErrorAction SilentlyContinue)
  if (-not $cmds) { Say "  not found on PATH"; continue }
  foreach ($c in $cmds) {
    $src = $c.Source
    Say "  on PATH: $src  [$($c.CommandType)]  owner: $(Owner $src)"
  }
  $first = $cmds[0].Source
  $ver = (& $first $a.v 2>&1 | Select-Object -First 1)
  Say "  version: $ver"
  # npm shims: report the package directory they point to.
  if ((Owner $first) -like 'npm*') {
    $shim = Get-Content -LiteralPath ($first -replace '\.ps1$','.cmd') -ErrorAction SilentlyContinue | Select-String -Pattern 'node_modules\\[^"%]+' -AllMatches
    $targets = $shim.Matches.Value | Sort-Object -Unique
    foreach ($t in $targets) { Say "  npm shim target: $t" }
  }
  foreach ($h in $a.homes) {
    if (Test-Path -LiteralPath $h) {
      $it = Get-Item -LiteralPath $h -Force
      $kind = if ($it.PSIsContainer) { 'dir' } else { 'file' }
      Say "  home candidate present: $h ($kind)"
    } else { Say "  home candidate absent: $h" }
  }
}

Say ""
Say "== winget ownership (names only)"
$wg = Get-Command winget -ErrorAction SilentlyContinue
if ($wg) {
  $list = (& winget list --accept-source-agreements 2>$null) -join "`n"
  foreach ($k in 'Claude','Codex','OpenCode','opencode','Pi Coding') {
    $hit = ($list -split "`n" | Where-Object { $_ -match [regex]::Escape($k) } | Select-Object -First 3)
    Say ("  {0}: {1}" -f $k, ($(if ($hit) { ($hit -join ' | ').Trim() } else { 'no winget entry' })))
  }
} else { Say "  winget not available" }

$lines | Set-Content -LiteralPath $Out -Encoding utf8
Write-Output "written: $Out"
