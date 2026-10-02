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

Steps 1 through 8 are implemented (Windows portable preview, not a signed installer).
See [PORTABLE.md](PORTABLE.md) for installation, diagnostics, and shortcuts.
The English-language native player opens a video,
plays it with synchronized audio, pauses, seeks, steps one frame in either
direction, and switches to fullscreen. It includes a position/duration display,
volume and mute, and replay at the end. Files open paused through the picker,
drag-and-drop, or a command-line argument.

Add text, rectangles, ellipses, and arrows directly on the preview. Select, move,
resize, recolor, and delete them using the annotation tools. Geometry and style
sizes use video coordinates, preserving placement when the window changes size.
The annotation timeline controls start/end times, seeking, creation at the
playhead, and stacking order.
Projects can be saved and reopened as `.vannot` files, with source-video relinking
when the original path is missing. Annotation edits support undo and redo.
Export MP4 burns the annotations into a new H.264 video with AAC source audio,
progress reporting, and cancellation.
Annotations support fades, linear movement, glow, and a traveling outline.
GIF export includes maximum-width and frame-rate controls.

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
the package's headers under `tools/mpv/`. It does not change
PATH. Alternatively set `VIDEO_ANNOTATIONS_MPV` to the full path to a compatible
`libmpv-2.dll`. The app searches `tools/mpv/` beside the executable and its
development ancestors before the working directory.

For MP4 export, frame extraction, and integration-test fixtures, also install FFmpeg locally
(download provider linked by [FFmpeg](https://ffmpeg.org/download.html)):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-ffmpeg.ps1
```

Alternatively, place FFmpeg 8 or newer and ffprobe on PATH, or set
`VIDEO_ANNOTATIONS_FFMPEG` and `VIDEO_ANNOTATIONS_FFPROBE` to their full executable
paths. The app discovers `tools/ffmpeg/bin/` beside the executable and its
development ancestors before the working directory. Installation does not alter PATH.

Run:

```powershell
cargo run
# Or open a video immediately:
cargo run -- "Movies_in/MindBlowing.mp4"
```

Build a release executable with `cargo build --release`. Use the optimized release
build for normal use: CPU animation export is much slower in debug builds.
Create a Windows x64 portable ZIP (media dependencies downloaded separately):

```powershell
./scripts/package-windows.ps1 -Offline
# Validate the ZIP from an isolated temporary location using installed runtimes:
./scripts/test-package.ps1 -Archive "dist/VideoAnnotations-<version>-windows-x64-<timestamp>.zip"
```

The script includes release executables, documentation, dependency notices,
build information, and checksums. It does not publish a GitHub release or include
local media/projects. No signing certificate, installer, or runtime redistribution
bundle is configured. See [step 8 validation](docs/step-8-reliability.md).

Verify:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
# Entire local suite including synthetic media, without physical audio:
./scripts/validate.ps1 -Offline -Media
# Same checks with optimized binaries:
./scripts/validate.ps1 -Offline -Media -Release
# Requires FFmpeg; generates its own temporary test video:
cargo test --test media_integration -- --ignored
# Export integration tests (timing, layers, audio, VFR, cancellation and failure):
cargo test --test export_integration -- --ignored
# Playback integration tests with clocked null audio:
cargo test --test playback_integration -- --ignored --skip system_audio_device
# Real playback clock with the UI seek slider (guards against automatic seeking):
cargo test --bin video-annotations live_playback_with_seek_slider -- --ignored
# Optional real audio device check (plays a quiet test tone briefly):
cargo test --test playback_integration system_audio_device -- --ignored --nocapture
# Diagnose a local file without the GUI (clocked null audio output):
cargo run --example playback_probe -- "Movies_in/MindBlowing.mp4"
# Runtime paths, tool versions, and libmpv software-renderer availability:
cargo run --example runtime_check
```

Local input videos belong in `Movies_in/`; that directory, downloaded FFmpeg/libmpv,
build output, and generated media are excluded from Git. `Cargo.lock` is tracked.

## Prototype limitations

Preview rendering currently uses libmpv's software renderer, capped at 1280 x 720,
then uploads the result to an egui texture. Hardware video decoding and HDR color
management are not yet validated. Backward frame stepping may be slower on long
GOP videos. The position follows the playback engine; step commands follow actual
frames, including tested variable-frame-rate media. Export currently supports
SDR MP4 and GIF, up to 32 annotations and 9 megapixels; HDR tone mapping is not implemented.
Animated export uses temporary PNG frames (maximum 18000 frames and 2 GiB).
Display rotations of 90/180/270 degrees are applied to preview pixels and project
dimensions; other angles are unsupported. Older projects saved with a quarter-turn
source may contain the old, incorrect dimensions and be rejected as mismatched:
retain a backup and recreate that project's annotations from the source video.
Projects reference the source video rather than embedding it; keep the video
alongside your project. There is no autosave or crash recovery yet. Opening
another video/project or closing the window offers Save, Discard, and Cancel
when there are unsaved changes.

## Player controls

| Action | Shortcut |
| --- | --- |
| Open video | Ctrl+O |
| Open project | Ctrl+Shift+O |
| Save project / Save as | Ctrl+S / Ctrl+Shift+S |
| Undo / Redo annotation edit | Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z |
| Show / hide video information | Ctrl+I |
| Play / pause / replay at end | Space |
| Back / forward one second | Left / Right |
| Back / forward five seconds | Shift+Left / Shift+Right |
| Previous / next frame (pauses playback) | Ctrl+Left / Ctrl+Right; also comma / period |
| Video-only fullscreen | F11 or double-click the preview with no active annotation selection/tool |
| Exit fullscreen | Esc |
| Mute | M |

The same navigation actions have buttons. Dragging the time slider seeks and
updates the video continuously, then leaves playback paused at the chosen frame.
The top bar groups opening and saving under **File**, MP4 and GIF exports under
**Export**, and fullscreen/video information under **Misc**. Undo/Redo remain
buttons. It shows the project and source video
filenames after VideoAnnotations; an asterisk beside the project name means
unsaved changes. Choosing **Export GIF** opens a settings dialog before the
native save dialog.
Resolution, frame rate, and audio-output status appear in the upper-right corner
of the video preview; **Misc → Show video information** or Ctrl+I toggles them.
The full shortcut list is available from **Misc → Keyboard shortcuts…** instead
of occupying space below the player.
Keyboard shortcuts do not interfere with typing
in numeric fields. Opening the file picker pauses the current video.
Fullscreen hides the editor panels and timeline, fitting the video to the screen
without cropping. Space, seeking keys, and mute still work. F11, Esc, or a
double-click returns to the editor.

## Annotation controls

Choose a shape tool and drag on the video; choose Text and click to add a text
box. Use Select to move an annotation and its blue handles to resize it. Arrows
have two endpoint handles. Text-box resizing changes wrapping/clipping; Font size
changes the lettering. The selection menu also reaches covered annotations.
Color (including opacity), thickness, text content, font size, and effects are
editable in the Properties panel to the right of the video. Delete removes the
selection; Esc cancels an unfinished gesture. Delete and player shortcuts do
not interfere with text entry.
To edit text, select its annotation (on the preview, in the timeline, or in the
selection menu), then edit the **Text content** field in Properties.
Focusing that field pauses playback. Selection bounds and resize handles appear
only when paused in the editor, never during playback or video-only fullscreen.

New annotations are drawn on top and start at the playhead, lasting five seconds
or until the video ends. The timeline's Add buttons create a default shape that
can then be positioned on the preview. Near EOF, creation is bounded to retain
at least a 1 ms interval (or the full duration for shorter videos).

Select a timeline row to seek to its start, or click/drag the ruler or a track to
seek to that time and pause. Edit Start/End in seconds or use the playhead buttons.
The timeline stays at the bottom of the window; its track list uses only the
height it needs (up to its scrolling limit), leaving the rest for the video.
Intervals include their start but exclude their end, except at the video endpoint.
Hidden annotations remain selectable in the timeline, not on the preview.
Bring forward / Send backward move the selected annotation one layer at a time;
the top timeline row is the front layer. Track dragging seeks; it does not trim
or shift intervals. The whole video fits the ruler (no timeline zoom yet).

## Project files

Use Save to create a `.vannot` JSON file containing the video reference, geometry,
text, colors, sizes, timing, and layer order. Open it through Open project,
drag-and-drop, or `cargo run -- "Projects/example.vannot"`. The video itself is
never modified. Store local projects in `Projects/` (ignored by Git), or beside
the source video to make its saved path relative. Sources outside the project
folder retain absolute paths, which may reveal local folder names when shared.

If a source is missing, choose the original video in the locate dialog. Dimensions
and duration must match the project; save again to retain the new reference.
This checks metadata, not video identity. A failed or canceled opening leaves the
current project intact. Save writes through a temporary sibling file before
replacing the destination. An asterisk marks unsaved changes.

Undo/redo covers annotation creation/deletion, geometry, style, text, timing, and
layer order, retaining at most 100 full annotation snapshots for the current
session. Mouse gestures are grouped; text edits group until focus leaves the
field. While typing, Ctrl+Z/Y belongs to the text field; use the Undo/Redo buttons
for project history. Playback, selection, file operations, and source relinking
are not undoable, and history is not stored in the project file.

Projects now save as format version 2, including effects. Version 1 projects still
open with effects disabled. Older app versions cannot open newly saved version 2
projects; keep a copy if you need to use an older executable.

## Annotation effects

Select an annotation, then expand **Effects** below its style controls:

- **Fade in / Fade out**: seconds from the start / before the end. Zero disables
  that fade. Overlapping fades use the lower opacity, so short annotations may
  never become fully opaque.
- **Move by X / Y**: total linear displacement in video pixels over the annotation
  lifetime. The stored box is the starting position; movement beyond the video
  is clipped. Seeking evaluates the same effect as continuous playback.
- **Glow color pulse**: smoothly shifts the annotation color toward a lighter shade,
  or toward a darker shade when its color is already very light. The 2-second
  cycle returns to the original color; intensity 0–30, with zero disabling it.
  It does not add a halo or change the annotation's transparency/size.
- **Traveling outline** (rectangles/ellipses): a bright quarter-contour segment
  over a dim outline, with seconds per revolution (0.1–60).

Effects compose and are included in save, undo/redo, fullscreen, MP4, and GIF.
Older version-2 projects still open, but their saved Glow value now controls this
color animation instead of the previous static halo. Keep a copy if that old
appearance matters.
An annotation with fade-in is invisible at its exact start; move the playhead
forward to see it. Its selection handles remain available when paused.

## Annotated MP4 export

Choose Export MP4, then a destination different from the source video. The current
annotation state is exported, including unsaved edits, without saving the project.
Playback pauses and editing is disabled during the background job. Use Cancel
export to stop it; wait for completion or cancellation before closing the app.
The destination is replaced only after successful encoding; failure or cancellation
leaves an existing destination intact. The source video is never overwritten.

The output uses H.264 (CRF 18), 8-bit YUV 4:2:0, and the first source audio track
re-encoded to AAC at 192 kb/s when present. Silent videos remain silent. Annotation
timing and layer order match the preview. Video display dimensions are preserved,
with at most one padding pixel on the right/bottom for even MP4 dimensions. Source
frame timestamps are retained without forcing a constant frame rate. Subtitles,
additional audio tracks, and source metadata are not copied. Animated overlays are
sampled at 60 fps while the source video's frame timestamps remain variable.
MP4 quality/size controls and packaging remain future work.

## Animated GIF export

Choose **Export GIF**, set **Max width** (64–1920 px, no upscaling) and
**Frame rate** (1–30 fps) in the dialog, then continue to the native save
dialog. Height follows the source aspect ratio. GIF has no audio
and loops forever. It uses a global palette built in a separate pass and dithering;
GIF color and centisecond timing limits can cause banding or rounded durations.
Effects are sampled at the selected GIF frame rate. Export progress, cancellation,
source protection, and safe destination replacement also apply to GIF.

See [architecture decisions](docs/architecture.md) and the
[step 1 validation report](docs/step-1-validation.md), and
[step 2 playback design and validation](docs/step-2-playback.md), and
[step 3 annotation design and validation](docs/step-3-annotations.md), and
[step 4 timeline design and validation](docs/step-4-timeline.md), and
[step 5 persistence design and validation](docs/step-5-persistence.md), and
[step 6 export design and validation](docs/step-6-export.md), and
[step 7 effects and GIF design and validation](docs/step-7-effects-gif.md).

See also [step 8 reliability and portable packaging](docs/step-8-reliability.md).
