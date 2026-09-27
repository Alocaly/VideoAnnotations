# Step 8: reliability and Windows portable preview

## Changes

- Executable-local runtime discovery takes priority over the working directory;
  explicit environment overrides still take priority. Shared resolution has an
  isolated filesystem test, including Unicode paths and Cargo's test layout.
- Fixed rotated source handling. libmpv's `dwidth`/`dheight` include pixel aspect
  scaling but not display rotation, and its software renderer does not transform
  the pixels. The player disables automatic rotation and explicitly rotates the
  RGB buffer using decoder metadata for 90/180/270 degrees. Project geometry uses
  the rotated dimensions; normal frames keep a linear-copy path. Other angles
  report an error rather than silently exporting inconsistent geometry.
- Reopening a file clears loaded/error/EOF state but retains observed-property
  caches: mpv does not resend a property that has the same value in the next file.
- Added an offline-capable validation script, a release ZIP builder, an isolated
  package/checksum test, and a console runtime diagnostic. No UI redesign.

Reference: [mpv properties](https://mpv.io/manual/master/#properties) and
[software renderer implementation](https://github.com/mpv-player/mpv/blob/master/video/out/libmpv_sw.c).

## Validation matrix

Generated fixtures (no private video checked into Git) cover:

| Case | Source / expectation |
| --- | --- |
| Portrait | 1080×1920, 30 fps |
| UHD | 3840×2160, 24 fps |
| Fractional rate | 30000/1001 fps |
| High rate | 120 fps |
| Odd dimensions | 321×181, MP4 padded to 322×182 |
| Anamorphic | 320×240 at SAR 4:3, mpv display 426×240 |
| Rotation metadata | 90°, 180°, 270°; preview RGB samples compared against FFmpeg |
| Long duration | One-hour 160×90/1-fps fixture, seek near end, complete static export |

Each case loads through libmpv, seeks, exports an annotated MP4, checks output
dimensions/duration and a red annotation pixel. Rotated cases also reopen an
unrotated source in the same player to detect stale orientation. A long animated
export is rejected at the existing frame limit. Existing tests cover VFR frame
timestamps, audio muxing, playback clock stability, saving, failed inputs,
cancellation, and destination preservation.

A one-second 1280×720 test exports a moving, fading, glowing ellipse with a
traveling outline, checking monotonic progress. On this development machine it
took about 0.9 seconds in release versus 19 seconds in debug. This is a small
synthetic benchmark, not a guaranteed export speed or sustained playback claim.
The one-hour fixture checks timeline extent and seeking, not an hour-long soak
or high-bitrate 4K playback. Preview remains software-decoded and memory use is
bounded by existing preview/cache/temporary-frame limits, not newly profiled
across machines.

Run `scripts/validate.ps1 -Offline -Media` (or add `-Release`). The default suite
does not play physical audio. The real-device test remains an explicit opt-in.
The release validation passed 33 unit tests and 12 explicitly enabled media/UI
tests, plus formatting and Clippy with warnings denied. The native packaged app
was launched from the isolated test directory and its rotated portrait fixture
was visually checked in the editor and video-only fullscreen.

## Packaging boundary

`scripts/package-windows.ps1` builds for x86_64-pc-windows-msvc and creates a
timestamped ZIP under ignored `dist/`. It includes the app, runtime checker,
playback diagnostic, documentation, Cargo lockfile, dependency inventory and
available crate/font notices, per-file SHA-256, and source revision/dirty status.
It does not include videos, projects, credentials, FFmpeg, or libmpv binaries.
The provided setup scripts fetch the media dependencies separately. Setup needs
Internet and 7-Zip; normal use does not need Rust, Internet, or admin access.

`scripts/test-package.ps1` extracts outside the repository, checks hashes, copies
the already-installed media runtimes into this test copy only, and starts the
console checker from an unrelated working directory. It verifies all three
resolved paths are package-local and that explicit missing overrides fail with
exit code 1. Test copies are retained for inspection. This does not re-test the
upstream download service or substitute for a clean Windows-machine deployment.

Remaining release work: code signing, an installer if desired, selecting a
project license, and a distribution/licensing review before publishing a bundled
third-party-runtime release. HDR, arbitrary rotation angles, GPU decoding, crash
recovery, long-running soak tests, and hardware-specific performance remain out
of this portable-preview milestone. The requested ergonomics pass remains separate.
