param([switch]$Offline)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'This package targets Windows x64.'
}
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    $cargoArgs = @('--locked')
    if ($Offline) { $cargoArgs += '--offline' }
    & cargo build @cargoArgs --release --target x86_64-pc-windows-msvc --bin video-annotations --example runtime_check --example playback_probe
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $metadataText = & cargo metadata @cargoArgs --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Dependency inventory failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $package = $metadata.packages | Where-Object name -EQ 'video-annotations'
    $name = "VideoAnnotations-$($package.version)-windows-x64-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    $output = Join-Path $projectRoot "dist/$name"
    if (Test-Path -LiteralPath $output) { throw "Package already exists: $output" }
    New-Item -ItemType Directory -Path $output | Out-Null
    $release = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc/release'
    Copy-Item -LiteralPath (Join-Path $release 'video-annotations.exe') -Destination $output
    Copy-Item -LiteralPath (Join-Path $release 'examples/runtime_check.exe') -Destination $output
    Copy-Item -LiteralPath (Join-Path $release 'examples/playback_probe.exe') -Destination $output
    Copy-Item -LiteralPath README.md, PORTABLE.md, ROADMAP.md, Agent.md, Cargo.lock -Destination $output
    Copy-Item -LiteralPath docs -Destination $output -Recurse
    $scripts = New-Item -ItemType Directory -Path (Join-Path $output 'scripts')
    Copy-Item -LiteralPath scripts/setup-ffmpeg.ps1, scripts/setup-mpv.ps1 -Destination $scripts
    $notices = New-Item -ItemType Directory -Path (Join-Path $output 'third-party-notices')
    $inventory = foreach ($dependency in $metadata.packages | Sort-Object name, version) {
        if ($dependency.id -eq $package.id) { continue }
        $root = Split-Path -Parent $dependency.manifest_path
        $destination = Join-Path $notices.FullName "$($dependency.name)-$($dependency.version)"
        # Include nested font notices as well as crate-level licenses.
        $files = Get-ChildItem -LiteralPath $root -Recurse -File | Where-Object {
            $_.Name -match '(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|^OFL|^UFL|^README)' -or
            ($_.DirectoryName -match '[\\/]fonts([\\/]|$)' -and $_.Extension -eq '.txt')
        }
        foreach ($file in $files) {
            $relative = $file.FullName.Substring($root.Length).TrimStart('\', '/')
            $target = Join-Path $destination $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $target
            if ((Get-Item -LiteralPath $target).LastWriteTime.Year -lt 1980) {
                (Get-Item -LiteralPath $target).LastWriteTime = [datetime]'2000-01-01'
            }
        }
        [pscustomobject]@{ name = $dependency.name; version = $dependency.version; license = $dependency.license; repository = $dependency.repository; notices = @($files).Count }
    }
    $inventory | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $output 'dependencies.json')
    $commit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify source revision.' }
    $dirty = [bool](& git status --porcelain)
    [pscustomobject]@{ version = $package.version; commit = $commit; dirty = $dirty; target = 'x86_64-pc-windows-msvc'; rust = (& rustc --version); externalMediaBinariesIncluded = $false } |
        ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $output 'build-info.json')
    Get-ChildItem -LiteralPath $output -File -Recurse | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($output.Length + 1).Replace('\', '/')
        "$( (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() )  $relative"
    } | Set-Content -Encoding UTF8 (Join-Path $output 'SHA256SUMS.txt')
    $archive = "$output.zip"
    Compress-Archive -LiteralPath $output -DestinationPath $archive
    Get-FileHash -LiteralPath $archive -Algorithm SHA256
    Write-Host "Portable package: $archive"
    Write-Host 'FFmpeg/libmpv are NOT bundled. See PORTABLE.md for first-time setup.'
} finally { Pop-Location }
