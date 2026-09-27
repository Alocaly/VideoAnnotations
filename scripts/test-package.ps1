param([Parameter(Mandatory = $true)][string]$Archive)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$archivePath = (Resolve-Path -LiteralPath $Archive).Path
# Outside the checkout: ancestor discovery must not mask missing packaged files.
$testRoot = Join-Path ([IO.Path]::GetTempPath()) "VideoAnnotations-smoke-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $testRoot | Out-Null
Expand-Archive -LiteralPath $archivePath -DestinationPath $testRoot
$folders = @(Get-ChildItem -LiteralPath $testRoot -Directory)
if ($folders.Count -ne 1) { throw 'Expected one top-level package folder.' }
$packageRoot = $folders[0].FullName
foreach ($line in Get-Content -LiteralPath (Join-Path $packageRoot 'SHA256SUMS.txt')) {
    if ($line -notmatch '^([a-fA-F0-9]{64})  (.+)$') { throw "Invalid checksum line: $line" }
    $expected = $Matches[1]
    $file = [IO.Path]::GetFullPath((Join-Path $packageRoot $Matches[2]))
    if (-not $file.StartsWith($packageRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe checksum path.' }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $expected) { throw "Checksum mismatch: $file" }
}
if (Test-Path -LiteralPath (Join-Path $packageRoot 'tools')) { throw 'Archive unexpectedly bundles media runtimes.' }
New-Item -ItemType Directory -Path (Join-Path $packageRoot 'tools') | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'tools/ffmpeg'), (Join-Path $projectRoot 'tools/mpv') -Destination (Join-Path $packageRoot 'tools') -Recurse

function Invoke-RuntimeCheck([bool]$Missing) {
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = Join-Path $packageRoot 'runtime_check.exe'
    $start.WorkingDirectory = $testRoot
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($name in 'FFMPEG', 'FFPROBE', 'MPV') {
        $variable = "VIDEO_ANNOTATIONS_$name"
        $start.EnvironmentVariables.Remove($variable)
        if ($Missing) { $start.EnvironmentVariables[$variable] = Join-Path $testRoot "missing-$name.exe" }
    }
    $process = [Diagnostics.Process]::Start($start)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(30000)) {
        $process.Kill()
        $process.WaitForExit()
        throw 'Runtime diagnostic timed out.'
    }
    $report = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
    $code = $process.ExitCode
    $process.Dispose()
    Write-Host $report
    if ($Missing) {
        if ($code -ne 1 -or $report -notmatch 'FAILED') { throw 'Missing-runtime failure was not reported.' }
    } else {
        if ($code -ne 0) { throw 'Relocated runtime check failed.' }
        foreach ($relative in 'tools\ffmpeg\bin\ffmpeg.exe', 'tools\ffmpeg\bin\ffprobe.exe', 'tools\mpv\libmpv-2.dll') {
            if (-not $report.Replace('/', '\').Contains((Join-Path $packageRoot $relative))) { throw "Dependency did not resolve inside package: $relative" }
        }
    }
}
Invoke-RuntimeCheck $false
Invoke-RuntimeCheck $true
Write-Host "Package smoke test passed. Isolated copy retained for inspection: $packageRoot"
