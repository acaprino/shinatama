# D19, Codex leg: shared helpers. Dot-source AFTER setting $D19PortBase (mock port; the egress sink uses +1).
# Reuses verify\D15\codex\lib.ps1 (which reuses the H27 Codex bench: Start-H27Mock, Invoke-H27Codex, egress sink)
# and moves the scratch home to a SHORT path (the daemon socket must stay under ~108 chars):
#   home (CODEX_HOME)   %LOCALAPPDATA%\shv\d19cx\home
#   decoy profile       %LOCALAPPDATA%\shv\d19cx\prof      (USERPROFILE/HOME; holds .agents\skills\d19-agents-skill)
#   fixtures            %LOCALAPPDATA%\shv\d19cx\fx        (local marketplace d19, set folders, MCP stub)
#   raw output          %LOCALAPPDATA%\shinatama-verify\D19\codex\out\...
if (-not $D19PortBase) { $D19PortBase = 47131 }
$SvRow = 'D19'; $SvPortBase = $D19PortBase
. (Join-Path $PSScriptRoot '..\..\D15\codex\lib.ps1')
$H27Sink = $D19PortBase + 1
$H27Mock = Join-Path $SvRepo 'verify\H27\mock\server.mjs'

$D19Root   = Join-Path $SvRealLocal 'shv\d19cx'
$D19Home   = Join-Path $D19Root 'home'
$D19Prof   = Join-Path $D19Root 'prof'
$D19Fx     = Join-Path $D19Root 'fx'
$D19Mkt    = Join-Path $D19Fx 'mkt'
$D19Out    = Join-Path $SvScratch 'out'
$D19Stub   = Join-Path $PSScriptRoot 'mcp-stub.mjs'
$D19Node   = (Get-Command node -CommandType Application | Select-Object -First 1).Source
$H27Home    = $D19Home
$H27Profile = $D19Prof
$H27Work    = Join-Path $D19Root 'work'
$H27Logs    = Join-Path $SvScratch 'logs'

function Enter-D19Env {
    # Child environment: markers out, decoy user dirs, scratch CODEX_HOME, proxies to the egress sink (env.ps1).
    $removed = @(Remove-SvAgentMarkers)
    if ($env:GIT_EDITOR) { Remove-Item Env:GIT_EDITOR; $removed += 'GIT_EDITOR' }
    $env:USERPROFILE = $D19Prof; $env:HOME = $D19Prof
    $env:APPDATA = Join-Path $D19Root 'ad'; $env:LOCALAPPDATA = Join-Path $D19Root 'lad'
    $env:CODEX_HOME = $D19Home
    $env:TEMP = Join-Path $D19Root 'tmp'; $env:TMP = $env:TEMP
    foreach ($d in $D19Home, $D19Prof, $env:APPDATA, $env:LOCALAPPDATA, $env:TEMP, $H27Work, $H27Logs, $D19Out) {
        New-Item -ItemType Directory -Force -Path $d | Out-Null
    }
    return $removed
}

function ConvertTo-D19TomlStr([string]$s) { "'" + $s + "'" }   # TOML literal string: backslashes kept as is

function Get-D19McpArgs([string]$Name) {
    # -c switches that DEFINE an MCP server for one launch (nothing written to the home).
    @('-c', "mcp_servers.$Name.command=$(ConvertTo-D19TomlStr $D19Node)",
      '-c', "mcp_servers.$Name.args=[$(ConvertTo-D19TomlStr $D19Stub), '$Name']")
}

function Get-D19HomeHashes {
    # SHA-256 of every file in the home and the decoy profile, minus session/rollout/SQLite/log state.
    $skip = '\\sessions\\|\\archived_sessions\\|\.sqlite|-wal$|-shm$|\\logs?\\|\\log_|history\.jsonl$|session_index\.jsonl$|\\shell_snapshots\\|\\\.tmp\\|\\tmp\\|\\\.sandbox\\|\\thread-writer-locks\\'
    $h = [ordered]@{}
    foreach ($root in $D19Home, $D19Prof) {
        if (-not (Test-Path $root)) { continue }
        Get-ChildItem $root -Recurse -Force -File -ErrorAction SilentlyContinue | Sort-Object FullName | ForEach-Object {
            $rel = $_.FullName.Substring($D19Root.Length + 1)
            $tag = if ($_.FullName -match $skip) { 'state' } else { 'cfg' }
            $h[$rel] = "$tag $((Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash)"
        }
    }
    return $h
}

function Compare-D19Hashes($Before, $After) {
    # Lines "<kind> <tag> <path>" for added / removed / changed files; tag cfg = counted, state = excluded class.
    $out = New-Object System.Collections.Generic.List[string]
    foreach ($k in $After.Keys) {
        if (-not $Before.Contains($k)) { $out.Add("added   $(($After[$k] -split ' ')[0]) $k") }
        elseif ($Before[$k] -ne $After[$k]) { $out.Add("changed $(($After[$k] -split ' ')[0]) $k") }
    }
    foreach ($k in $Before.Keys) { if (-not $After.Contains($k)) { $out.Add("removed $(($Before[$k] -split ' ')[0]) $k") } }
    return $out
}

function Invoke-D19Inspect([string]$Capture, [string]$Marker, [string]$OutFile) {
    $j = & node (Join-Path $PSScriptRoot 'inspect.mjs') --capture $Capture --marker $Marker --out $OutFile
    return (($j -join "`n") | ConvertFrom-Json)
}
