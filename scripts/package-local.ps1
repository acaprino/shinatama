#requires -Version 7.4
[CmdletBinding()]
param([switch]$Verify, [string]$Package)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Existing binaries and pinned resources only: no build, restore, network, native
# execution, app launch, user state copy, deletion or overwrite of old artifacts.
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$portableRoot = Join-Path $workspace '.verification/portable'
$maxFileBytes = 512MB
$maxTotalBytes = 1GB
$maxFiles = 1024
$maxVisited = 4096
$maxDepth = 32
$utf8 = [Text.UTF8Encoding]::new($false)
$launcher = @'
@echo off
setlocal
if not exist "%~dp0shinatama-desktop.exe" goto missing
if not exist "%~dp0shinatama-terminal-helper.exe" goto missing
if not exist "%~dp0shinatama-session-helper.exe" goto missing
if not exist "%~dp0shinatama-hook.exe" goto missing
start "" /d "%~dp0" "%~dp0shinatama-desktop.exe"
exit /b 0
:missing
echo Il pacchetto locale di Shinatama e incompleto.
echo Verificare il pacchetto prima di avviarlo.
pause
exit /b 1
'@
$launcherBytes = $utf8.GetBytes(($launcher -replace "`r?`n", "`r`n") + "`r`n")

function Assert-NoLink([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    $at = $full
    while ($at) {
        $item = Get-Item -LiteralPath $at -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse/linked package paths are refused.' }
        $parent = [IO.Directory]::GetParent($at)
        if ($null -eq $parent) { break }
        $at = $parent.FullName
    }
    return $full
}
function Assert-Relative([string]$Relative) {
    if (-not $Relative -or $Relative.Length -gt 1024 -or $Relative -cnotmatch '^[A-Za-z0-9_./-]+$') { throw 'Unsafe package relative path.' }
    foreach ($part in $Relative.Split('/')) { if ($part -in @('', '.', '..') -or $part.EndsWith('.')) { throw 'Package traversal/ambiguous name refused.' } }
}
function Under([string]$Root, [string]$Relative) {
    Assert-Relative $Relative
    $full = [IO.Path]::GetFullPath((Join-Path $Root $Relative))
    if (-not $full.StartsWith([IO.Path]::GetFullPath($Root) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package path escaped its root.' }
    return $full
}
function Regular([string]$Root, [string]$Relative) {
    $path = Assert-NoLink (Under $Root $Relative)
    if ((Get-Item -LiteralPath $path -Force).PSIsContainer) { throw 'Expected an ordinary package file.' }
    return $path
}
function Read-Json([string]$Path, [long]$Maximum = 1MB) {
    [void](Assert-NoLink $Path)
    $file = Get-Item -LiteralPath $Path -Force
    if ($file.PSIsContainer -or $file.Length -gt $Maximum) { throw 'Package metadata exceeds its fixed bound.' }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -gt $Maximum) { throw 'Package metadata changed.' }
        $reader = [IO.StreamReader]::new($stream, $utf8, $true, 65536, $true)
        try { $json = $reader.ReadToEnd(); $value = $json | ConvertFrom-Json -AsHashtable -Depth 32 } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
    if ($value -isnot [Collections.IDictionary]) { throw 'Package metadata must be an object.' }
    # ConvertFrom-Json coerces ISO dates to DateTime on supported PowerShell
    # versions. Preserve top-level JSON strings for the exact metadata schema;
    # JsonDocument also keeps this compatible with PowerShell 7.4 (no DateKind).
    $document = [Text.Json.JsonDocument]::Parse($json)
    try {
        foreach ($property in $document.RootElement.EnumerateObject()) {
            if ($property.Value.ValueKind -eq [Text.Json.JsonValueKind]::String) { $value[$property.Name] = $property.Value.GetString() }
        }
    } finally { $document.Dispose() }
    return $value
}
function Keys-Exactly($Value, [string[]]$Names) {
    if ($Value -isnot [Collections.IDictionary] -or $Value.Count -ne $Names.Count) { throw 'Unknown package metadata fields.' }
    foreach ($name in $Names) { if (-not $Value.Contains($name)) { throw 'Missing package metadata field.' } }
}
function Integer($Value, [long]$Maximum) {
    return ($Value -is [int] -or $Value -is [long]) -and $Value -ge 0 -and $Value -le $Maximum
}
function Digest([string]$Path, [long]$ExpectedBytes = -1) {
    [void](Assert-NoLink $Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -gt $maxFileBytes -or ($ExpectedBytes -ge 0 -and $stream.Length -ne $ExpectedBytes)) { throw 'Package file length mismatch/bound exceeded.' }
        return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()
    } finally { $stream.Dispose() }
}
function Write-New([string]$Path, [byte[]]$Bytes) {
    [void](Assert-NoLink ([IO.Path]::GetDirectoryName($Path)))
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($Bytes, 0, $Bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}
function Ensure-Directory([string]$Root, [string]$Relative) {
    [void](Assert-NoLink $Root)
    Assert-Relative $Relative
    $at = $Root
    foreach ($part in $Relative.Split('/')) {
        $at = Join-Path $at $part
        if (-not (Test-Path -LiteralPath $at)) { [void][IO.Directory]::CreateDirectory($at) }
        [void](Assert-NoLink $at)
        if (-not (Get-Item -LiteralPath $at).PSIsContainer) { throw 'A package directory component is a file.' }
    }
}
function Files-Bounded([string]$Root) {
    [void](Assert-NoLink $Root)
    $files = [Collections.Generic.List[string]]::new()
    $pending = [Collections.Generic.Stack[object]]::new()
    $pending.Push(@{ path = $Root; depth = 0 })
    $visited = 0; [long]$total = 0
    while ($pending.Count) {
        $directory = $pending.Pop()
        foreach ($path in [IO.Directory]::EnumerateFileSystemEntries($directory.path)) {
            if (++$visited -gt $maxVisited) { throw 'Package directory walk exceeded its entry bound.' }
            $item = Get-Item -LiteralPath $path -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked package entry refused.' }
            if ($item.PSIsContainer) {
                if ($directory.depth -ge $maxDepth) { throw 'Package directory depth exceeded.' }
                $pending.Push(@{ path = $path; depth = $directory.depth + 1 }); continue
            }
            if ($files.Count -ge $maxFiles -or $item.Length -gt $maxFileBytes) { throw 'Package file count/size bound exceeded.' }
            $total += $item.Length
            if ($total -gt $maxTotalBytes) { throw 'Package cumulative byte bound exceeded.' }
            $relative = [IO.Path]::GetRelativePath($Root, $path).Replace('\', '/')
            Assert-Relative $relative; $files.Add($relative)
        }
    }
    return ,$files
}
function Expected-Prepared($Pin) {
    if ($Pin.formatVersion -ne 1 -or $Pin.packageName -cne 'billion-context' -or $Pin.version -cne '0.1.207') { throw 'Unreviewed sidecar identity.' }
    $entries = [Collections.Generic.Dictionary[string, object]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($group in @(@{ entries = $Pin.sourceFiles; prefix = '' }, @{ entries = $Pin.artifactFiles; prefix = '' }, @{ entries = $Pin.runtimeDependencyFiles; prefix = 'node_modules/zod/' })) {
        foreach ($entry in $group.entries) {
            Assert-Relative $entry.path
            if (-not (Integer $entry.bytes 32MB) -or $entry.sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid sidecar file pin.' }
            $entries.Add(($group.prefix + $entry.path), $entry)
            if ($entries.Count -gt 609) { throw 'Unexpected sidecar allowlist count.' }
        }
    }
    if ($entries.Count -ne 609) { throw 'The reviewed sidecar pin must contain exactly 609 files before its generated identity.' }
    $entries.Add('bundle.identity.json', $null)
    return ,$entries
}
function Assert-License($Pin, $Identity) {
    if ($Pin.formatVersion -ne 1 -or $Pin.runtime -cne 'node' -or $Pin.relativePath -cne 'node.exe' -or
        -not (Integer $Pin.bytes 128MB) -or $Pin.sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid pinned runtime identity.' }
    $license = $Pin.license
    if ($license.relativePath -cne 'LICENSE' -or -not (Integer $license.bytes 2MB) -or $license.bytes -lt 10000 -or
        $license.sha256 -cnotmatch '^[a-f0-9]{64}$' -or $license.archiveExecutableSha256 -cne $Pin.sha256 -or
        $license.publisherLicenseBundleComplete -isnot [bool] -or -not $license.publisherLicenseBundleComplete -or
        $license.distributionReady -isnot [bool] -or -not $license.distributionReady -or
        $license.legalReleaseApproval -isnot [bool] -or $license.legalReleaseApproval) { throw 'The complete pinned Node license is required; no legal release approval is inferred.' }
    if ($Identity.formatVersion -ne 1 -or $Identity.sha256 -cne $Pin.sha256 -or $Identity.bytes -ne $Pin.bytes -or
        $Identity.licenseRelativePath -cne 'LICENSE' -or $Identity.licenseSha256 -cne $license.sha256 -or $Identity.licenseBytes -ne $license.bytes -or
        $Identity.licenseProvenanceSha256 -cne $license.provenanceSha256 -or $Identity.officialArchiveSha256 -cne $license.archiveSha256 -or
        $Identity.licenseNoticeMissing -isnot [bool] -or $Identity.licenseNoticeMissing -or
        $Identity.licenseBundleComplete -isnot [bool] -or -not $Identity.licenseBundleComplete -or
        $Identity.distributionReady -isnot [bool] -or -not $Identity.distributionReady -or
        $Identity.legalReleaseApproval -isnot [bool] -or $Identity.legalReleaseApproval) { throw 'Runtime identity does not attest the complete static publisher license.' }
}
function Git-Head {
    # Read only commit/ref metadata. Never execute git, hooks, filters or config.
    $gitDirectory = Assert-NoLink (Join-Path $workspace '.git')
    if (-not (Get-Item -LiteralPath $gitDirectory -Force).PSIsContainer) { throw 'This local packager requires the checkout .git directory.' }
    $headFile = Regular $gitDirectory 'HEAD'
    if ((Get-Item -LiteralPath $headFile -Force).Length -gt 512) { throw 'Git HEAD exceeds its bound.' }
    $head = [IO.File]::ReadAllText($headFile).Trim()
    if ($head -cmatch '^[a-f0-9]{40}$') { return $head }
    if ($head -cnotmatch '^ref: (refs/[A-Za-z0-9_./-]+)$') { throw 'Unrecognized Git HEAD.' }
    $reference = $Matches[1]; Assert-Relative $reference
    $refPath = Under $gitDirectory $reference
    if (Test-Path -LiteralPath $refPath) {
        [void](Assert-NoLink $refPath)
        if ((Get-Item -LiteralPath $refPath -Force).Length -gt 128) { throw 'Git ref exceeds its bound.' }
        $value = [IO.File]::ReadAllText($refPath).Trim()
        if ($value -cmatch '^[a-f0-9]{40}$') { return $value }
        throw 'Invalid Git commit ref.'
    }
    $packed = Regular $gitDirectory 'packed-refs'
    if ((Get-Item -LiteralPath $packed -Force).Length -gt 4MB) { throw 'Git packed refs exceed their bound.' }
    $count = 0
    foreach ($line in [IO.File]::ReadLines($packed)) {
        if (++$count -gt 65536) { throw 'Git ref count exceeded.' }
        if ($line -cmatch '^([a-f0-9]{40}) (refs/[A-Za-z0-9_./-]+)$' -and $Matches[2] -ceq $reference) { return $Matches[1] }
    }
    throw 'The checkout commit could not be resolved.'
}
function Assert-Native([string]$Path, [bool]$Desktop) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -lt 256 -or $stream.Length -gt $maxFileBytes) { throw 'Invalid bounded PE executable.' }
        $reader = [IO.BinaryReader]::new($stream, [Text.Encoding]::ASCII, $true)
        try {
            if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Executable is not a PE.' }
            $stream.Position = 60; $offset = $reader.ReadUInt32()
            if ($offset -lt 64 -or $offset -gt 1MB -or $offset + 96 -gt $stream.Length) { throw 'PE header exceeds its bound.' }
            $stream.Position = $offset
            if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'Only the reviewed Windows x64 binaries are packaged.' }
            $stream.Position = $offset + 22; $flags = $reader.ReadUInt16()
            if (($flags -band 2) -eq 0 -or ($flags -band 0x2000)) { throw 'A native executable, not a DLL, is required.' }
            $stream.Position = $offset + 24
            if ($reader.ReadUInt16() -ne 0x20b) { throw 'Only PE32+ executables are supported.' }
            $stream.Position = $offset + 24 + 68; $subsystem = $reader.ReadUInt16()
            if ($Desktop -and $subsystem -ne 2) { throw 'Rebuild the desktop as Windows GUI before packaging; no console-window fallback is packaged.' }
        } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
}
function Add-AgentPlugins($Plan, [string]$PluginsRoot) {
    $lock = Read-Json (Regular $PluginsRoot 'plugins.lock.json') 64KB
    Keys-Exactly $lock @('formatVersion', 'marketplace', 'plugins')
    if ($lock.formatVersion -ne 1) { throw 'Unknown agent plugin lock format.' }
    $names = @('ponytail', 'i-have-adhd')
    Keys-Exactly $lock.plugins $names
    $prefix = 'resources/agent-plugins'
    $lockRelative = "$prefix/plugins.lock.json"
    $Plan.Add($lockRelative, @{ source = $lockRelative; expected = $null })
    Keys-Exactly $lock.marketplace @('bytes', 'sha256')
    if (-not (Integer $lock.marketplace.bytes 64KB) -or $lock.marketplace.sha256 -cnotmatch '^[a-f0-9]{64}$') {
        throw 'Invalid local agent plugin marketplace pin.'
    }
    $marketplaceRelative = "$prefix/.agents/plugins/marketplace.json"
    $Plan.Add($marketplaceRelative, @{ source = $marketplaceRelative; expected = $lock.marketplace })
    foreach ($name in $names) {
        $plugin = $lock.plugins[$name]
        Keys-Exactly $plugin @('repository', 'commit', 'version', 'license', 'files')
        if ($plugin.repository -cnotmatch '^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or
            $plugin.commit -cnotmatch '^[a-f0-9]{40}$' -or $plugin.version -cnotmatch '^\d+\.\d+\.\d+$' -or
            $plugin.license -cne 'MIT' -or $plugin.files -isnot [array] -or $plugin.files.Count -gt $maxFiles) {
            throw "Invalid agent plugin pin: $name"
        }
        foreach ($file in $plugin.files) {
            Keys-Exactly $file @('path', 'bytes', 'sha256')
            Assert-Relative $file.path
            if (-not (Integer $file.bytes $maxFileBytes) -or $file.sha256 -cnotmatch '^[a-f0-9]{64}$') {
                throw "Invalid agent plugin file pin: $name/$($file.path)"
            }
            $relative = "$prefix/$name/$($file.path)"
            $Plan.Add($relative, @{ source = $relative; expected = $file })
        }
        foreach ($required in @('LICENSE', '.claude-plugin/plugin.json', '.codex-plugin/plugin.json', "skills/$name/SKILL.md")) {
            if (-not $Plan.ContainsKey("$prefix/$name/$required")) { throw "Agent plugin is missing a required pin: $name/$required" }
        }
    }
}
function Plan([string]$ResourcesRoot, [string]$PluginsRoot) {
    $pin = Read-Json (Regular $ResourcesRoot 'source-build/provenance.lock.json')
    $bundle = Read-Json (Regular $ResourcesRoot 'source-build/prepared/bundle.identity.json') 16KB
    if ($bundle.formatVersion -ne 1 -or $bundle.packageName -cne $pin.packageName -or $bundle.version -cne $pin.version -or
        $bundle.upstreamCommit -cne $pin.upstreamCommit -or $bundle.sourceTreeSha256 -cne $pin.sourceTreeSha256 -or
        $bundle.artifactTreeSha256 -cne $pin.artifactTreeSha256 -or $bundle.runtimeDependencyTreeSha256 -cne $pin.runtimeDependencyTreeSha256 -or
        $bundle.entry -cne $pin.entry -or $bundle.entrySha256 -cne $pin.entrySha256 -or
        $bundle.sourceFiles -ne $pin.sourceFiles.Count -or $bundle.artifactFiles -ne $pin.artifactFiles.Count -or
        $bundle.runtimeQualified -isnot [bool] -or $bundle.runtimeQualified -or
        $bundle.nodeExecuted -isnot [bool] -or $bundle.nodeExecuted -or $bundle.SC16 -cne 'open') { throw 'Prepared sidecar identity differs from its static provenance.' }
    $runtime = Read-Json (Regular $ResourcesRoot 'runtime.lock.json') 16KB
    $identity = Read-Json (Regular $ResourcesRoot 'runtime/runtime.identity.json') 16KB
    Assert-License $runtime $identity
    $prepared = Expected-Prepared $pin
    $plan = [Collections.Generic.Dictionary[string, object]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($relative in $prepared.Keys) {
        $expected = $prepared[$relative]
        $plan.Add(('resources/sidecar/source-build/prepared/' + $relative), @{ source = "source-build/prepared/$relative"; expected = $expected })
    }
    foreach ($relative in @('source-build/provenance.lock.json', 'runtime.lock.json', 'runtime/runtime.identity.json')) {
        $plan.Add("resources/sidecar/$relative", @{ source = $relative; expected = $null })
    }
    $plan.Add('resources/sidecar/runtime/node.exe', @{ source = 'runtime/node.exe'; expected = @{ bytes = $runtime.bytes; sha256 = $runtime.sha256 } })
    $plan.Add('resources/sidecar/runtime/LICENSE', @{ source = 'runtime/LICENSE'; expected = @{ bytes = $runtime.license.bytes; sha256 = $runtime.license.sha256 } })
    foreach ($name in @('shinatama-desktop.exe', 'shinatama-terminal-helper.exe', 'shinatama-session-helper.exe', 'shinatama-hook.exe', 'AVVIO.md', 'Shinatama.cmd')) {
        $plan.Add($name, @{ source = $name; expected = $null })
    }
    if ($PluginsRoot) { Add-AgentPlugins $plan $PluginsRoot }
    return ,$plan
}
function Verify-Package([string]$Directory) {
    $directory = Assert-NoLink $Directory
    if (-not [string]::Equals([IO.Directory]::GetParent($directory).FullName, [IO.Path]::GetFullPath($portableRoot), [StringComparison]::OrdinalIgnoreCase)) { throw 'Verify accepts only a recorded local portable staging directory.' }
    $id = [IO.Path]::GetFileName($directory)
    $parsed = [Guid]::Empty
    if (-not [Guid]::TryParseExact($id, 'D', [ref]$parsed) -or $parsed.ToString('D') -cne $id) { throw 'Invalid staging identity.' }
    $receipt = Read-Json (Regular $portableRoot "$id.receipt.json") 16KB
    $receiptKeys = @('schemaVersion', 'packageId', 'gitHead', 'fileCount', 'totalBytes', 'manifestSha256', 'sourcePinSha256', 'runtimePinSha256')
    $pluginsRoot = ''
    if ($receipt.schemaVersion -eq 2) {
        $receiptKeys += 'agentPluginsPinSha256'
        $pluginsRoot = Join-Path $directory 'resources/agent-plugins'
    }
    Keys-Exactly $receipt $receiptKeys
    if ($receipt.schemaVersion -notin @(1, 2) -or $receipt.packageId -cne $id -or $receipt.manifestSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Local package receipt refused.' }
    if ($pluginsRoot -and (Digest (Regular $pluginsRoot 'plugins.lock.json')) -cne $receipt.agentPluginsPinSha256) {
        throw 'Agent plugin pin differs from the trusted local receipt.'
    }
    $manifestPath = Regular $directory 'package-manifest.json'
    $manifestBytes = (Get-Item -LiteralPath $manifestPath).Length
    if ($manifestBytes -gt 1MB) { throw 'Package manifest exceeds its metadata bound.' }
    if ((Digest $manifestPath $manifestBytes) -cne $receipt.manifestSha256) { throw 'Package manifest differs from its independent workspace receipt.' }
    $manifest = Read-Json $manifestPath
    Keys-Exactly $manifest @('schemaVersion', 'product', 'packageId', 'buildMode', 'gitHead', 'binaryBuildRevisionVerified', 'createdUtc', 'nativeAppExecuted', 'releaseApproved', 'scope', 'totalBytes', 'files')
    if ($manifest.schemaVersion -ne 1 -or $manifest.product -cne 'Shinatama' -or $manifest.packageId -cne $id -or
        $manifest.buildMode -cne 'existing-target-debug' -or $manifest.gitHead -cnotmatch '^[a-f0-9]{40}$' -or
        $manifest.gitHead -cne $receipt.gitHead -or $manifest.releaseApproved -isnot [bool] -or $manifest.releaseApproved -or
        $manifest.nativeAppExecuted -isnot [bool] -or $manifest.nativeAppExecuted -or
        $manifest.binaryBuildRevisionVerified -isnot [bool] -or $manifest.binaryBuildRevisionVerified -or
        $manifest.createdUtc -isnot [string] -or $manifest.createdUtc.Length -gt 64 -or
        $manifest.scope -cne 'local debug portable files; public release/X3 gates remain open' -or
        $manifest.files -isnot [array] -or -not (Integer $manifest.totalBytes $maxTotalBytes) -or
        $manifest.files.Count -gt $maxFiles -or $manifest.files.Count -ne $receipt.fileCount) { throw 'Unknown/inconsistent local package manifest schema.' }
    $plan = Plan (Join-Path $directory 'resources/sidecar') $pluginsRoot
    if ($manifest.files.Count -ne $plan.Count) { throw 'Package allowlist count differs from the reviewed recipe.' }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    [long]$total = 0
    foreach ($file in $manifest.files) {
        Keys-Exactly $file @('path', 'bytes', 'sha256')
        Assert-Relative $file.path
        if (-not $seen.Add($file.path) -or -not $plan.ContainsKey($file.path) -or -not (Integer $file.bytes $maxFileBytes) -or $file.sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Manifest file record refused.' }
        $path = Regular $directory $file.path
        if ((Digest $path $file.bytes) -cne $file.sha256) { throw "Package content changed: $($file.path)" }
        $expected = $plan[$file.path].expected
        if ($null -ne $expected -and ($expected.bytes -ne $file.bytes -or $expected.sha256 -cne $file.sha256)) { throw "Package file differs from its pin: $($file.path)" }
        $total += $file.bytes
        if ($total -gt $maxTotalBytes) { throw 'Package byte budget exceeded.' }
    }
    if ($total -ne $manifest.totalBytes -or $total -ne $receipt.totalBytes) { throw 'Package byte accounting mismatch.' }
    if ((Digest (Regular $directory 'resources/sidecar/source-build/provenance.lock.json')) -cne $receipt.sourcePinSha256 -or
        (Digest (Regular $directory 'resources/sidecar/runtime.lock.json')) -cne $receipt.runtimePinSha256) { throw 'Package provenance pins differ from the trusted local receipt.' }
    $actual = Files-Bounded $directory
    if ($actual.Count -ne $seen.Count + 1) { throw 'Package has missing or additional files.' }
    foreach ($relative in $actual) { if ($relative -cne 'package-manifest.json' -and -not $seen.Contains($relative)) { throw 'Unlisted package file refused.' } }
    foreach ($name in @('shinatama-desktop.exe', 'shinatama-terminal-helper.exe', 'shinatama-session-helper.exe', 'shinatama-hook.exe')) { Assert-Native (Regular $directory $name) ($name -ceq 'shinatama-desktop.exe') }
    return [ordered]@{ status = 'verified'; directory = $directory; packageId = $id; files = $seen.Count; bytes = $total; gitHead = $manifest.gitHead; buildMode = $manifest.buildMode; nativeAppExecuted = $false; releaseApproved = $false }
}

if ($Verify) {
    if (-not $Package -or -not [IO.Path]::IsPathRooted($Package)) { throw '-Verify requires -Package with the recorded absolute stage directory.' }
    Verify-Package $Package | ConvertTo-Json -Depth 8
    return
}
if ($Package) { throw 'Create always reserves a fresh UUID stage; -Package is accepted only with -Verify.' }
[void](Assert-NoLink $workspace)
# Existing offline validators: never call Prepare or execute Node/native programs.
$sourceVerification = ((& (Regular $workspace 'sidecar/prepare.ps1') -Mode Verify) -join "`n") | ConvertFrom-Json -AsHashtable
$runtimeVerification = ((& (Regular $workspace 'sidecar/prepare-runtime.ps1') -Mode Verify) -join "`n") | ConvertFrom-Json -AsHashtable
if ($sourceVerification.mode -cne 'verify' -or $runtimeVerification.mode -cne 'verify' -or -not $runtimeVerification.licenseBundleComplete) { throw 'Static sidecar/runtime input verification failed.' }
$resources = Join-Path $workspace 'sidecar'
$plugins = Join-Path $workspace 'resources/agent-plugins'
$pluginsPinSha256 = Digest (Regular $plugins 'plugins.lock.json')
$plan = Plan $resources $plugins
$preparedFiles = Files-Bounded (Join-Path $resources 'source-build/prepared')
if ($preparedFiles.Count -ne 610) { throw 'Prepared sidecar must contain the exact 609 pinned files plus its generated identity.' }
$expectedPrepared = Expected-Prepared (Read-Json (Regular $resources 'source-build/provenance.lock.json'))
foreach ($relative in $preparedFiles) { if (-not $expectedPrepared.ContainsKey($relative)) { throw 'Unexpected prepared sidecar file.' } }
$gitHead = Git-Head
foreach ($name in @('shinatama-desktop.exe', 'shinatama-terminal-helper.exe', 'shinatama-session-helper.exe', 'shinatama-hook.exe')) { Assert-Native (Regular $workspace "target/debug/$name") ($name -ceq 'shinatama-desktop.exe') }
foreach ($directory in @((Join-Path $workspace '.verification'), $portableRoot)) {
    if (Test-Path -LiteralPath $directory) { [void](Assert-NoLink $directory) }
    else { [void](Assert-NoLink ([IO.Path]::GetDirectoryName($directory))); [void][IO.Directory]::CreateDirectory($directory); [void](Assert-NoLink $directory) }
}
$packageId = [Guid]::NewGuid().ToString('D')
$stage = Join-Path $portableRoot $packageId
if (Test-Path -LiteralPath $stage) { throw 'The new package UUID is already occupied.' }
[void][IO.Directory]::CreateDirectory($stage)
$records = [Collections.Generic.List[object]]::new()
[long]$totalBytes = 0
foreach ($relative in @($plan.Keys | Sort-Object -CaseSensitive)) {
    $target = Under $stage $relative
    $parent = [IO.Path]::GetDirectoryName($target)
    if ($parent -cne $stage) { Ensure-Directory $stage ([IO.Path]::GetRelativePath($stage, $parent).Replace('\', '/')) }
    if ($relative -ceq 'Shinatama.cmd') { Write-New $target $launcherBytes; $bytes = $launcherBytes.LongLength; $sha = Digest $target $bytes }
    else {
        $source = if ($relative.StartsWith('resources/sidecar/', [StringComparison]::Ordinal)) { Regular $resources $plan[$relative].source }
            elseif ($relative.EndsWith('.exe', [StringComparison]::Ordinal)) { Regular $workspace "target/debug/$relative" }
            else { Regular $workspace $plan[$relative].source }
        $sourceStream = [IO.File]::Open($source, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        try {
            $bytes = $sourceStream.Length
            if ($bytes -gt $maxFileBytes -or $totalBytes + $bytes -gt $maxTotalBytes) { throw 'Package input byte budget exceeded.' }
            $sha = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($sourceStream)).ToLowerInvariant()
            $expected = $plan[$relative].expected
            if ($null -ne $expected -and ($expected.bytes -ne $bytes -or $expected.sha256 -cne $sha)) { throw "Source file differs from its pin: $relative" }
            $sourceStream.Position = 0
            $targetStream = [IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
            try { $sourceStream.CopyTo($targetStream, 65536); $targetStream.Flush($true) } finally { $targetStream.Dispose() }
        } finally { $sourceStream.Dispose() }
        if ((Digest $target $bytes) -cne $sha) { throw 'Package source/output hash mismatch; incomplete stage preserved.' }
    }
    $totalBytes += $bytes
    if ($records.Count -ge $maxFiles -or $totalBytes -gt $maxTotalBytes) { throw 'Package cumulative budget exceeded.' }
    $records.Add([ordered]@{ path = $relative; bytes = [long]$bytes; sha256 = $sha })
}
$manifest = [ordered]@{ schemaVersion = 1; product = 'Shinatama'; packageId = $packageId; buildMode = 'existing-target-debug'; gitHead = $gitHead
    binaryBuildRevisionVerified = $false; createdUtc = [DateTimeOffset]::UtcNow.ToString('O'); nativeAppExecuted = $false; releaseApproved = $false
    scope = 'local debug portable files; public release/X3 gates remain open'; totalBytes = $totalBytes; files = $records.ToArray() }
$manifestPath = Join-Path $stage 'package-manifest.json'
Write-New $manifestPath ($utf8.GetBytes(($manifest | ConvertTo-Json -Depth 8) + "`n"))
$receipt = [ordered]@{ schemaVersion = 2; packageId = $packageId; gitHead = $gitHead; fileCount = $records.Count; totalBytes = $totalBytes
    manifestSha256 = (Digest $manifestPath); sourcePinSha256 = (Digest (Regular $resources 'source-build/provenance.lock.json'))
    runtimePinSha256 = (Digest (Regular $resources 'runtime.lock.json')); agentPluginsPinSha256 = $pluginsPinSha256 }
# Independent workspace receipt prevents a package from self-asserting changed
# hashes by rewriting its own manifest. Keep it beside the stage for -Verify.
Write-New (Join-Path $portableRoot "$packageId.receipt.json") ($utf8.GetBytes(($receipt | ConvertTo-Json -Depth 8) + "`n"))
Verify-Package $stage | ConvertTo-Json -Depth 8
