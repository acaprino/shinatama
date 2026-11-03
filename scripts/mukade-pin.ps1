# Build preparation only. Never invoked at application/CLI startup. Exports the
# reviewed object, not the checkout/HEAD/config, into this workspace's ignored area.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Repository)
$ErrorActionPreference = 'Stop'
$revision = '4124b1f880f49a8f958801195324628752e35c4a'
$expectedTree = '759c2426a57dfd0491f60678e9788a84768900bb'
$origin = 'https://github.com/acaprino/mukade'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$repositoryPath = (Resolve-Path -LiteralPath $Repository).ProviderPath
$git = (Get-Command git.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$evidence = Join-Path $workspace '.verification'
if (Test-Path -LiteralPath $evidence) {
    if ((Get-Item -LiteralPath $evidence -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'The evidence directory must not be redirected.' }
} else { [void][IO.Directory]::CreateDirectory($evidence) }
if (-not [string]::Equals((Resolve-Path -LiteralPath $evidence).ProviderPath, $evidence, [StringComparison]::OrdinalIgnoreCase)) { throw 'The evidence directory is outside the workspace.' }
$target = Join-Path $evidence 'mukade-pin-4124b1f'
$archive = Join-Path $evidence 'mukade-pin-4124b1f.tar'
$manifestPath = Join-Path $evidence 'mukade-pin-4124b1f.manifest.json'
foreach ($path in @($target, $archive, $manifestPath)) {
    if (-not [IO.Path]::GetFullPath($path).StartsWith($evidence + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Export target escaped the workspace.' }
    if (Test-Path -LiteralPath $path) { throw 'The pinned export already exists. Preserve or inspect it explicitly; this script does not overwrite it.' }
}
$commit = & $git -C $repositoryPath rev-parse --verify ($revision + '^{commit}')
if ($LASTEXITCODE -ne 0 -or $commit -ne $revision) { throw 'The exact reviewed Mukade commit is unavailable.' }
$tree = & $git -C $repositoryPath rev-parse --verify ($revision + '^{tree}')
if ($LASTEXITCODE -ne 0 -or $tree -ne $expectedTree) { throw 'The reviewed Mukade tree identity did not match.' }
$entries = @(& $git -c core.quotepath=false -C $repositoryPath ls-tree -r $revision)
if ($LASTEXITCODE -ne 0 -or $entries.Count -gt 20000) { throw 'The source tree could not be read within its fixed export limit.' }
$names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$files = foreach ($entry in $entries) {
    if ($entry -notmatch '^(100644|100755) blob ([0-9a-f]{40})\t(.+)$') { throw 'Only ordinary source files are allowed: no submodules or symbolic links.' }
    $relative = $Matches[3]
    if ($relative.Contains('..') -or $relative.Contains(':') -or $relative.StartsWith('/') -or -not $names.Add($relative)) { throw 'A source name is unsafe or ambiguous on Windows.' }
    $absolute = [IO.Path]::GetFullPath((Join-Path $target $relative))
    if (-not $absolute.StartsWith($target + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'A source entry escaped the export directory.' }
    [PSCustomObject]@{ Relative = $relative; Absolute = $absolute; Blob = $Matches[2] }
}
& $git -C $repositoryPath archive --format=tar ('--output=' + $archive) $revision
if ($LASTEXITCODE -ne 0) { throw 'Exporting the reviewed commit failed.' }
if ((Get-Item -LiteralPath $archive).Length -gt 128MB) { throw 'The pinned archive exceeds the fixed source-preparation budget.' }
[void][IO.Directory]::CreateDirectory($target)
Add-Type -AssemblyName System.Formats.Tar
[System.Formats.Tar.TarFile]::ExtractToDirectory($archive, $target, $false)
$records = foreach ($file in $files) {
    $metadata = Get-Item -LiteralPath $file.Absolute -Force
    if ($metadata.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'A redirected source entry was refused.' }
    # Validate Git's content-addressed blob, without executing checkout filters.
    $hash = [Security.Cryptography.IncrementalHash]::CreateHash([Security.Cryptography.HashAlgorithmName]::SHA1)
    $stream = [IO.File]::OpenRead($file.Absolute)
    try {
        $hash.AppendData([Text.Encoding]::ASCII.GetBytes(('blob ' + $stream.Length + [char]0)))
        $buffer = [byte[]]::new(65536)
        while (($count = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) { $hash.AppendData($buffer, 0, $count) }
        $blob = [Convert]::ToHexString($hash.GetHashAndReset()).ToLowerInvariant()
    } finally { $stream.Dispose(); $hash.Dispose() }
    if ($blob -ne $file.Blob) { throw 'An exported source file did not match the reviewed Git object.' }
    $sha = (Get-FileHash -LiteralPath $file.Absolute -Algorithm SHA256).Hash.ToLowerInvariant()
    $file.Relative + "`t" + $sha
}
[Array]::Sort($records, [StringComparer]::Ordinal)
$text = [string]::Join("`n", $records)
$digest = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant()
$reviewed = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'mukade-pin.json') -Raw | ConvertFrom-Json
if ($reviewed.revision -ne $revision -or $reviewed.tree -ne $tree -or $reviewed.sourceFilesSha256 -ne $digest -or $reviewed.fileCount -ne $files.Count) { throw 'The exported source digest differs from the reviewed source manifest.' }
$manifest = [ordered]@{ schemaVersion = 1; repository = $origin; revision = $revision; tree = $tree;
    archiveSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant();
    cargoTomlSha256 = (Get-FileHash -LiteralPath (Join-Path $target 'Cargo.toml') -Algorithm SHA256).Hash.ToLowerInvariant();
    sourceFilesSha256 = $digest; fileCount = $files.Count }
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
$manifest | ConvertTo-Json
