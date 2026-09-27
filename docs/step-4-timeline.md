# Step 4: annotation timeline

Each annotation has a separate track beneath the player. The ruler spans the
whole source duration, and each track shows an interval bar and red playhead.
Tracks scroll vertically. Clicking a row label selects it and seeks to its
start; clicking or dragging a track or the ruler seeks and pauses playback.

The selected interval can be changed with Start/End numeric fields (seconds,
three decimal places) or the Start/End = playhead buttons. Go to start makes it
easy to find a hidden annotation. Values are clamped to video bounds and cannot
invert the interval; minimum length is 1 ms or the video's full duration if
shorter. Non-finite edits are ignored.

Both canvas tools and timeline Add buttons create at the current player time,
with a default five-second duration capped by EOF. Timeline creation supplies
default geometry editable on the preview. Creation at EOF moves the start back
by the minimum interval. This is time-based, not frame-snapped editing: very short
intervals may contain no presented frame, and seek presentation follows libmpv.

Visibility uses [start, end), with inclusion of the exact final video endpoint
when end equals the source duration. Painting, selection handles, and canvas hit
testing share this rule. Hidden annotations remain available in the timeline and
selection menu. Timing controls do not change geometry or style.

The annotation vector is ordered back to front. Bring forward / Send backward
swap adjacent entries and update the selection index. Timeline rows are shown
front to back so their top-to-bottom order matches layering. Overlapping intervals
are permitted. Selection remains attached to the same annotation after a swap.

## Validation (Windows, 2026-09-27)

- `cargo build --offline`, formatting, and Clippy with warnings denied pass.
- 13 unit tests pass, including interval boundaries, short videos, EOF creation,
  invalid timing edits, hidden-object hit testing, selection-preserving layer
  changes, and ruler coordinate mapping.
- All four non-hardware integration tests pass: one FFmpeg test and three libmpv
  tests, including variable-rate stepping and playback/seek/replay regression.
  The optional physical audio-device test was not rerun in this step.
- Native GUI checked with computer-use: ruler seek to 1.25 s, creation there,
  setting end to 2.50 s, disappearance at that endpoint, Go to start restoring
  visibility, overlapping ellipse creation, Send backward preserving selection,
  and layout in fullscreen and the normal window.

## Remaining limitations

No track trimming handles, interval drag-to-move, zoom, snapping, multi-selection,
or animation yet. Use numeric fields for precise timing. The preview follows
the player's reported time rather than a separate export clock. Persistence and
undo/redo remain step 5; all annotations are temporary until then. The native
window minimum is now 900 x 760 logical pixels to fit editing controls.
