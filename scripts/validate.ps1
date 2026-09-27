param([switch]$Offline, [switch]$Media, [switch]$Release)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
    $options = @('--locked')
    if ($Offline) { $options += '--offline' }
    if ($Release) { $options += '--release' }
    & cargo clippy @options --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    & cargo test @options
    if ($LASTEXITCODE -ne 0) { throw 'Unit tests failed.' }
    if ($Media) {
        & cargo run @options --example runtime_check
        if ($LASTEXITCODE -ne 0) { throw 'Runtime check failed.' }
        # Never plays physical audio; opt into that test separately.
        & cargo test @options -- --ignored --skip system_audio_device --nocapture
        if ($LASTEXITCODE -ne 0) { throw 'Media integration tests failed.' }
    }
} finally { Pop-Location }
