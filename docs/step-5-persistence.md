# Step 5: project persistence and history

## File format and validation

`.vannot` is UTF-8 JSON with a top-level `version: 1` and `project` object. The
project includes one source video's path/display dimensions/duration/FPS, plus
the ordered annotation array. All text, geometry, RGBA colors, stroke/font sizes,
and start/end times round-trip. UI selection, playback position, volume, and undo
history are deliberately not persisted. No media is embedded or changed.

Source paths under the project directory are stored relative to that directory;
other sources retain absolute paths. Relative paths resolve from the opened file,
not the process working directory. Share absolute-path files with care.

Unknown versions/fields, invalid JSON, non-finite or out-of-range geometry/style/
timing, and invalid video metadata are rejected. Limits are 8 MiB per project,
2000 annotations, and 64 KiB per annotation text. The UI may exceed these limits
while editing, but saving then reports an error without replacing the old file.

Save validates and serializes before writing to a temporary file beside the
destination. It flushes that file and uses tempfile's persist operation to replace
the destination. Failure leaves the project dirty. The source video cannot be the
save destination. Save As uses the exact path confirmed by the native dialog and
requires the .vannot extension. Concurrent edits by other applications are not
detected; autosave, file locking, and crash recovery are not implemented.

## Loading and relinking

Open project, dropped .vannot files, and command-line project arguments share the
same path. A missing source opens a locate dialog; cancel preserves the current
project. The candidate source loads in a separate libmpv core while the old project
and player remain intact. Only a successful load replaces them. It times out
after 20 seconds if required video metadata never becomes available.

For saved projects, display dimensions must match exactly and duration within
50 ms. This catches obvious wrong sources but is not an identity check: there is
no content hash. Users must select the original video. A changed source reference
marks the loaded project dirty so Save can retain the relocation. The stored
annotation coordinate system and timing metadata are retained unchanged.

New projects start unsaved, including empty ones. A saved snapshot is compared to
the current project; undoing back to that state clears the dirty marker. Opening
another file or closing asks Save and continue / Discard changes / Cancel. Failed
or canceled saving does not continue the destructive operation.

## Undo/redo

History captures complete annotation states, up to 100 undo snapshots. Continuous
mouse input and focused text editing are grouped before committing a history
entry. Canceling a geometry gesture restores the original without adding an entry.
New edits discard redo states. Undo/redo clears selection and pauses playback.
Text fields retain their own keyboard undo; toolbar buttons access project history.
Source relinking, playback, save/open operations, and GUI state are not undoable.
History resets on successful load and is session-only. Full snapshots may use
significant memory for large text-heavy projects; byte-budgeting is future work.

## Validation (Windows, 2026-09-27)

- Build, formatting, and Clippy with warnings denied pass.
- 22 unit tests pass. Added coverage includes all-shape/style/Unicode round-trip,
  overwrite, relocated relative paths, missing source references, malformed files,
  future versions, invalid geometry, oversized input, preservation on failed save,
  source-overwrite protection, history grouping/cancellation/redo invalidation,
  history limits, dirty tracking across undo, and preserving the current project
  after invalid opening or a failed save before close.
- Four non-hardware integration tests pass (FFmpeg plus libmpv playback tests).
  The optional physical audio-device test was not rerun.
- Native computer-use smoke test: create and move rectangle, Ctrl+Z/Ctrl+Y restore
  its two positions, save through the native dialog, inspect the JSON, reopen it
  through Open project, and verify matching geometry and the saved marker.

Missing-source dialog selection and every close/cancel/error combination were not
exhaustively exercised through the GUI. No local media or smoke-test project is
included in the public repository.
