#Requires -Version 7.4
# Puts the GitHub release files a pin file names into a folder, each checked against its SHA-256.
#
#   pwsh -NoProfile -File scripts\fetch-release.ps1 -Pin scripts\xiox-pin.json -Destination release\xiox
#
# A program Shinatama ships without changing it comes from its release, not from a build of its
# source: Xiox (scripts\xiox-pin.json) and the Mukade terminal browser (scripts\mukade-pin.json).
# The pin's `release` names the repository, the tag and each file with its SHA-256; a release
# whose file differs is refused, never shipped. A file is an asset of the release or, when the pin
# names an `archive` (the portable zip the releases hand out, with its own SHA-256), a file of that
# zip: at its root or in its one top folder. Downloads go through gh, so a private repository
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
if ($release.archive -and (-not $release.archive.name -or -not $release.archive.sha256)) {
    throw "$Pin names an archive without its name and SHA-256."
}
$cache = Join-Path (Split-Path -Parent $PSScriptRoot) ('target\releases\' + ($release.repository -replace '/', '-') + '-' + $release.tag)
New-Item -ItemType Directory -Force -Path $cache, $Destination | Out-Null

function Get-Sha256([string] $Path) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

# A release file into the cache, checked against its SHA-256 (a wrong copy is deleted).
function Save-Checked([string] $Name, [string] $Expected, [scriptblock] $Fetch) {
    $path = Join-Path $cache $Name
    if ((Get-Sha256 $path) -eq $Expected) { return $path }
    & $Fetch $path | Out-Null
    $actual = Get-Sha256 $path
    if ($actual -ne $Expected) {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
        throw "$Name of $($release.repository) $($release.tag) has SHA-256 $actual; $Pin expects $Expected."
    }
    $path
}

function Get-Asset([string] $Name) {
    gh release download $release.tag --repo $release.repository --pattern $Name --dir $cache --clobber
    if ($LASTEXITCODE -ne 0) { throw "gh could not download $Name of $($release.repository) $($release.tag)." }
}

# The portable zip, opened once and only when one of its files is not in the cache yet.
$zip = $null
function Get-FromArchive([string] $Name, [string] $Path) {
    if (-not $script:zip) {
        $archive = Save-Checked $release.archive.name ([string] $release.archive.sha256).ToLowerInvariant() { Get-Asset $release.archive.name }
        $script:zip = [IO.Compression.ZipFile]::OpenRead($archive)
    }
    $entries = @($script:zip.Entries | Where-Object { $_.FullName -ceq $Name -or $_.FullName -cmatch ('^[^/]+/' + [regex]::Escape($Name) + '$') })
    if ($entries.Count -ne 1) { throw "$Name is not once in $($release.archive.name) of $($release.repository) $($release.tag) (found $($entries.Count))." }
    [IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $Path, $true)
}

try {
    foreach ($asset in $release.assets.PSObject.Properties) {
        $name = $asset.Name
        $expected = ([string] $asset.Value).ToLowerInvariant()
        $fetch = if ($release.archive) { { param($p) Get-FromArchive $name $p } } else { { Get-Asset $name } }
        $cached = Save-Checked $name $expected $fetch
        Copy-Item -LiteralPath $cached -Destination (Join-Path $Destination $name) -Force
        Write-Output "$name  $($release.repository) $($release.tag)  sha256 $expected"
    }
} finally {
    if ($zip) { $zip.Dispose() }
}
