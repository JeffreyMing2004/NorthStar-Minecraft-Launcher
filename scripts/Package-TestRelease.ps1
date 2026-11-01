param([string]$Tag = '')

$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskManifest = Get-Content -LiteralPath (Join-Path $taskRoot 'Cargo.toml') -Raw -Encoding UTF8
$taskVersion = [regex]::Match($taskManifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+-[0-9A-Za-z.-]+)"').Groups[1].Value
if (!$taskVersion) { throw 'Test packages require a prerelease version, for example 0.1.0-beta.1.' }
if ($Tag -and $Tag -cne "v$taskVersion") { throw 'Release tag does not match Cargo.toml.' }

Push-Location -LiteralPath $taskRoot
try {
    # Always build the current source; never package a stale preview executable.
    cargo build --locked --bin nscl
    if ($LASTEXITCODE) { throw 'Debug build failed.' }
    $taskBinary = Join-Path $taskRoot 'target/debug/nscl.exe'
    $taskBytes = [IO.File]::ReadAllBytes($taskBinary)
    $taskPeOffset = [BitConverter]::ToInt32($taskBytes, 60)
    if ([BitConverter]::ToUInt32($taskBytes, $taskPeOffset) -ne 0x4550 -or [BitConverter]::ToUInt16($taskBytes, $taskPeOffset + 4) -ne 0x8664) {
        throw 'Expected a Windows x86_64 executable.'
    }

    $taskOutput = Join-Path $taskRoot 'target/test-release'
    $taskStage = Join-Path $taskOutput ('portable-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path (Join-Path $taskStage 'docs') -Force | Out-Null
    Copy-Item -LiteralPath $taskBinary -Destination (Join-Path $taskStage 'NSCL.exe')
    Copy-Item -LiteralPath (Join-Path $taskRoot 'LICENSE') -Destination $taskStage
    Copy-Item -LiteralPath (Join-Path $taskRoot "docs/releases/$taskVersion.md") -Destination (Join-Path $taskStage 'RELEASE-NOTES.md')
    Copy-Item -LiteralPath (Join-Path $taskRoot 'docs/test-release-readme.md') -Destination (Join-Path $taskStage 'README.md')
    foreach ($taskDocument in @('user-guide.md', 'troubleshooting.md', 'instance-backups.md', 'launcher-updates.md', 'multiplayer.md')) {
        Copy-Item -LiteralPath (Join-Path $taskRoot "docs/$taskDocument") -Destination (Join-Path $taskStage "docs/$taskDocument")
    }
    $taskArchiveName = "nscl-v$taskVersion-windows-x86_64-debug.zip"
    $taskArchive = Join-Path $taskOutput $taskArchiveName
    if (Test-Path -LiteralPath $taskArchive) { throw "Archive already exists: $taskArchive" }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($taskStage, $taskArchive)
    $taskHash = (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $taskOutput 'SHA256SUMS'), "$taskHash  $taskArchiveName`n", [Text.UTF8Encoding]::new($false))
    Write-Output $taskArchive
    Write-Output "SHA256: $taskHash"
} finally {
    Pop-Location
}
