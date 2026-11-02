[CmdletBinding()]
param([ValidateSet('Verify', 'Prepare')][string]$Mode = 'Verify')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# An internal static development copy, not a distributable Node package. No Node,
# npm, installer, network, configuration or credential access is performed.
$pin = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'runtime.lock.json') | ConvertFrom-Json
if ($pin.formatVersion -ne 1 -or $pin.runtime -cne 'node' -or $pin.relativePath -cne 'node.exe' -or $pin.sha256 -notmatch '^[a-f0-9]{64}$' -or $pin.bytes -gt 128MB) { throw 'Invalid runtime pin.' }
$source = [IO.Path]::GetFullPath($pin.sourcePath)
$destination = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'runtime'))
$stage = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'runtime.incomplete'))
function Check-File([string]$Path) {
    $ancestor = $Path
    while ($ancestor) {
        $item = Get-Item -LiteralPath $ancestor -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked runtime paths are refused.' }
        $parent = [IO.Directory]::GetParent($ancestor)
        if ($null -eq $parent) { break }
        $ancestor = $parent.FullName
    }
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
Check-File $source
foreach ($target in @($destination, $stage)) {
    if ([IO.Directory]::GetParent($target).FullName -cne [IO.Path]::GetFullPath($PSScriptRoot)) { throw 'Runtime output escapes the named workspace directory.' }
}
if ($Mode -eq 'Prepare') {
    if (Test-Path -LiteralPath $destination) { Check-File (Join-Path $destination 'node.exe') }
    else {
        if (Test-Path -LiteralPath $stage) { throw 'An incomplete runtime copy already exists; inspect it first.' }
        [void][IO.Directory]::CreateDirectory($stage)
        [IO.File]::Copy($source, (Join-Path $stage 'node.exe'), $false)
        Check-File (Join-Path $stage 'node.exe')
        $receipt = [ordered]@{ formatVersion = 1; runtime = 'node'; relativePath = 'node.exe'; bytes = $pin.bytes
            sha256 = $pin.sha256; declaredFileVersion = $pin.fileVersion; declaredProductVersion = $pin.productVersion
            modulesAbi = $null; nodeExecuted = $false; builtinSqliteQualified = $false; runtimeQualified = $false
            licenseNoticeMissing = $true; distributionReady = $false; licenseProvenance = $pin.license.upstreamNoticeUrl }
        [IO.File]::WriteAllText((Join-Path $stage 'runtime.identity.json'), ($receipt | ConvertTo-Json) + "`n", [Text.UTF8Encoding]::new($false))
        [IO.Directory]::Move($stage, $destination)
    }
}
@{ mode = $Mode.ToLowerInvariant(); runtime = 'node'; fileVersion = $pin.fileVersion; sha256 = $pin.sha256
    bytes = $pin.bytes; nodeExecuted = $false; modulesAbi = $null; runtimeQualified = $false
    licenseNoticeMissing = $true; distributionReady = $false } | ConvertTo-Json
