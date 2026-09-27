# Step 1: architecture decision

## GUI: egui / eframe

Use eframe with its OpenGL renderer for the native prototype. Immediate-mode UI
fits a timeline and interactive shapes, and video frames can be uploaded as
textures. No browser runtime is required. The initial target is Windows/MSVC.

## Media: FFmpeg and ffprobe subprocesses

ffprobe supplies stream dimensions, average frame rate, and duration. FFmpeg
performs an accurate input seek and decodes one frame into a PNG pipe. Frames are
scaled to fit 1280 x 720, corrected for sample aspect ratio, and automatically
rotated by FFmpeg. Original metadata remains separate from preview dimensions.

This avoids native library linkage and establishes a usable decoding baseline.
Starting a process and encoding a PNG per seek has overhead: this is a seeking
prototype, not the final playback engine. Step 2 must introduce persistent frame
decoding, presentation timestamps, bounded buffering, and audio synchronization.
Average FPS is descriptive and cannot implement exact stepping for variable-rate
video. The displayed time is explicitly the requested position, not an extracted
frame timestamp. HDR color-managed preview and exact end-frame seeking for VFR
are not validated in this milestone.

## Module boundaries

- `src/ui.rs`: window, file picker, timeline scrubber, texture upload, UI state.
- `src/media.rs`: probing, decoding, errors, and a background worker; no GUI types.
- `src/project.rs`: exactly one source video and its annotation collection.
- `src/annotations.rs`: initial video-coordinate shape data; no editing yet.
- `src/export.rs`: reserved boundary for step 6; no export implementation yet.

The worker serializes decoding and discards queued obsolete requests before the
next decode. Each result carries a request ID; the UI ignores obsolete results,
including those for a previously opened file. Both process pipes are drained
concurrently and operations time out after 30 seconds. Missing executables,
invalid media, missing duration, and failed decodes become English UI errors.

The annotation renderer will be shared between preview and export. UI pixels must
never be persisted as annotation geometry. Playback, project serialization, and
export codecs are deliberately deferred to their roadmap milestones.

## Dependencies and distribution

FFmpeg is a separate executable. The local setup script downloads a Windows build
from gyan.dev (linked by the FFmpeg project) and verifies the published SHA-256.
It preserves that package's license files. Binaries and downloaded media are not
committed. FFmpeg redistribution and codec licensing must be addressed when
packaging the application in step 8.

## References

- [eframe application API](https://docs.rs/eframe/0.36.2/eframe/trait.App.html)
- [FFmpeg seek semantics](https://ffmpeg.org/ffmpeg.html)
- [FFmpeg download providers](https://ffmpeg.org/download.html)
