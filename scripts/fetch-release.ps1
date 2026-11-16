#Requires -Version 7.4
# Puts the GitHub release assets a pin file names into a folder, each checked against its SHA-256.
#
#   pwsh -NoProfile -File scripts\fetch-release.ps1 -Pin scripts\xiox-pin.json -Destination release\xiox
#
# A program Shinatama ships without changing it comes from its release, not from a build of its
# source: Xiox (scripts\xiox-pin.json) and the Mukade terminal browser (scripts\mukade-pin.json).
# The pin's `release` names the repository, the tag and each asset with its SHA-256; a release
# whose file differs is refused, never shipped. Downloads go through gh, so a private repository
# works with gh's own sign-in. A verified copy is kept in target\releases and reused.
param(
    [Parameter(Mandatory)][string] $Pin,
    [Parameter(Mandatory)][string] $Destination
)
$ErrorActionPreference = 'Stop'

$release = (Get-Content -Raw -LiteralPath $Pin | ConvertFrom-Json).release
if (-not $release.repository -or -not $release.tag -or -not $release.assets) {
    throw "$Pin names no release (repository, tag, assets)."
}
$cache = Join-Path (Split-Path -Parent $PSScriptRoot) ('target\releases\' + ($release.repository -replace '/', '-') + '-' + $release.tag)
New-Item -ItemType Directory -Force -Path $cache, $Destination | Out-Null

function Get-Sha256([string] $Path) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

foreach ($asset in $release.assets.PSObject.Properties) {
    $name = $asset.Name
    $expected = ([string] $asset.Value).ToLowerInvariant()
    $cached = Join-Path $cache $name
    if ((Get-Sha256 $cached) -ne $expected) {
        gh release download $release.tag --repo $release.repository --pattern $name --dir $cache --clobber
        if ($LASTEXITCODE -ne 0) { throw "gh could not download $name of $($release.repository) $($release.tag)." }
        $actual = Get-Sha256 $cached
        if ($actual -ne $expected) {
            Remove-Item -LiteralPath $cached -Force -ErrorAction SilentlyContinue
            throw "$name of $($release.repository) $($release.tag) has SHA-256 $actual; $Pin expects $expected."
        }
    }
    Copy-Item -LiteralPath $cached -Destination (Join-Path $Destination $name) -Force
    Write-Output "$name  $($release.repository) $($release.tag)  sha256 $expected"
}
