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

Step 1 is implemented: an English-language native window opens a video, displays
metadata and a frame preview, and seeks using a slider or +/-1 and +/-5 second
buttons. Files can be opened with the picker, drag-and-drop, or a command-line
argument. Decoding runs in the background.

The initial target is Windows. The prototype uses egui/eframe with an OpenGL
renderer and FFmpeg/ffprobe subprocesses. Playback and audio are not implemented
yet; they are the next milestone. Native file dialogs follow the OS language.

See [ROADMAP.md](ROADMAP.md) for implementation milestones and [Agent.md](Agent.md) for project requirements.

## Development

Install stable Rust through rustup and Visual Studio Build Tools with the C++
desktop workload on Windows. Commands below run from the repository root.

Install FFmpeg locally (download provider linked by [FFmpeg](https://ffmpeg.org/download.html)):

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

Build a release executable with `cargo build --release`. Keep FFmpeg available
using one of the locations above; this is not yet a standalone distribution.

Verify:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
# Requires FFmpeg; generates its own temporary test video:
cargo test --test media_integration -- --ignored
```

Local input videos belong in `Movies_in/`; that directory, downloaded FFmpeg,
build output, and generated media are excluded from Git. `Cargo.lock` is tracked.

## Prototype limitations

Seeking launches FFmpeg per requested image and is not a real-time playback
pipeline. Previews fit within 1280 x 720. Displayed times are seek requests, not
decoded frame timestamps; exact frame navigation, variable-frame-rate edge cases,
and synchronized audio belong to step 2. A finite video duration is required.
No annotations, saving, or export are available yet.

See [architecture decisions](docs/architecture.md) and the
[step 1 validation report](docs/step-1-validation.md).
