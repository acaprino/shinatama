[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# CI only: a fresh checkout has no sidecar/runtime and the lock's sourcePath is the owner's installed Node.
# Downloads the official archive the lock names, refuses it unless its SHA256 is the pinned one, takes
# node.exe from it and lets prepare-runtime.ps1 check and attest it. Does nothing when sidecar/runtime exists.
$pin = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'runtime.lock.json') | ConvertFrom-Json
$license = $pin.license
$destination = Join-Path $PSScriptRoot 'runtime'
if (Test-Path -LiteralPath $destination) { Write-Host 'sidecar/runtime exists; nothing to fetch.' }
else {
    if ($license.archiveUrl -cnotmatch '^https://nodejs\.org/dist/v[0-9.]+/node-v[0-9.]+-win-x64\.zip$') { throw 'Unexpected Node archive URL.' }
    $work = Join-Path ([IO.Path]::GetTempPath()) ('node-runtime-' + [Guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($work)
    try {
        $zip = Join-Path $work 'node.zip'
        Invoke-WebRequest -Uri $license.archiveUrl -OutFile $zip -MaximumRedirection 0
        if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant() -cne $license.archiveSha256) { throw 'Node archive SHA256 mismatch.' }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archive = [IO.Compression.ZipFile]::OpenRead($zip)
        try {
            $entry = $archive.GetEntry('node-v' + $pin.fileVersion + '-win-x64/node.exe')
            if ($null -eq $entry -or $entry.Length -ne $pin.bytes) { throw 'node.exe is missing from the archive or has another size.' }
            $extracted = Join-Path $work 'node.exe'
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $extracted)
        } finally { $archive.Dispose() }
        if ((Get-FileHash -LiteralPath $extracted -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pin.sha256) { throw 'node.exe SHA256 mismatch.' }
        [void][IO.Directory]::CreateDirectory($destination)
        [IO.File]::Copy($extracted, (Join-Path $destination 'node.exe'), $false)
    } finally { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}
& (Join-Path $PSScriptRoot 'prepare-runtime.ps1') -Mode Prepare
