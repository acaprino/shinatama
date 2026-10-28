# PI10 - installed Pi package name, version and update path.
# Read-only: npm registry queries and local package listing. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$cmd = Get-Command pi
$rootg = (npm root -g 2>&1 | Out-String).Trim()
$lines = @("pi shim: $($cmd.Source)", "npm root -g: $rootg")
$pkg = Join-Path $rootg '@earendil-works\pi-coding-agent'
$lines += "pkg dir: $pkg, exists: $(Test-Path $pkg)"
$lines | Set-Content (Join-Path $out 'pi-shim.txt') -Encoding utf8
(npm ls -g --depth=0 2>&1 | Out-String) | Set-Content (Join-Path $out 'npm-ls-g.txt') -Encoding utf8
(npm view '@earendil-works/pi-coding-agent' version dist-tags 2>&1 | Out-String) | Set-Content (Join-Path $out 'npm-view-earendil.txt') -Encoding utf8
(npm view '@mariozechner/pi-coding-agent' version 2>&1 | Out-String) | Set-Content (Join-Path $out 'npm-view-mariozechner.txt') -Encoding utf8
$lines
"npm ls -g written to $out"
