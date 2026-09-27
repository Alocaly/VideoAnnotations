# Step 3: static annotations

## Scope and design

The preview supports text, rectangle, ellipse, and arrow annotations. Shape tools
use drag gestures; text uses a click followed by editing in a multiline field.
Selection works on the video or through a list, including covered annotations.
Dragging moves a selection; corner handles resize boxes, and endpoint handles
reshape arrows. Color/opacity, line thickness, and text font size are editable.
Delete removes the selection. Esc cancels a draft or restores the original
geometry of an active move/resize. Degenerate gestures are rejected.

Geometry uses the display-oriented video dimensions, not preview pixels. The
image rectangle accounts for letterboxing; coordinates, line widths, and font
sizes scale with it. Movement is bounded by video edges. Text wraps to its box
and is clipped there. Selection handles retain a usable screen-space size.
Editing gestures pause playback. Annotation overlays do not alter the source.

The GUI-independent model is in `annotations.rs`; `editor.rs` owns interaction
and egui painting. The preview painter is not yet an export renderer: step 6
must share these rendering rules with the export path. Drawing order currently
follows insertion order (last on top). Start/end fields are initialized to the
full video duration; timing controls and filtering arrive in step 4.

Annotations are in-memory only. A visible warning explains the limitation, and
opening another video or closing the window requires discard confirmation.
Project persistence and undo/redo remain step 5 work.

## Validation (Windows, 2026-09-27)

- Offline check, formatting, and Clippy with warnings denied pass.
- Nine unit tests pass, including geometry-aware hit testing, movement bounds,
  letterbox/resize coordinate mapping, topmost selection, deletion, cancellation
  of create/move/resize, and handle/minimum-size rules.
- Native GUI smoke test: created all four annotation types; moved and resized a
  rectangle; edited text and observed wrapping; left fullscreen and verified
  that the overlays remained aligned with the video.

The GUI smoke test is not exhaustive: color-picker interactions, all extreme
window sizes, and discard-confirmation paths still merit broader manual testing.
No GUI deletion was performed; deletion behavior is covered by unit tests.
