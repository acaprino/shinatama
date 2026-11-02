[CmdletBinding()]
param(
    [ValidateSet('Verify', 'Prepare')][string]$Mode = 'Verify',
    [string]$SourceRoot = 'D:/Projects/horness-launcher/packages/billion-context',
    [string]$PreparedRoot = 'D:/Projects/horness-launcher/sidecar/source-build'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Static source/artifact transfer only. Never invoke Node/npm, upstream prepare,
# a server, a model, a package restore or any live configuration/database path.
$buildRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'source-build'))
$lockFile = Join-Path $buildRoot 'provenance.lock.json'
$destination = Join-Path $buildRoot 'prepared'
$stage = Join-Path $buildRoot 'prepared.incomplete'
$maxFileBytes = 32MB
$maxBundleBytes = 128MB
$maxFiles = 1024

function Assert-RegularPath([string]$Root, [string]$Relative) {
    if ($Relative.Length -gt 1024 -or $Relative -notmatch '^[A-Za-z0-9_./-]+$') { throw 'Invalid allowlist path.' }
    $parts = $Relative.Split('/')
    if (@($parts | Where-Object { $_ -in @('', '.', '..') }).Count) { throw 'Invalid relative allowlist path.' }
    $rootFull = [IO.Path]::GetFullPath($Root)
    if (-not [IO.Path]::IsPathRooted($Root)) { throw 'An absolute source directory is required.' }
    $ancestor = $rootFull
    while ($ancestor) {
        $info = Get-Item -LiteralPath $ancestor -Force
        if ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked source/output paths are refused.' }
        $parent = [IO.Directory]::GetParent($ancestor)
        if ($null -eq $parent) { break }
        $ancestor = $parent.FullName
    }
    $path = $rootFull
    foreach ($part in $parts) {
        $path = Join-Path $path $part
        $info = Get-Item -LiteralPath $path -Force
        if ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked allowlist entries are refused.' }
    }
    if ($info.PSIsContainer) { throw 'An allowlist file must be regular.' }
    return $path
}
function Read-BoundedJson([string]$Path) {
    $file = Get-Item -LiteralPath $Path -Force
    if ($file.Length -gt 1MB -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Metadata refused.' }
    return ([IO.File]::ReadAllText($Path) | ConvertFrom-Json -Depth 50)
}
function Hash-File([string]$Path, [long]$ExpectedBytes) {
    if ($ExpectedBytes -lt 0 -or $ExpectedBytes -gt $maxFileBytes) { throw 'File size exceeds the static preparation bound.' }
    # Deny concurrent writers while hashing; a source/build change fails closed.
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -ne $ExpectedBytes) { throw 'Pinned file length mismatch.' }
        return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()
    } finally { $stream.Dispose() }
}
function Assert-Entries($Entries, [string]$Root, [string]$TreeHash) {
    if ($Entries.Count -eq 0 -or $Entries.Count -gt $maxFiles) { throw 'Allowlist count refused.' }
    $previous = ''
    $tree = [Text.StringBuilder]::new()
    foreach ($entry in $Entries) {
        if ($previous -and [StringComparer]::Ordinal.Compare($previous, $entry.path) -ge 0) { throw 'Allowlist must be unique and ordinally sorted.' }
        if ($entry.sha256 -notmatch '^[a-f0-9]{64}$') { throw 'Invalid SHA256 pin.' }
        $path = Assert-RegularPath $Root $entry.path
        if ((Hash-File $path $entry.bytes) -cne $entry.sha256) { throw "Pinned SHA256 mismatch: $($entry.path)" }
        [void]$tree.Append($entry.path).Append([char]0).Append($entry.sha256).Append([char]0).Append([long]$entry.bytes).Append("`n")
        $previous = $entry.path
    }
    $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($tree.ToString()))).ToLowerInvariant()
    if ($actual -cne $TreeHash) { throw 'Allowlist tree SHA256 mismatch.' }
}
function Assert-ContainedOutput([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($buildRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Output escapes this workspace build directory.' }
    return $full
}
function Copy-Entries($Entries, [string]$From, [string]$To, [string]$Prefix = '') {
    foreach ($entry in $Entries) {
        $source = Assert-RegularPath $From $entry.path
        $relative = if ($Prefix) { "$Prefix/$($entry.path)" } else { $entry.path }
        $target = Assert-ContainedOutput (Join-Path $To $relative)
        [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
        [IO.File]::Copy($source, $target, $false)
        if ((Hash-File $target $entry.bytes) -cne $entry.sha256) { throw 'Source changed during preparation; incomplete output retained.' }
    }
}
function Assert-Output([string]$Root, $Lock) {
    Assert-Entries $Lock.sourceFiles $Root $Lock.sourceTreeSha256
    Assert-Entries $Lock.artifactFiles $Root $Lock.artifactTreeSha256
    Assert-Entries $Lock.runtimeDependencyFiles (Join-Path $Root 'node_modules/zod') $Lock.runtimeDependencyTreeSha256
    $identity = Read-BoundedJson (Assert-RegularPath $Root 'bundle.identity.json')
    if ($identity.entrySha256 -cne $Lock.entrySha256 -or $identity.sourceTreeSha256 -cne $Lock.sourceTreeSha256 -or $identity.runtimeQualified) { throw 'Prepared bundle identity mismatch.' }
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in @($Lock.sourceFiles) + @($Lock.artifactFiles)) { [void]$expected.Add($entry.path) }
    foreach ($entry in $Lock.runtimeDependencyFiles) { [void]$expected.Add("node_modules/zod/$($entry.path)") }
    [void]$expected.Add('bundle.identity.json')
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($Root)
    $visited = 0; $files = 0
    while ($pending.Count) {
        foreach ($entry in [IO.Directory]::EnumerateFileSystemEntries($pending.Pop())) {
            if (++$visited -gt 2048) { throw 'Prepared directory enumeration bound exceeded.' }
            $attributes = [IO.File]::GetAttributes($entry)
            if ($attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked prepared entries refused.' }
            if ($attributes -band [IO.FileAttributes]::Directory) { $pending.Push($entry); continue }
            $relative = [IO.Path]::GetRelativePath($Root, $entry).Replace('\', '/')
            if (-not $expected.Contains($relative)) { throw 'Unknown file found in the static prepared bundle.' }
            $files++
        }
    }
    if ($files -ne $expected.Count) { throw 'Prepared bundle file count mismatch.' }
}

$lock = Read-BoundedJson $lockFile
if ($lock.formatVersion -ne 1 -or $lock.packageName -cne 'billion-context' -or $lock.version -cne '0.1.207' -or $lock.upstreamCommit -cne '7e69b49ff4facb00b5ba6541b48d0f7fbe63d63d') { throw 'Unexpected pinned source identity.' }
$count = $lock.sourceFiles.Count + $lock.artifactFiles.Count + $lock.runtimeDependencyFiles.Count
$bytes = (@($lock.sourceFiles) + @($lock.artifactFiles) + @($lock.runtimeDependencyFiles) | Measure-Object -Property bytes -Sum).Sum
if ($count -gt $maxFiles -or $bytes -gt $maxBundleBytes) { throw 'Static preparation budget exceeded.' }
Assert-Entries $lock.sourceFiles $SourceRoot $lock.sourceTreeSha256
Assert-Entries $lock.sourceFiles $PreparedRoot $lock.sourceTreeSha256
Assert-Entries $lock.artifactFiles $PreparedRoot $lock.artifactTreeSha256
Assert-Entries $lock.runtimeDependencyFiles (Join-Path $PreparedRoot 'node_modules/zod') $lock.runtimeDependencyTreeSha256
$musashiRoot = [IO.Directory]::GetParent([IO.Directory]::GetParent([IO.Path]::GetFullPath($SourceRoot)).FullName).FullName
$routePath = Assert-RegularPath $musashiRoot 'res/sidecar-prepare.mjs'
if ((Hash-File $routePath (Get-Item -LiteralPath $routePath).Length) -cne $lock.musashiPreparationRoute.sha256) { throw 'The pinned Musashi preparation route changed.' }
$receiptPath = Assert-RegularPath $PreparedRoot '.musashi-terminal-source-build.json'
$receipt = Read-BoundedJson $receiptPath
if ((Hash-File $receiptPath (Get-Item -LiteralPath $receiptPath).Length) -cne $lock.sourceReceiptSha256 -or $receipt.fingerprint -cne $lock.musashiSourceFingerprint) { throw 'Musashi prepared-source receipt mismatch.' }
$buildInfo = Read-BoundedJson (Assert-RegularPath $PreparedRoot 'dist/build-info.json')
if ($buildInfo.sourceFingerprint -cne $receipt.fingerprint) { throw 'Build fingerprint does not match prepared source.' }
foreach ($entry in $lock.artifactFiles) {
    if ($receipt.artifacts.($entry.path) -cne $entry.sha256) { throw 'Artifact pin differs from existing Musashi receipt.' }
}
$schema = [IO.File]::ReadAllText((Assert-RegularPath $SourceRoot 'src/storage/schema.ts'))
$store = [IO.File]::ReadAllText((Assert-RegularPath $SourceRoot 'src/storage/store.ts'))
$entryText = [IO.File]::ReadAllText((Assert-RegularPath $PreparedRoot $lock.entry))
if ($schema -notmatch 'STORAGE_SCHEMA_VERSION\s*=\s*9' -or $store -notmatch 'private syncCcrEntries\(' -or $store -notmatch 'this\.syncCcrEntries\(s\.sessionId, input\.ccrEntries\)') { throw 'Required source storage correction is absent.' }
foreach ($index in $lock.storage.indexes) {
    if (-not $schema.Contains("CREATE INDEX IF NOT EXISTS $index") -or -not $entryText.Contains($index)) { throw 'A required schema-v9 index is absent.' }
}
if (-not $entryText.Contains('syncCcrEntries')) { throw 'Prepared artifact lacks the incremental CCR method.' }
$identity = [ordered]@{
    formatVersion = 1; packageName = $lock.packageName; version = $lock.version; upstreamCommit = $lock.upstreamCommit
    sourceTreeSha256 = $lock.sourceTreeSha256; artifactTreeSha256 = $lock.artifactTreeSha256
    runtimeDependencyTreeSha256 = $lock.runtimeDependencyTreeSha256; musashiSourceFingerprint = $lock.musashiSourceFingerprint
    entry = $lock.entry; entrySha256 = $lock.entrySha256; sourceFiles = $lock.sourceFiles.Count
    artifactFiles = $lock.artifactFiles.Count; staticBytes = [long]$bytes; nodeExecuted = $false
    requiredNodeEngine = $lock.runtimeMetadata.requiredNodeEngine; nodeVersion = $null; nativeAddonAbi = $null
    cleanBuildPerformed = $false; runtimeQualified = $false; SC16 = 'open'
}
if ($Mode -eq 'Prepare') {
    [void](Assert-ContainedOutput $destination)
    [void](Assert-ContainedOutput $stage)
    if (Test-Path -LiteralPath $destination) { Assert-Output $destination $lock }
    else {
        if (Test-Path -LiteralPath $stage) { throw 'An incomplete preparation already exists; inspect it before retrying.' }
        [void][IO.Directory]::CreateDirectory($stage)
        Copy-Entries $lock.sourceFiles $SourceRoot $stage
        Copy-Entries $lock.artifactFiles $PreparedRoot $stage
        Copy-Entries $lock.runtimeDependencyFiles (Join-Path $PreparedRoot 'node_modules/zod') $stage 'node_modules/zod'
        [IO.File]::WriteAllText((Join-Path $stage 'bundle.identity.json'), ($identity | ConvertTo-Json -Depth 8) + "`n", [Text.UTF8Encoding]::new($false))
        Assert-Output $stage $lock
        # Both resolved absolute targets were checked above and remain inside
        # this repository's named build output. No overwrite/delete is used.
        [IO.Directory]::Move($stage, $destination)
    }
}
$identity['mode'] = $Mode.ToLowerInvariant()
$identity | ConvertTo-Json -Depth 8
