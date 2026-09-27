$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$projectRoot = Split-Path -Parent $PSScriptRoot
$downloadDir = Join-Path $projectRoot 'tools/downloads'
$installDir = Join-Path $projectRoot 'tools/mpv'
$sevenZip = 'C:\Program Files\7-Zip\7z.exe'
if (-not (Test-Path $sevenZip)) {
    $sevenZip = (Get-Command 7z -ErrorAction Stop).Source
}
New-Item -ItemType Directory -Force -Path $downloadDir, $installDir | Out-Null
# Pin the tested build; use baseline x86_64 rather than requiring an AVX2 CPU.
$release = Invoke-RestMethod 'https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/tags/20260927'
$asset = $release.assets | Where-Object name -Match '^mpv-dev-x86_64-[0-9].*\.7z$' | Select-Object -First 1
if (-not $asset -or $asset.digest -notmatch '^sha256:([a-fA-F0-9]{64})$') {
    throw 'Release asset or SHA-256 digest is missing.'
}
$expected = $Matches[1]
$archive = Join-Path $downloadDir $asset.name
Invoke-WebRequest $asset.browser_download_url -OutFile $archive
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) {
    throw 'libmpv archive checksum mismatch.'
}
& $sevenZip x $archive "-o$installDir" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'libmpv extraction failed.' }
if (-not (Test-Path (Join-Path $installDir 'libmpv-2.dll'))) { throw 'libmpv DLL not found.' }
Write-Host "Installed $($asset.name) in $installDir (SHA-256 verified)."
