# VideoAnnotations

A Rust desktop application for annotating a single video per project, currently at the technical-preview stage.

## Planned features

- Video playback with audio, fullscreen, and keyboard/button navigation by one frame, one second, or five seconds in either direction.
- Text, rectangles, ellipses, and arrows, with editable appearance and placement.
- A timeline controlling annotation start and end times and stacking order.
- Annotation animations, including fades, movement, and glow.
- Save and reopen annotation projects.
- Export annotated videos, including MP4 and GIF.
- An English-language interface.

## Status

Steps 1 and 2 are implemented. The English-language native player opens a video,
plays it with synchronized audio, pauses, seeks, steps one frame in either
direction, and switches to fullscreen. It includes a position/duration display,
volume and mute, and replay at the end. Files open paused through the picker,
drag-and-drop, or a command-line argument.

The initial target is Windows. The UI uses egui/eframe; persistent playback uses
libmpv, with FFmpeg internally. The step-1 FFmpeg/ffprobe extraction backend is
retained separately. Native file dialogs follow the OS language.

See [ROADMAP.md](ROADMAP.md) for implementation milestones and [Agent.md](Agent.md) for project requirements.

## Development

Install stable Rust through rustup and Visual Studio Build Tools with the C++
desktop workload on Windows. Commands below run from the repository root.

Install the tested libmpv build locally (requires 7-Zip installed or `7z` on PATH):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-mpv.ps1
```

This pins the Windows x64 build dated 2026-09-27, checks its SHA-256, and preserves
the package's headers and license files under `tools/mpv/`. It does not change
PATH. Alternatively set `VIDEO_ANNOTATIONS_MPV` to the full path to a compatible
`libmpv-2.dll`. The app searches `tools/mpv/` from its working directory and from
the executable's directory and parent directories.

For frame extraction and integration-test fixtures, also install FFmpeg locally
(download provider linked by [FFmpeg](https://ffmpeg.org/download.html)):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-ffmpeg.ps1
```

Alternatively, place FFmpeg 8 or newer and ffprobe on PATH, or set
`VIDEO_ANNOTATIONS_FFMPEG` and `VIDEO_ANNOTATIONS_FFPROBE` to their full executable
paths. The app also discovers `tools/ffmpeg/bin/` from the working directory or
the executable's directory and its two parents. Installation does not alter PATH.

Run:

```powershell
cargo run
# Or open a video immediately:
cargo run -- "Movies_in/MindBlowing.mp4"
```

Build a release executable with `cargo build --release`. Keep libmpv available
using one of the locations above; this is not yet a standalone distribution.

Verify:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
# Requires FFmpeg; generates its own temporary test video:
cargo test --test media_integration -- --ignored
# Playback integration tests with clocked null audio:
cargo test --test playback_integration -- --ignored --skip system_audio_device
# Optional real audio device check (plays a quiet test tone briefly):
cargo test --test playback_integration system_audio_device -- --ignored --nocapture
# Diagnose a local file without the GUI (clocked null audio output):
cargo run --example playback_probe -- "Movies_in/MindBlowing.mp4"
```

Local input videos belong in `Movies_in/`; that directory, downloaded FFmpeg/libmpv,
build output, and generated media are excluded from Git. `Cargo.lock` is tracked.

## Prototype limitations

Preview rendering currently uses libmpv's software renderer, capped at 1280 x 720,
then uploads the result to an egui texture. Hardware video decoding and HDR color
management are not yet validated. Backward frame stepping may be slower on long
GOP videos. The position follows the playback engine; step commands follow actual
frames, including tested variable-frame-rate media. No annotations, saving, or
export are available yet.

## Player controls

| Action | Shortcut |
| --- | --- |
| Open video | Ctrl+O |
| Play / pause / replay at end | Space |
| Back / forward one second | Left / Right |
| Back / forward five seconds | Shift+Left / Shift+Right |
| Previous / next frame (pauses playback) | Ctrl+Left / Ctrl+Right; also comma / period |
| Fullscreen | F11 or double-click the preview |
| Exit fullscreen | Esc |
| Mute | M |

The same navigation actions have buttons. The time slider seeks on release and
preserves the play/pause state. Keyboard shortcuts do not interfere with typing
in numeric fields. Opening the file picker pauses the current video.

See [architecture decisions](docs/architecture.md) and the
[step 1 validation report](docs/step-1-validation.md), and
[step 2 playback design and validation](docs/step-2-playback.md).
