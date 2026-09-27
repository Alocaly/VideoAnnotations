# Step 6: annotated MP4 export

## Design

Export MP4 snapshots the current project, including unsaved annotations, and
pauses playback. A background worker renders static transparent PNG overlays,
then runs FFmpeg to composite them in back-to-front order during their [start,end)
intervals. Frame presentation timestamps, not average FPS, determine visibility.
The preview's special EOF selection state does not create an extra encoded frame.

`render::paint` is shared by preview and export: geometry, clipping, text layout,
bundled egui fonts, stroke widths, arrowheads, and colors use video coordinates.
The export rasterizer tessellates egui shapes at source display resolution and
samples the font atlas, using premultiplied blending and straight-alpha PNG output.
GPU/CPU antialiasing, preview scaling, and lossy video compression can still cause
minor pixel differences; this is not a pixel-identical screenshot exporter.

FFmpeg autorotates the input, normalizes display dimensions and square pixels,
composites overlays, and pads odd dimensions to even sizes. Output is H.264
CRF 18 / medium, YUV 4:2:0, with the first optional audio track encoded to AAC
192 kb/s. Variable frame timing is retained. Other streams and metadata are omitted.

UI progress separates preparation (0–10%) from encoding (10–99%); 100% means the
temporary sibling MP4 has been flushed and successfully committed to the selected
destination. Cancellation is checked during rasterization and encoding. The worker
terminates and reaps FFmpeg, drains its pipes, and cleans temporary files on failure
or cancellation. A child-process guard also handles worker panics. A 120-second
no-progress watchdog bounds stalled encoding. Source overwrite is rejected.
Editing/opening and normal window closure are blocked during the job; Cancel export
remains available. Existing output is replaced only after success.

## Validation (Windows, 2026-09-27)

- `cargo fmt --all -- --check`, `cargo build --offline`, and
  `cargo clippy --offline --all-targets -- -D warnings` pass.
- 24 unit tests pass, including offline rendering of all four annotation types,
  transparency, text clipping, and cancellation.
- Three explicit FFmpeg export integration tests pass: decoded pixel checks for
  timing and layer order; H.264/AAC streams; silent input; Unicode/spaced paths;
  unchanged source; VFR timestamps/frame count; preparation/encoding cancellation;
  corrupt input; protected destination and temporary-file cleanup.
- The media integration test and three null-audio playback integration tests pass.
  The optional physical audio-device test was not rerun in this step.
- Native UI smoke test opened the saved annotated project and exported a five-second
  1296×720 MP4. UI completion, H.264/AAC durations, and a decoded annotated frame
  were checked. The visual check exposed a full-width progress bar hiding the
  cancel control; its width was bounded and a second native export confirmed the
  cancel control remains visible during preparation.

## Current limits

Static annotations only; MP4 only; at most 32 annotations and 9 million display
pixels. Full-frame overlays consume memory and preparation time. No HDR tone
mapping, audio-track selection, quality controls, subtitles, or GIF yet. The source
must remain available and unchanged while exporting. File identity is path-based,
not a content hash. Abrupt process/OS termination is outside normal cancellation
handling. Rotated/anamorphic media, unusual starting timestamps, long videos, and
large projects require broader validation in step 8. Effects and GIF follow in step 7.
