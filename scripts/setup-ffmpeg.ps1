$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$downloadDir = Join-Path $projectRoot 'tools/downloads'
$installDir = Join-Path $projectRoot 'tools/ffmpeg'
New-Item -ItemType Directory -Force -Path $downloadDir, $installDir | Out-Null
$archive = Join-Path $downloadDir 'ffmpeg-release-essentials.zip'
$url = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
Write-Host 'Downloading the Windows build linked from ffmpeg.org...'
Invoke-WebRequest $url -OutFile $archive
$checksum = (Invoke-RestMethod "$url.sha256").Trim().Split(' ')[0]
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $checksum) {
    throw 'FFmpeg archive checksum mismatch.'
}
$unpackDir = Join-Path $downloadDir ([guid]::NewGuid().ToString())
Expand-Archive -LiteralPath $archive -DestinationPath $unpackDir
$package = Get-ChildItem -LiteralPath $unpackDir -Directory | Select-Object -First 1
if (-not (Test-Path (Join-Path $package.FullName 'bin/ffmpeg.exe'))) { throw 'Unexpected archive structure.' }
Copy-Item -Path (Join-Path $package.FullName '*') -Destination $installDir -Recurse -Force
& (Join-Path $installDir 'bin/ffmpeg.exe') -version | Select-Object -First 1
if ($LASTEXITCODE -ne 0) { throw 'FFmpeg installation verification failed.' }
& (Join-Path $installDir 'bin/ffprobe.exe') -version | Select-Object -First 1
Write-Host "FFmpeg installed locally in $installDir. No PATH changes were made."
