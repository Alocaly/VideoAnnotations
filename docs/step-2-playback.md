# Step 2: persistent video player

## Design

`src/playback.rs` owns a persistent libmpv core. Its FFmpeg-backed decoding and
audio-driven clock replace per-image subprocesses during playback. Time seeks
are exact seeks; frame stepping uses the decoder's next/previous frame commands,
not average FPS arithmetic. The UI observes asynchronous position, duration,
pause, EOF, seek, display-size, and audio-output properties.

The safe Rust wrapper encapsulates a small C API surface, dynamically loaded
through `libloading`. It checks major client ABI version 2. Handles remain on the
creating thread. All runtime commands are asynchronous. Render and event
callbacks only flag work and wake the UI; they cannot unwind into C. Shutdown
removes callbacks and frees the renderer before terminating the core and
unloading the library. A new core for each opened video prevents old-file events
or sound from leaking into a replacement project.

The software render API writes RGB pixels to an aligned buffer, capped at
1280 x 720, then egui uploads a texture. This keeps the preview ready for Rust
annotation overlays without native child-window stacking problems. Rotation and
display aspect ratio come from mpv. CPU rendering is a deliberate first player
implementation; a GPU render backend is a future performance improvement.
Demuxer cache budgets are 64 MiB forward and 16 MiB backward; the entire decoded
video is never retained in application memory.

The slider applies its seek on release. A paused seek stays paused. Frame
stepping pauses and suppresses step audio. At EOF the last image is retained and
Replay starts again at zero. Volume and mute survive opening another file.

## Verification

On Windows x64, Rust 1.98.1, with the pinned libmpv build dated 2026-09-27:

- Automated playback with a generated H.264/AAC fixture: startup paused,
  position progression, stable pause, accurate time seek, forward/backward frame
  steps, EOF retention, delayed replay, volume/mute commands.
- Silent variable-frame-rate fixture: verifies 80 ms steps in one segment and
  40 ms steps in another, plus backward navigation.
- Missing/corrupt media and creating a fresh player after an error.
- Real Windows WASAPI output initialized and stayed synchronized before/after a
  seek; measured engine-reported A/V offset approximately -3 ms before the seek
  in the validation run (test threshold 150 ms). This checks engine timing and
  device initialization, not an acoustic measurement of speaker output.
- Native UI exercised with a generated MP4: rendering, audio-output status,
  frame navigation, time navigation, playback, fullscreen and exit.
- Local sample also checked through the 1280 x 720 playback diagnostic: audio
  and video advanced through all five seconds and remained paused at EOF.

Run commands are in the README. Fixtures, DLLs, downloaded archives and local
sample videos are excluded from Git. The system-device test is explicitly
ignored by default because it requires an audio device and emits a short tone.

## Remaining limits

No annotations, project persistence, or export yet. Hardware decoding, HDR,
very high resolution/high frame-rate performance, and broad codec/device
coverage remain to be validated. Long-GOP backward steps can take longer.
Distribution must account for libmpv/FFmpeg licenses and package dependencies.

## References

- [libmpv client API](https://github.com/mpv-player/mpv/blob/master/include/mpv/client.h)
- [Rendering and threading contracts](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h)
- [Playback commands](https://mpv.io/manual/stable/#playback-control)
- [Pinned Windows build](https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20260927)
