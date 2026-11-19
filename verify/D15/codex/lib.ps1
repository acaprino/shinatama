# Shared helpers for the D15 / D16 / P20 Codex rows. Dot-source it AFTER setting $SvRow (e.g. 'D15') and
# $SvPortBase (mock port; the egress sink uses +10). Reuses the H27 Codex bench (verify\H27\codex\env.ps1:
# mock + egress sink + Invoke-H27Codex), then moves every path to this row's own scratch root.
#   . (Join-Path $PSScriptRoot 'lib.ps1')
$ErrorActionPreference = 'Stop'
if (-not $SvRow) { throw 'set $SvRow before dot-sourcing lib.ps1' }
if (-not $SvPortBase) { $SvPortBase = 4250 }

# Real locations, computed once BEFORE anything overrides USERPROFILE / APPDATA / LOCALAPPDATA.
if (-not $env:SV_REAL_USERPROFILE)  { $env:SV_REAL_USERPROFILE  = if ($env:H27_REAL_USERPROFILE) { $env:H27_REAL_USERPROFILE } else { $env:USERPROFILE } }
if (-not $env:SV_REAL_APPDATA)      { $env:SV_REAL_APPDATA      = $env:APPDATA }
if (-not $env:SV_REAL_LOCALAPPDATA) { $env:SV_REAL_LOCALAPPDATA = $env:LOCALAPPDATA }
$env:H27_REAL_USERPROFILE = $env:SV_REAL_USERPROFILE
$SvRealProfile = $env:SV_REAL_USERPROFILE
$SvRealAppData = $env:SV_REAL_APPDATA
$SvRealLocal   = $env:SV_REAL_LOCALAPPDATA
$SvRealTemp    = [IO.Path]::GetTempPath().TrimEnd('\')
$SvRepo        = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path

# The H27 bench: mock, sink, Invoke-H27Codex, ConvertTo-H27ArgLine, Write-H27Config ...
$env:LOCALAPPDATA = $SvRealLocal
. (Join-Path $SvRepo 'verify\H27\codex\env.ps1')

$SvScratch = Join-Path $SvRealLocal "shinatama-verify\$SvRow\codex"
$H27Port    = $SvPortBase
$H27Sink    = $SvPortBase + 10
$H27Scratch = $SvScratch
$H27Logs    = Join-Path $SvScratch 'logs'
$H27Work    = Join-Path $SvScratch 'work'

# Agent markers inherited from the session that runs this script (Claude Code, Codex, OpenCode, the launcher).
# Exact names, enumerated from the current environment; P20 puts some back on purpose.
function Remove-SvAgentMarkers {
    $names = Get-ChildItem Env: | ForEach-Object Name | Where-Object {
        $_ -in @('CLAUDECODE', 'AI_AGENT', 'CLAUDE_PID', 'CLAUDE_EFFORT', 'OPENCODE', 'OPENCODE_CLIENT', 'OPENCODE_SERVER',
                 'CODEX_THREAD_ID', 'CODEX_CI', 'CODEX_SANDBOX', 'CODEX_SANDBOX_NETWORK_DISABLED', 'CODEX_TOOL_CALL_ID',
                 'CODEX_SESSION_ID', 'CODEX_NETWORK_PROXY_ACTIVE', 'CODEX_MANAGED_BY_NPM', 'CODEX_MANAGED_PACKAGE_ROOT') -or
        $_ -like 'CLAUDE_CODE_*' -or $_ -like 'MUSASHI_TERMINAL_*'
    }
    foreach ($n in $names) { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
    return $names
}

# Decoy user dirs: USERPROFILE/HOME, APPDATA and LOCALAPPDATA all point into $Root; the private CODEX_HOME
# lives where Shinatama would keep it (under the decoy LOCALAPPDATA), so any write elsewhere in the decoy is a leak.
function Enter-SvDecoy([string]$Root) {
    Remove-Item -Recurse -Force $Root -ErrorAction SilentlyContinue
    $d = [pscustomobject]@{
        Root = $Root; Home = (Join-Path $Root 'home'); AppData = (Join-Path $Root 'appdata'); Local = (Join-Path $Root 'localappdata')
        Private = (Join-Path $Root 'localappdata\Shinatama\agents\codex\home')
    }
    foreach ($p in $d.Home, $d.AppData, $d.Local, $d.Private) { New-Item -ItemType Directory -Force -Path $p | Out-Null }
    $env:USERPROFILE = $d.Home; $env:HOME = $d.Home; $env:APPDATA = $d.AppData; $env:LOCALAPPDATA = $d.Local
    $env:CODEX_HOME = $d.Private
    $script:H27Home = $d.Private; $script:H27Profile = $d.Home
    return $d
}

function Get-SvTree([string]$Root, [string]$Exclude = '') {
    # Relative path + size of every file under $Root (names and sizes only).
    if (-not (Test-Path $Root)) { return @() }
    Get-ChildItem $Root -Recurse -Force -File -ErrorAction SilentlyContinue |
        Where-Object { -not $Exclude -or $_.FullName -notlike "$Exclude*" } |
        ForEach-Object { '{0,10}  {1}' -f $_.Length, $_.FullName.Substring($Root.Length + 1) }
}

function Get-SvRealChanges([datetime]$Since) {
    # Names only: entries of the REAL user dirs created or modified since $Since. Other processes (the user's own
    # Codex daemon, Claude Code, browsers) also write here, so each hit is attributed by name, not assumed.
    $out = New-Object System.Collections.Generic.List[string]
    $codexReal = Join-Path $SvRealProfile '.codex'
    if (Test-Path $codexReal) {
        Get-ChildItem $codexReal -Recurse -Force -File -ErrorAction SilentlyContinue |
            Where-Object { $_.LastWriteTime -gt $Since -or $_.CreationTime -gt $Since } |
            ForEach-Object { $out.Add("real-.codex  $($_.FullName.Substring($codexReal.Length + 1))") }
    }
    foreach ($pair in @(@('real-profile', $SvRealProfile), @('real-appdata', $SvRealAppData), @('real-localappdata', $SvRealLocal), @('real-temp', $SvRealTemp))) {
        Get-ChildItem $pair[1] -Force -ErrorAction SilentlyContinue |
            Where-Object { ($_.LastWriteTime -gt $Since -or $_.CreationTime -gt $Since) -and $_.Name -ne 'shinatama-verify' } |
            ForEach-Object { $out.Add("$($pair[0])  $($_.Name)") }
    }
    foreach ($base in $SvRealAppData, $SvRealLocal, $SvRealTemp) {
        Get-ChildItem $base -Force -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -match 'codex|openai' } | ForEach-Object {
            Get-ChildItem $_.FullName -Recurse -Force -ErrorAction SilentlyContinue |
                Where-Object { $_.LastWriteTime -gt $Since -or $_.CreationTime -gt $Since } |
                ForEach-Object { $out.Add("codex-named  $($_.FullName)") }
        }
    }
    return $out
}

function Save-SvLines([string]$Path, $Lines) {
    New-Item -ItemType Directory -Force -Path (Split-Path $Path) | Out-Null
    @($Lines) | Set-Content -Path $Path -Encoding UTF8
}
