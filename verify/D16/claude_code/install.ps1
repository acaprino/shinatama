# D16 / Claude Code - obtain given versions into private folders with npm (the published
# package @anthropic-ai/claude-code ships the native binary through a per-platform optional
# dependency @anthropic-ai/claude-code-win32-x64 and a postinstall install.cjs).
# Each version lands in  %LOCALAPPDATA%\shinatama-verify\D16\claude_code\versions\<ver>\
# npm runs with USERPROFILE/HOME/APPDATA/LOCALAPPDATA/TEMP/TMP pointed at an empty decoy and
# with its cache inside the D16 folder, so anything written outside the version folder shows
# up in the decoy listing (install-outside.<ver>.txt).
# Usage: pwsh -File install.ps1 [-Versions 2.1.312,2.1.311]
# -VersionsRoot: target parent folder (default <row folder>\versions); npm cache next to it.
param([string[]]$Versions = @('2.1.312', '2.1.311'), [string]$VersionsRoot = '')
$ErrorActionPreference = 'Stop'
$Root = Join-Path $env:LOCALAPPDATA 'shinatama-verify\D16\claude_code'
$npm = (Get-Command npm.cmd).Source
$node = (Get-Command node).Source
# --version runs of the installed binaries use a scratch config dir, never the real one.
$versionCheckHome = Join-Path $Root 'version-check-home'

foreach ($n in @(Get-ChildItem Env: | Where-Object { $_.Name -match '^(CLAUDE|ANTHROPIC)|^AI_AGENT$' } | ForEach-Object { $_.Name })) {
  Remove-Item "Env:$n" -ErrorAction SilentlyContinue
}
$env:CLAUDE_CONFIG_DIR = $versionCheckHome
$env:DISABLE_AUTOUPDATER = '1'

function Get-Listing([string]$dir) {
  if (-not (Test-Path $dir)) { return @() }
  Get-ChildItem -LiteralPath $dir -Recurse -Force -File -ErrorAction SilentlyContinue |
    ForEach-Object { "{0}`t{1}" -f $_.FullName.Substring($dir.Length).TrimStart('\'), $_.Length } | Sort-Object
}

foreach ($v in $Versions) {
  $vroot = if ($VersionsRoot) { $VersionsRoot } else { Join-Path $Root 'versions' }
  $dest = Join-Path $vroot $v
  $decoy = Join-Path $Root "decoy-install-$v"
  $cache = Join-Path (Split-Path $vroot) 'npm-cache'
  if (Test-Path $decoy) { Remove-Item -Recurse -Force $decoy }
  $d = @{ home = "$decoy\home"; appdata = "$decoy\appdata"; localappdata = "$decoy\localappdata"; temp = "$decoy\temp" }
  New-Item -ItemType Directory -Force -Path $dest, $cache, $d.home, $d.appdata, $d.localappdata, $d.temp | Out-Null
  $saved = @{}
  foreach ($k in 'USERPROFILE', 'HOME', 'APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP') { $saved[$k] = [Environment]::GetEnvironmentVariable($k) }
  $env:USERPROFILE = $d.home; $env:HOME = $d.home; $env:APPDATA = $d.appdata; $env:LOCALAPPDATA = $d.localappdata
  $env:TEMP = $d.temp; $env:TMP = $d.temp
  $log = Join-Path $Root "install.$v.log"
  $t0 = Get-Date
  & $npm install --prefix $dest --cache $cache --no-audit --no-fund --loglevel info "@anthropic-ai/claude-code@$v" *> $log
  $code = $LASTEXITCODE
  foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
  $secs = [int]((Get-Date) - $t0).TotalSeconds
  $exe = Join-Path $dest 'node_modules\@anthropic-ai\claude-code\bin\claude.exe'
  $outside = Get-Listing $decoy
  Set-Content -Path (Join-Path $Root "install-outside.$v.txt") -Value $outside -Encoding utf8
  $ver = if (Test-Path $exe) { (& $exe --version 2>&1 | Out-String).Trim() } else { 'missing' }
  "{0} npm-exit={1} secs={2} exe={3} exists={4} size={5} --version='{6}' decoy-files={7}" -f $v, $code, $secs, $exe, (Test-Path $exe), ((Get-Item $exe -ErrorAction SilentlyContinue).Length), $ver, @($outside).Count |
    Tee-Object -Append -FilePath (Join-Path $Root 'install-summary.txt')
}
