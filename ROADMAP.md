# Implementation roadmap

Scope: one video per project, a Rust application, and an entirely English interface.

1. **Architecture and technical prototype — completed** — Validated a native egui/eframe window and FFmpeg decoding on Windows. Open a video, display frames, and seek. UI, media, project, annotation data, and the future export boundary are separated. See [architecture](docs/architecture.md) and [validation](docs/step-1-validation.md).
2. **Video player** — Add playback/pause, audio synchronization, position/duration, fullscreen, and buttons/shortcuts for +/-1 second, +/-5 seconds, and +/-1 frame.
3. **Static annotations** — Create, select, move, resize, style, and delete text, rectangles, ellipses, and arrows. Store geometry in video coordinates so resizing the preview preserves placement.
4. **Annotation timeline** — Edit start/end times, seek, create annotations at the playhead, and manage overlapping annotations and stacking order.
5. **Project persistence** — Save and reopen the source-video reference and all annotation settings. Handle relocated source files and add undo/redo.
6. **First annotated export** — Export MP4 with source audio, progress reporting, and cancellation. Share rendering rules with the preview. This completes the first usable end-to-end milestone.
7. **Effects and additional formats** — Add fades, movement, glow, and an animated outline effect once its appearance is defined. Export GIF with size and frame-rate controls, then other selected formats.
8. **Reliability and distribution** — Verify different resolutions, orientations, frame rates, long videos, timing, performance, and error handling. Package for Windows, manage FFmpeg distribution if used, and document shortcuts.

Multi-clip editing, transitions between clips, and multiple video tracks are outside the initial scope.
