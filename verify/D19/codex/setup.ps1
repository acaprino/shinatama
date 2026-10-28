# D19, Codex leg: builds the synthetic "home" (scratch CODEX_HOME, never the real ~\.codex) and the fixtures.
#   home: plugin d19-home-plugin (skill d19-home-skill) installed + enabled from the local marketplace "d19";
#         user skill <CODEX_HOME>\skills\d19-user-skill; decoy ~\.agents\skills\d19-agents-skill;
#         MCP server d19-home-mcp in config.toml; the mock as model provider; plugins feature on.
#   sets: plugin d19-a (skill d19-a-skill) / d19-b (skill d19-b-skill) in the same marketplace, NOT installed;
#         one-plugin marketplaces fx\mkta ("d19a") and fx\mktb ("d19b"); loose skills fx\loose\{d19-a-loose,d19-b-loose}.
# Rerunnable (wipes only %LOCALAPPDATA%\shv\d19cx):  pwsh -NoProfile -File verify\D19\codex\setup.ps1
$D19PortBase = 47131
. (Join-Path $PSScriptRoot 'lib.ps1')
Remove-Item -Recurse -Force $D19Root -ErrorAction SilentlyContinue
$removed = Enter-D19Env
"codex: $H27Codex ($(& $H27Codex --version)); markers removed: $($removed -join ', ')"
"CODEX_HOME: $D19Home ($($D19Home.Length) chars; daemon socket would be $((Join-Path $D19Home 'app-server-control\app-server-control.sock').Length) chars)"

function New-D19Skill([string]$Dir, [string]$Name, [string]$What) {
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    @"
---
name: $Name
description: D19 marker skill $Name ($What). Use only when asked for $Name.
---

# $Name

Marker skill for the D19 verification; it does nothing.
"@ | Set-Content -Path (Join-Path $Dir 'SKILL.md') -Encoding UTF8
}
function New-D19Plugin([string]$MktRoot, [string]$Plugin, [string]$Skill) {
    $p = Join-Path $MktRoot "plugins\$Plugin"
    New-Item -ItemType Directory -Force -Path (Join-Path $p '.codex-plugin') | Out-Null
    @{ name = $Plugin; version = '1.0.0'; description = "D19 marker plugin $Plugin"; author = @{ name = 'D19' }; license = 'MIT' } |
        ConvertTo-Json | Set-Content -Path (Join-Path $p '.codex-plugin\plugin.json') -Encoding UTF8
    New-D19Skill (Join-Path $p "skills\$Skill") $Skill "plugin $Plugin"
    return [ordered]@{ name = $Plugin; source = "./plugins/$Plugin"; version = '1.0.0'; description = "D19 marker plugin $Plugin";
                       author = @{ name = 'D19' }; license = 'MIT'; skills = @("./skills/$Skill") }
}
function New-D19Marketplace([string]$Root, [string]$Name, [object[]]$Entries) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Root '.agents\plugins') | Out-Null
    [ordered]@{ name = $Name; owner = @{ name = 'D19' }; metadata = @{ description = "D19 fixture marketplace $Name"; version = '1.0.0' }; plugins = $Entries } |
        ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $Root '.agents\plugins\marketplace.json') -Encoding UTF8
}

# Fixtures
$e = @((New-D19Plugin $D19Mkt 'd19-home-plugin' 'd19-home-skill'), (New-D19Plugin $D19Mkt 'd19-a' 'd19-a-skill'), (New-D19Plugin $D19Mkt 'd19-b' 'd19-b-skill'))
New-D19Marketplace $D19Mkt 'd19' $e
New-D19Marketplace (Join-Path $D19Fx 'mkta') 'd19a' @(New-D19Plugin (Join-Path $D19Fx 'mkta') 'd19-a' 'd19-a-skill')
New-D19Marketplace (Join-Path $D19Fx 'mktb') 'd19b' @(New-D19Plugin (Join-Path $D19Fx 'mktb') 'd19-b' 'd19-b-skill')
New-D19Skill (Join-Path $D19Fx 'loose\d19-a-loose') 'd19-a-loose' 'loose set-A skill outside the home'
New-D19Skill (Join-Path $D19Fx 'loose\d19-b-loose') 'd19-b-loose' 'loose set-B skill outside the home'

# The home
New-D19Skill (Join-Path $D19Home 'skills\d19-user-skill') 'd19-user-skill' 'user-level skill in CODEX_HOME\skills'
New-D19Skill (Join-Path $D19Prof '.agents\skills\d19-agents-skill') 'd19-agents-skill' 'user-level skill in ~\.agents\skills'
$cfg = Join-Path $D19Home 'config.toml'
@"
# D19 synthetic home: the model is the local mock; nothing here is a real account.
model = "h27-mock"
model_provider = "h27"
approval_policy = "never"
sandbox_mode = "danger-full-access"
check_for_update_on_startup = false

[model_providers.h27]
name = "H27 mock"
base_url = "http://127.0.0.1:$H27Port/v1"
wire_api = "responses"
env_key = "H27_API_KEY"

[analytics]
enabled = false

[features]
plugins = true

[mcp_servers.d19-home-mcp]
command = $(ConvertTo-D19TomlStr $D19Node)
args = [$(ConvertTo-D19TomlStr $D19Stub), 'd19-home-mcp']
"@ | Set-Content -Path $cfg -Encoding ASCII

$out = Join-Path $D19Out 'setup'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue; New-Item -ItemType Directory -Force -Path $out | Out-Null
Start-H27Mock -Capture (Join-Path $out 'capture.jsonl') -EgressLog (Join-Path $out 'egress.jsonl') | Out-Null
try {
    foreach ($s in @(@{ n = 'marketplace-add'; a = @('plugin', 'marketplace', 'add', $D19Mkt) },
                     @{ n = 'plugin-add-home'; a = @('plugin', 'add', 'd19-home-plugin@d19') },
                     @{ n = 'plugin-list';     a = @('plugin', 'list') },
                     # first turn of the home: Codex unpacks its system skills, installation_id etc. (a used home)
                     @{ n = 'warm-exec';       a = @('exec', '--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox', 'D19-warm hello') })) {
        $r = Invoke-H27Codex -ArgList $s.a -OutPrefix (Join-Path $out $s.n) -Cwd $H27Work -TimeoutSec 120
        $txt = ((Get-Content $r.Stdout, $r.Stderr -ErrorAction SilentlyContinue) | Where-Object { $_ }) -join ' | '
        "{0}: exit {1}: {2}" -f $s.n, $r.Exit, $txt.Substring(0, [Math]::Min(600, $txt.Length))
    }
} finally { Stop-H27Mock }
Copy-Item $cfg (Join-Path $out 'config.toml.after-setup')
"--- home config.toml after setup ---"; Get-Content $cfg | Select-Object -Skip 18
"plugin cache: $((Get-ChildItem (Join-Path $D19Home 'plugins\cache') -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName.Substring($D19Home.Length + 1) }) -join '; ')"
"egress refused: $(@(Get-Content (Join-Path $out 'egress.jsonl') -ErrorAction SilentlyContinue).Count)"
"raw output: $out"
