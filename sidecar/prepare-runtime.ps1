[CmdletBinding()]
param([ValidateSet('Verify', 'Prepare')][string]$Mode = 'Verify')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Offline static preparation only. No Node/npm/installer execution, network,
# configuration or credential access. License completeness is not legal signoff.
$pin = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'runtime.lock.json') | ConvertFrom-Json
if ($pin.formatVersion -ne 1 -or $pin.runtime -cne 'node' -or $pin.relativePath -cne 'node.exe' -or $pin.sha256 -notmatch '^[a-f0-9]{64}$' -or $pin.bytes -gt 128MB) { throw 'Invalid runtime pin.' }
$source = [IO.Path]::GetFullPath($pin.sourcePath)
$destination = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'runtime'))
$stage = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'runtime.incomplete'))
function Check-Path([string]$Path) {
    $ancestor = $Path
    while ($ancestor) {
        $item = Get-Item -LiteralPath $ancestor -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked runtime paths are refused.' }
        $parent = [IO.Directory]::GetParent($ancestor)
        if ($null -eq $parent) { break }
        $ancestor = $parent.FullName
    }
}
function Check-File([string]$Path) {
    Check-Path $Path
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -ne $pin.bytes -or $item.VersionInfo.FileVersion -cne $pin.fileVersion -or $item.VersionInfo.ProductVersion -cne $pin.productVersion -or $item.VersionInfo.CompanyName -cne $pin.companyName) { throw 'Runtime file/version metadata changed.' }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -ne $pin.bytes) { throw 'Runtime length changed.' }
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()
        if ($hash -cne $pin.sha256) { throw 'Runtime SHA256 mismatch.' }
        $stream.Position = 0; $reader = [IO.BinaryReader]::new($stream, [Text.Encoding]::ASCII, $true)
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Runtime is not a native PE.' }
        $stream.Position = 60; $offset = $reader.ReadUInt32()
        if ($offset -lt 64 -or $offset -gt 1MB) { throw 'Invalid PE header offset.' }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne $pin.peMachine) { throw 'Runtime PE machine mismatch.' }
        $stream.Position = $offset + 22; $flags = $reader.ReadUInt16()
        if (($flags -band 2) -eq 0 -or ($flags -band 0x2000)) { throw 'Runtime is not a native executable.' }
        $reader.Dispose()
    } finally { $stream.Dispose() }
}
function Check-Static([string]$Path, [long]$Bytes, [string]$Sha256, [long]$Maximum) {
    Check-Path $Path
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or $item.Length -gt $Maximum -or ($Bytes -ge 0 -and $item.Length -ne $Bytes)) { throw 'Static license metadata length/type mismatch.' }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -ne $item.Length) { throw 'Static license metadata changed.' }
        if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant() -cne $Sha256) { throw 'Static license SHA256 mismatch.' }
    } finally { $stream.Dispose() }
}
$license = $pin.license
if ($pin.fileVersion -cne '24.14.1' -or $license.sourceRelativePath -cne 'licenses/node-v24.14.1/LICENSE' -or $license.relativePath -cne 'LICENSE' -or $license.checksumRelativePath -cne 'licenses/node-v24.14.1/SHASUMS256.txt' -or $license.provenanceRelativePath -cne 'licenses/node-v24.14.1/provenance.json') { throw 'Unreviewed Node license layout.' }
if ($license.bytes -le 10000 -or $license.bytes -gt 2MB -or $license.archiveExecutableSha256 -cne $pin.sha256 -or $license.archiveUrl -cne 'https://nodejs.org/dist/v24.14.1/node-v24.14.1-win-x64.zip' -or $license.publisherLicenseBundleComplete -isnot [bool] -or -not $license.publisherLicenseBundleComplete -or $license.distributionReady -isnot [bool] -or -not $license.distributionReady -or $license.legalReleaseApproval -isnot [bool] -or $license.legalReleaseApproval) { throw 'Node license completeness/provenance is unavailable.' }
foreach ($digest in @($license.sha256, $license.checksumSha256, $license.provenanceSha256, $license.archiveSha256)) {
    if ($digest -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid license fingerprint.' }
}
$licenseSource = Join-Path $PSScriptRoot $license.sourceRelativePath
$checksumSource = Join-Path $PSScriptRoot $license.checksumRelativePath
$provenanceSource = Join-Path $PSScriptRoot $license.provenanceRelativePath
Check-Static $licenseSource $license.bytes $license.sha256 2MB
Check-Static $checksumSource -1 $license.checksumSha256 64KB
Check-Static $provenanceSource -1 $license.provenanceSha256 16KB
$checksums = [IO.File]::ReadAllText($checksumSource)
if ($checksums -cnotmatch ('(?m)^' + [Regex]::Escape($license.archiveSha256) + '  node-v24\.14\.1-win-x64\.zip\r?$') -or $checksums -cnotmatch ('(?m)^' + [Regex]::Escape($pin.sha256) + '  win-x64/node\.exe\r?$')) { throw 'Official release checksums do not bind the runtime/archive.' }
$provenance = [IO.File]::ReadAllText($provenanceSource) | ConvertFrom-Json
if ($provenance.version -cne $pin.fileVersion -or $provenance.archiveSha256 -cne $license.archiveSha256 -or $provenance.nodeSha256 -cne $pin.sha256 -or $provenance.licenseSha256 -cne $license.sha256 -or $provenance.licenseBytes -ne $license.bytes -or $provenance.licenseArchiveEntry -cne $license.archiveLicenseEntry -or -not $provenance.licenseCopiedVerbatim -or -not $provenance.archiveExeMatchesBundledPin) { throw 'Node license receipt does not match the reviewed bundle.' }
function Write-Identity([string]$Directory) {
    $receipt = [ordered]@{ formatVersion = 1; runtime = 'node'; relativePath = 'node.exe'; bytes = $pin.bytes
        sha256 = $pin.sha256; declaredFileVersion = $pin.fileVersion; declaredProductVersion = $pin.productVersion
        modulesAbi = $null; nodeExecuted = $false; builtinSqliteQualified = $false; runtimeQualified = $false
        licenseNoticeMissing = $false; licenseBundleComplete = $true; distributionReady = $true
        licenseRelativePath = 'LICENSE'; licenseBytes = $license.bytes; licenseSha256 = $license.sha256
        licenseProvenanceSha256 = $license.provenanceSha256; officialArchiveSha256 = $license.archiveSha256
        licenseProvenance = $license.archiveUrl; signatureVerified = $false; legalReleaseApproval = $false }
    $path = Join-Path $Directory 'runtime.identity.json'
    if (Test-Path -LiteralPath $path) { Check-Path $path }
    $pending = Join-Path $Directory 'runtime.identity.incomplete.json'
    $stream = [IO.File]::Open($pending, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($receipt | ConvertTo-Json) + "`n")
        $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
    } finally { $stream.Dispose() }
    [IO.File]::Move($pending, $path, $true)
}
function Check-Prepared([string]$Directory) {
    Check-File (Join-Path $Directory 'node.exe')
    Check-Static (Join-Path $Directory 'LICENSE') $license.bytes $license.sha256 2MB
}
foreach ($target in @($destination, $stage)) {
    if ([IO.Directory]::GetParent($target).FullName -cne [IO.Path]::GetFullPath($PSScriptRoot)) { throw 'Runtime output escapes the named workspace directory.' }
}
if ($Mode -eq 'Prepare') {
    if (Test-Path -LiteralPath $destination) {
        Check-File (Join-Path $destination 'node.exe')
        $licenseDestination = Join-Path $destination 'LICENSE'
        if (-not (Test-Path -LiteralPath $licenseDestination)) { [IO.File]::Copy($licenseSource, $licenseDestination, $false) }
        Check-Prepared $destination
        Write-Identity $destination
    }
    else {
        if (Test-Path -LiteralPath $stage) { throw 'An incomplete runtime copy already exists; inspect it first.' }
        [void][IO.Directory]::CreateDirectory($stage)
        Check-File $source
        [IO.File]::Copy($source, (Join-Path $stage 'node.exe'), $false)
        [IO.File]::Copy($licenseSource, (Join-Path $stage 'LICENSE'), $false)
        Check-Prepared $stage
        Write-Identity $stage
        [IO.Directory]::Move($stage, $destination)
    }
}
Check-Prepared $destination
$identityPath = Join-Path $destination 'runtime.identity.json'
Check-Path $identityPath
if ((Get-Item -LiteralPath $identityPath).Length -gt 16KB) { throw 'Runtime identity exceeds its bound.' }
$identity = [IO.File]::ReadAllText($identityPath) | ConvertFrom-Json
if ($identity.sha256 -cne $pin.sha256 -or $identity.licenseSha256 -cne $license.sha256 -or $identity.licenseProvenanceSha256 -cne $license.provenanceSha256 -or $identity.licenseNoticeMissing -or -not $identity.licenseBundleComplete -or -not $identity.distributionReady -or $identity.legalReleaseApproval) { throw 'Runtime identity does not attest the complete pinned license bundle.' }
@{ mode = $Mode.ToLowerInvariant(); runtime = 'node'; fileVersion = $pin.fileVersion; sha256 = $pin.sha256
    bytes = $pin.bytes; nodeExecuted = $false; modulesAbi = $null; runtimeQualified = $false
    licenseNoticeMissing = $false; licenseBundleComplete = $true; licenseSha256 = $license.sha256
    distributionReady = $true; legalReleaseApproval = $false; signatureVerified = $false } | ConvertTo-Json
