# H27 / Pi - dot-source this file: . "$PSScriptRoot\env.ps1"
# Points pi 1.2.0 at a scratch home under %LOCALAPPDATA%\shinatama-verify\H27\pi\ and at the
# H27 mock (http://127.0.0.1:4134, Anthropic Messages). Writes config only into the scratch home.
# Process-scoped: run the scripts with `powershell -NoProfile -File`, not in a shell you keep.

$H27PiRoot    = Join-Path $env:LOCALAPPDATA 'shinatama-verify\H27\pi'
$H27PiHome    = Join-Path $H27PiRoot 'home'
$H27PiWork    = Join-Path $H27PiRoot 'work'
$H27PiPort    = 4134
$H27PiRepo    = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
# Local copy of verify/H27/mock/server.mjs: Pi's compaction request quotes "H27-TOOLS", so the
# shared mock answered it with H27-NOTOOL; the copy answers a tool-less summary request first.
$H27PiMock    = Join-Path $PSScriptRoot 'server.local.mjs'
$H27PiOut     = Join-Path $H27PiRepo 'verify\H27\out\2026-10-27\pi'
$H27PiProvider = 'h27'
$H27PiModel    = 'h27-mock'

# Pi's own root and storage: scratch only.
$env:PI_CODING_AGENT_DIR = $H27PiHome
Remove-Item Env:PI_CODING_AGENT_SESSION_DIR -ErrorAction SilentlyContinue
Remove-Item Env:PI_PACKAGE_DIR -ErrorAction SilentlyContinue

# No automatic network: no catalog refresh, no version check, no telemetry.
$env:PI_OFFLINE = '1'
$env:PI_SKIP_VERSION_CHECK = '1'
$env:PI_TELEMETRY = '0'

# Any outbound request that is not to the mock goes to a dead proxy (port 9) and fails.
$env:HTTP_PROXY  = 'http://127.0.0.1:9'
$env:HTTPS_PROXY = 'http://127.0.0.1:9'
$env:NO_PROXY    = '127.0.0.1,localhost'
$env:no_proxy    = '127.0.0.1,localhost'

# No real credential reachable from this process.
foreach ($v in 'ANTHROPIC_API_KEY','ANTHROPIC_AUTH_TOKEN','OPENAI_API_KEY','OPENROUTER_API_KEY','GEMINI_API_KEY','GOOGLE_API_KEY') {
  Remove-Item "Env:$v" -ErrorAction SilentlyContinue
}

# Common flags for every pi run: the mock model, nothing loaded from outside the scratch home
# (~/.agents/skills holds the user's real skills; -ns keeps them out of the prompt).
$H27PiFlags = @('--provider', $H27PiProvider, '--model', $H27PiModel, '--offline', '--no-skills')

function Write-H27Utf8([string]$Path, [string]$Text) {
  New-Item -ItemType Directory -Force -Path (Split-Path $Path) | Out-Null
  [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding $false))
}

function Initialize-H27PiHome {
  # Custom provider speaking Anthropic Messages to the mock; literal dummy key.
  $models = @"
{
  "providers": {
    "$H27PiProvider": {
      "baseUrl": "http://127.0.0.1:$H27PiPort",
      "api": "anthropic-messages",
      "apiKey": "h27-dummy",
      "models": [
        {
          "id": "$H27PiModel",
          "name": "H27 mock",
          "reasoning": true,
          "input": ["text"],
          "contextWindow": 200000,
          "maxTokens": 8192,
          "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
        }
      ]
    }
  }
}
"@
  # keepRecentTokens is tiny so a manual compaction of this short session summarizes T1 and
  # keeps the T2 span: Pi estimates chars/4, T2 = user 4 + assistant 7 tokens, so 10 cuts at
  # the T2 user message (1 would cut at the T2 assistant: a "split turn" with two summary calls).
  # Auto-compaction stays at its defaults.
  $settings = @"
{
  "defaultProvider": "$H27PiProvider",
  "defaultModel": "$H27PiModel",
  "compaction": { "keepRecentTokens": 10 },
  "cacheWarming": "off",
  "enableAnalytics": false,
  "enableInstallTelemetry": false
}
"@
  Write-H27Utf8 (Join-Path $H27PiHome 'models.json') $models
  Write-H27Utf8 (Join-Path $H27PiHome 'settings.json') $settings
}

if (-not (Test-Path (Join-Path $H27PiHome 'models.json'))) { Initialize-H27PiHome }
