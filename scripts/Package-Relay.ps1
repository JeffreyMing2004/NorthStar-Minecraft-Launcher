param([switch]$IncludePrivateCatalog, [switch]$PoolOnly)

$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskOutput = Join-Path $taskRoot 'target/relay-packages'
$taskSuffix = if ($IncludePrivateCatalog) { '-private' } else { '' }
$taskPrefix = if ($PoolOnly) { 'nscl-frpc-pool' } else { 'nscl-relay-1panel' }
$taskName = $taskPrefix + $taskSuffix + '-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 6)
$taskPackage = Join-Path $taskOutput $taskName
$taskTemplate = Join-Path $taskRoot 'docs/relay/1panel'
$taskCatalog = Join-Path $taskRoot 'relay-data/frpc-nodes.json'
if ($IncludePrivateCatalog -and !(Test-Path -LiteralPath $taskCatalog -PathType Leaf)) {
    throw 'Private catalog is missing: relay-data/frpc-nodes.json'
}
if ($PoolOnly -and !$IncludePrivateCatalog) {
    throw '-PoolOnly requires -IncludePrivateCatalog; this package contains provider credentials.'
}
if ($PoolOnly) {
    New-Item -ItemType Directory -Path $taskPackage -Force | Out-Null
    Copy-Item -LiteralPath $taskCatalog -Destination (Join-Path $taskPackage 'frpc-nodes.json')
    Copy-Item -LiteralPath (Join-Path $taskTemplate 'tunnel-pool.md') -Destination (Join-Path $taskPackage 'README.md')
} else {
    New-Item -ItemType Directory -Path (Join-Path $taskPackage 'source'), (Join-Path $taskPackage 'relay-data') -Force | Out-Null
    foreach ($taskFile in @('Dockerfile', 'docker-compose.yml', '.dockerignore', 'openresty-location.conf', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $taskTemplate $taskFile) -Destination $taskPackage
    }
    foreach ($taskFile in @('Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $taskRoot $taskFile) -Destination (Join-Path $taskPackage 'source')
    }
    foreach ($taskFolder in @('src', 'assets')) {
        Copy-Item -LiteralPath (Join-Path $taskRoot $taskFolder) -Destination (Join-Path $taskPackage 'source') -Recurse
    }
    [IO.File]::WriteAllText((Join-Path $taskPackage 'relay-data/members.json'), "{`"members`": []}`n", [Text.UTF8Encoding]::new($false))
    if ($IncludePrivateCatalog) {
        Copy-Item -LiteralPath $taskCatalog -Destination (Join-Path $taskPackage 'relay-data/frpc-nodes.json')
    } else {
        Copy-Item -LiteralPath (Join-Path $taskTemplate 'frpc-nodes.example.json') -Destination (Join-Path $taskPackage 'relay-data')
    }
}
$taskArchive = $taskPackage + '.zip'
# CreateFromDirectory includes .dockerignore on both Windows PowerShell and PowerShell 7.
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($taskPackage, $taskArchive)
$taskHash = (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText(($taskArchive + '.sha256'), "$taskHash  $taskName.zip`n", [Text.UTF8Encoding]::new($false))
Write-Output $taskArchive
Write-Output ('SHA256: ' + $taskHash)
if ($IncludePrivateCatalog) { Write-Output 'This private package includes provider credentials; upload only to your own server.' }
