# D15 / Pi - private home, no leaks (decoy-home method). Rerunnable; resets its scratch.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\D15\pi\run.ps1 [-ContainCompileCache]
# Raw output: %LOCALAPPDATA%\shinatama-verify\D15\pi\<default|contained>\out\
# -ContainCompileCache sets NODE_COMPILE_CACHE inside Shinatama's data folder (Pi's bundle calls
# node:module enableCompileCache(), which otherwise writes to %TEMP%\node-compile-cache).
# Decoy root with empty home/appdata/localappdata(+Temp); USERPROFILE/HOME/APPDATA/LOCALAPPDATA/
# TEMP/TMP point there; PI_CODING_AGENT_DIR = decoy\localappdata\Shinatama\agents\pi\home (where
# Shinatama's data folder would sit). Pass = no file under the decoy outside the private home.
param([switch]$ContainCompileCache)
$ErrorActionPreference = 'Stop'
$BenchRow = 'D15'
. (Join-Path $PSScriptRoot 'bench.ps1')

$Variant = if ($ContainCompileCache) { 'contained' } else { 'default' }
$Base    = Join-Path $BenchRealLocalAppData ('shinatama-verify\D15\pi\' + $Variant)
$Decoy   = Join-Path $Base 'decoy'
$DHome   = Join-Path $Decoy 'home'
$DApp    = Join-Path $Decoy 'appdata'
$DLocal  = Join-Path $Decoy 'localappdata'
$DTemp   = Join-Path $DLocal 'Temp'
$ShData  = Join-Path $DLocal 'Shinatama'
$Private = Join-Path $ShData 'agents\pi\home'
$Work    = Join-Path $Base 'work'
$Out     = Join-Path $Base 'out'
$Log     = Join-Path $Out 'run.log.txt'
$Port    = 4161
$Daodan  = 'D:\Projects\daodan'

Reset-BenchDir $Base
foreach ($d in $DHome, $DApp, $DTemp, $Private, $Work, $Out) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
Set-BenchPi -Root $Base -PiHome $Private -Port $Port -Mock (Join-Path $BenchH27 'server.local.mjs')
Initialize-H27PiHome   # models.json + settings.json (mock provider) into the private home
Write-H27Utf8 (Join-Path $Work 'alpha.txt') 'ALPHA-CONTENT-42'
Write-H27Utf8 (Join-Path $Work 'beta.txt') 'BETA-CONTENT-17'

Write-BenchLog $Log ("pi cli: {0}" -f $H27PiCli)
Write-BenchLog $Log ("node: {0} {1}" -f $H27NodeExe, (& $H27NodeExe --version))
Write-BenchLog $Log ("real ~\.pi\agent newest before: {0}" -f (Get-BenchRealPiStamp))
$before = Get-BenchTree $Decoy
Write-BenchLog $Log ("decoy files before: {0}" -f $before.Count)

# Mock first, with the REAL environment, so its own node process cannot write into the decoy.
$mock = Start-H27Mock (Join-Path $Out 'capture.jsonl')

# Decoy environment for every Pi child from here on.
$env:USERPROFILE = $DHome; $env:HOME = $DHome; $env:APPDATA = $DApp; $env:LOCALAPPDATA = $DLocal
$env:TEMP = $DTemp; $env:TMP = $DTemp
$env:PI_CODING_AGENT_DIR = $Private
if ($ContainCompileCache) { $env:NODE_COMPILE_CACHE = Join-Path $ShData 'agents\pi\node-compile-cache' }
Write-BenchLog $Log ("variant={0} NODE_COMPILE_CACHE={1}" -f $Variant, $env:NODE_COMPILE_CACHE)

try {
  $alpha = Join-Path $Work 'alpha.txt'; $beta = Join-Path $Work 'beta.txt'
  $runs = @()
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('D15-T1 say hello')) $Work 300; Save-H27Run $r (Join-Path $Out 'run1-new'); $runs += $r
  $s = Get-BenchNewestSession $Private
  Write-BenchLog $Log ("run1 new session: exit={0} id={1} out={2}" -f $r.ExitCode, $s.Id, $r.Stdout.Trim())
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('--session', $s.Id, "H27-TOOLS read $alpha and $beta")) $Work 300; Save-H27Run $r (Join-Path $Out 'run2-tools'); $runs += $r
  Write-BenchLog $Log ("run2 --session tools: exit={0} out={1}" -f $r.ExitCode, $r.Stdout.Trim())
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('-c', 'D15-T3 resumed with -c')) $Work 300; Save-H27Run $r (Join-Path $Out 'run3-continue'); $runs += $r
  Write-BenchLog $Log ("run3 -c resume: exit={0} out={1}" -f $r.ExitCode, $r.Stdout.Trim())

  # Same, network-enabled startup (no --offline, no PI_OFFLINE/PI_SKIP_VERSION_CHECK); the dead
  # proxy still blocks every non-mock request. Catches files written by online-only paths.
  $saveOff = $env:PI_OFFLINE; $saveSkip = $env:PI_SKIP_VERSION_CHECK
  Remove-Item Env:PI_OFFLINE, Env:PI_SKIP_VERSION_CHECK -ErrorAction SilentlyContinue
  $online = @('--provider', $H27PiProvider, '--model', $H27PiModel, '--no-skills')
  $r = Invoke-H27Pi (@('-p') + $online + @('-c', 'D15-T4 online startup')) $Work 300; Save-H27Run $r (Join-Path $Out 'run4-online'); $runs += $r
  Write-BenchLog $Log ("run4 online -c: exit={0} out={1} stderr={2}" -f $r.ExitCode, $r.Stdout.Trim(), $r.Stderr.Trim())
  $env:PI_OFFLINE = $saveOff; $env:PI_SKIP_VERSION_CHECK = $saveSkip

  # Daodan from the local checkout (Pi "local" package source: a path, no clone, no network).
  $r = Invoke-H27Pi @('install', $Daodan) $Work 300; Save-H27Run $r (Join-Path $Out 'daodan-install'); $runs += $r
  Write-BenchLog $Log ("pi install <daodan path>: exit={0} out={1} err={2}" -f $r.ExitCode, $r.Stdout.Trim(), $r.Stderr.Trim())
  $r = Invoke-H27Pi @('list') $Work 120; Save-H27Run $r (Join-Path $Out 'daodan-list'); $runs += $r
  Write-BenchLog $Log ("pi list: exit={0} out={1}" -f $r.ExitCode, $r.Stdout.Trim())
  # Does the Daodan package load? One turn; the capture shows whether its skills/prompts reach
  # the system prompt (skills are on here: no --no-skills).
  $r = Invoke-H27Pi (@('-p', '--provider', $H27PiProvider, '--model', $H27PiModel, '--offline') + @('D15-T5 daodan loaded?')) $Work 300; Save-H27Run $r (Join-Path $Out 'run5-daodan'); $runs += $r
  Write-BenchLog $Log ("run5 with daodan: exit={0} out={1} err={2}" -f $r.ExitCode, $r.Stdout.Trim(), $r.Stderr.Trim())
}
finally {
  $env:USERPROFILE = $BenchRealUserProfile; $env:HOME = $BenchRealHome; $env:APPDATA = $BenchRealAppData; $env:LOCALAPPDATA = $BenchRealLocalAppData
  $env:TEMP = $BenchRealTemp; $env:TMP = $BenchRealTemp
  Stop-H27Mock $mock
}

$after = Get-BenchTree $Decoy
$privRel = $Private.Substring($Decoy.Length + 1)
$shRel = $ShData.Substring($Decoy.Length + 1)
$priv = @($after | Where-Object { $_.Rel.StartsWith($privRel + '\') })
$shOther = @($after | Where-Object { $_.Rel.StartsWith($shRel + '\') -and -not $_.Rel.StartsWith($privRel + '\') })
$leaks = @($after | Where-Object { -not $_.Rel.StartsWith($shRel + '\') })
Write-H27Utf8 (Join-Path $Out 'shinatama-other-files.txt') (Format-BenchTree $shOther)
Write-H27Utf8 (Join-Path $Out 'decoy-files-after.txt') (Format-BenchTree $after)
Write-H27Utf8 (Join-Path $Out 'leaks.txt') (Format-BenchTree $leaks)
Write-H27Utf8 (Join-Path $Out 'private-home-files.txt') (Format-BenchTree $priv)
Write-H27Utf8 (Join-Path $Out 'work-files.txt') (Format-BenchTree (Get-BenchTree $Work))
Write-BenchLog $Log ("decoy files after: {0}; under private home: {1}; elsewhere in Shinatama's folder: {2}; OUTSIDE Shinatama's folder (leaks): {3}" -f $after.Count, $priv.Count, $shOther.Count, $leaks.Count)
foreach ($l in $leaks) { Write-BenchLog $Log ("  LEAK {0} ({1} bytes)" -f $l.Rel, $l.Size) }
Write-BenchLog $Log ("real ~\.pi\agent newest after: {0}" -f (Get-BenchRealPiStamp))
$hosts = @(Get-Content (Join-Path $Out 'capture.jsonl') -ErrorAction SilentlyContinue | ForEach-Object { ($_ | ConvertFrom-Json).headers.host } | Sort-Object -Unique)
Write-BenchLog $Log ("mock requests: {0}; hosts: {1}" -f @(Get-Content (Join-Path $Out 'capture.jsonl') -ErrorAction SilentlyContinue).Count, ($hosts -join ','))
Write-BenchLog $Log ("raw output: {0}" -f $Out)
