# Playback and editing fixes (2026-09-27)

## Causes and changes

- The seek slider used egui's default `SliderClamping::Always` together with three
  fixed decimals. This rounded the external playback clock on idle frames and
  marked the widget changed. The UI then requested an exact seek on almost every
  frame, causing interrupted audio and the persistent seeking indicator. Using
  `SliderClamping::Edits` preserves the clock until the user actually edits it.
  User dragging still commits on release; pointer/keyboard editing remains available.
- The annotation text field relied on an automatic widget ID, which could change
  when playback status widgets were added or removed. It now has an explicit ID
  per selected annotation, a visible Text content label, and pauses when focused.
- F11 previously only changed the native window state. Fullscreen now renders a
  black, margin-free, video-only panel, preserving the aspect ratio and annotations.
  Space, seek shortcuts, and mute remain available; F11/Esc/double-click exit.
- Playback previously called the editing canvas and drew selection bounds and
  resize handles. Viewing now paints only actual annotations; selection remains
  available when returning to paused editing. Fullscreen hides handles even paused.

## Validation

- 28 regular unit tests pass, with new headless egui tests for untouched clock
  updates, pointer-driven seeking, text focus across changing status widgets, and
  fullscreen viewing without editor handles or document changes.
- An explicit FFmpeg/libmpv/egui regression test plays a generated 24 fps video
  with audio for two seconds while updating the actual seek widget. No UI seek is
  emitted; the video clock stays within 250 ms of wall time and A/V offset below
  150 ms.
- All four playback integration tests pass, including the physical WASAPI output
  check, and all three MP4 export integration tests pass.
- Formatting, offline build, and Clippy with warnings denied pass.
- Native Windows UI checks: F11 video-only display and Esc return; editing text
  with accents/spaces; an ellipse selected while paused, then playing without its
  blue bounding rectangle/handles or a recurring seeking spinner. The local test
  project was saved separately; no user project was overwritten.

This corrects the automatic-seek bug rather than changing decoder buffering or
playback speed. Long/high-resolution/HDR media still require the broader performance
validation planned for step 8.
