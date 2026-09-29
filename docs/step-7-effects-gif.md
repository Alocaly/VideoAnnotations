# Step 7: effects and GIF

## Model and renderer

`Annotation::effects` stores fade durations, a linear displacement, Glow intensity,
and traveling-outline period. Defaults disable all effects. `evaluated(time)`
returns transformed geometry/color without mutating the project. Opacity is the
minimum of the fade-in/out ramps multiplied by the base alpha; overlapping fades
do not implicitly change their durations. Movement is linear over the active
interval and clipped at the video edges. Validation rejects nonfinite/unbounded
settings. Document version 2 persists effects; version 1 remains readable.

`render::paint_at` is shared by paused editing, playback, fullscreen, and export.
Glow now animates the annotation color over a smooth two-second cycle, with no
halo or changes to opacity. Darker colors shift toward white; already very light
colors shift toward black. Intensity remains 0–30 in the persisted `glow` field.
This replaced the original step-7 layered-halo rendering; existing version-2
projects retain their values but render differently. The outline effect remains
a bright traveling quarter-contour over a
dim contour, with a configurable revolution period. Rectangles use perimeter
distance; ellipses use angle. Outline is offered only for rectangles/ellipses.
Picking, selection bounds, and resize input account for the animated displacement.
History stores effect edits with all other annotation state.

## Export

Static projects retain the existing one-PNG-per-annotation path. A nonzero Glow
is now time-varying. If any time-varying effect is enabled, the CPU renderer builds a single
transparent, composited scene for each overlay sample, preserving layer order.
Lossless PNGs are written to a temporary directory: 60 samples/second for MP4,
the selected frame rate for GIF. FFmpeg composites this stream onto source frames.
MP4 retains variable source timestamps and optional AAC audio; overlay sampling
can introduce up to one sample of timing quantization versus the preview.

GIF uses an aspect-preserving downscale, 1–30 fps, a two-pass global palette,
Sierra dithering, no audio, and infinite looping. Separate palette generation
avoids holding the entire decoded video in memory and reduces palette flicker.
Source fps conversion happens before overlay, avoiding a lost final-frame duration
at overlay EOF. Final-frame padding and duration trimming bound the output length.
GIF's centisecond delays quantize rates such as 15/30 fps.

Preparation has cancellable per-frame/per-row work and uses up to 50% of progress
for animation; encoding uses the remainder. Temporary frames are capped at 18000
and 2 GiB, alongside the existing 32-annotation / 9-megapixel limits. These caps
bound disk use, not total runtime; large animated exports are CPU-intensive and
the GIF overlays are still rendered at source resolution. Cancellation/failure
cleans temporary files and leaves existing destinations unchanged. No source
video or local test fixture is added to Git.

## Validation

- 32 regular unit tests, five export integration tests, three null-audio playback
  tests, and the live UI seek-slider regression pass. Formatting, offline build,
  and Clippy with warnings denied pass.
- Unit tests cover deterministic effect evaluation, overlapping fades, unchanged
  stored geometry, version-1 migration/version-2 roundtrip, invalid effects, empty
  transparent frames, movement, fade alpha, color pulse, and changing outline phase.
- FFmpeg integration verifies animated MP4 pixels at several times, GIF codec,
  dimensions, frame count/rate, absence of audio, Unicode paths, cancellation
  during animation preparation, and protected destinations. Existing static MP4,
  layer/timing, VFR, failure, and cancellation tests remain passing.
- The real-time UI seek-slider regression and null-audio playback tests pass.
- The original native UI validation used a five-second project with moving/fading text
  and an ellipse with a traveling outline, including seek/fullscreen and the
  Effects panel. GIF export is exercised from the native save dialog.
  The resulting GIF was decoded and visually checked: 640×356, 75 frames over
  five seconds, with the then-current text halo and traveling ellipse correctly
  composited. Subsequent tests check color pulsing in the shared renderer and
  exported MP4/GIF frames.

This milestone does not add keyframes, easing curves, arbitrary motion paths,
audio effects, or additional formats beyond MP4/GIF. The dedicated ergonomics pass
and broader performance/distribution work remain separate.
