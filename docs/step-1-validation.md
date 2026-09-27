# Step 1 validation

Validated on Windows x64 with Rust/Cargo 1.98.1, eframe 0.36.2 and
FFmpeg/ffprobe 9.0.2 (gyan.dev essentials build).

## Automated checks

- `cargo check`: application and library compile.
- `cargo test`: metadata parser accepts fractional frame rates and duration
  fallback, clamps invalid/out-of-range seek values, and rejects invalid metadata
  and files without video streams.
- `cargo test --test media_integration -- --ignored`: generates a two-second
  video in a temporary path with spaces and a non-ASCII character. Probes its
  dimensions, duration, and frame rate; verifies that seeking changes decoded
  pixels; decodes a clamped end position; rejects missing and corrupt files.
- `cargo clippy --all-targets -- -D warnings`: no warnings.

## Native window check

Launched the built executable on Windows. Used the native file picker to open
the local sample video, verified its first frame and metadata (1296 x 720,
5 seconds, 24 fps), and clicked +1 second. The preview changed and the displayed
request advanced to 1.000 seconds. The busy indicator cleared after decoding.
The local sample and screenshots are not published in the repository.

## Scope of the result

This validates the initial window, decoding, file-picker path and seek pipeline.
It does not validate real-time playback, sound, every supported codec, HDR color
management, frame-exact VFR stepping, or packaged distribution. Those remain
future work as described in the roadmap and architecture document.
