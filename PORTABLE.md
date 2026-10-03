# VideoAnnotations — Windows x64 portable preview

## First launch

1. Extract the entire ZIP to a writable folder (not Program Files). Do not run
   the executable inside the ZIP. Windows x64 is required; ARM64 is not tested.
2. Install 7-Zip if it is not already available; the libmpv setup script needs
   `7z.exe` on PATH or in `C:\Program Files\7-Zip`.
3. Open PowerShell in the extracted folder and run:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-ffmpeg.ps1
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-mpv.ps1
   .\runtime_check.exe
   ```

   Review the scripts before running them. They download from the upstream build
   providers, verify SHA-256, and install inside `tools/` without changing PATH
   or requiring administrator access. Internet access is needed for setup only.
   The mpv build is pinned; FFmpeg uses the provider's current release archive.
   Record the versions printed by `runtime_check.exe` when reporting an issue.
4. Double-click `video-annotations.exe`. Rust and build tools are not required.
   If Windows or your antivirus blocks an unsigned executable, stop and review
   the warning with your administrator; this preview is not code-signed.

You can instead copy your existing, trusted runtime folders into `tools/ffmpeg`
and `tools/mpv`, or set `VIDEO_ANNOTATIONS_FFMPEG`, `VIDEO_ANNOTATIONS_FFPROBE`,
and `VIDEO_ANNOTATIONS_MPV` to absolute paths. Explicit overrides take priority,
then executable-local folders, development ancestor folders, the working folder,
and finally system tool/library lookup. FFmpeg 8+ and libmpv client API 2 are needed.

## Using the app

Open a video, add annotations, and save the editable project as `.vannot`.
Projects reference the original video; keep both together when moving them.
MP4 and GIF exports are separate rendered files. GIF has no audio.

| Shortcut | Action |
| --- | --- |
| Space | Play / pause |
| Left / Right | Back / forward 1 second |
| Shift + Left / Right | Back / forward 5 seconds |
| Ctrl + Left / Right | Previous / next frame |
| F11 | Video-only fullscreen |
| Ctrl + I | Show / hide video information |
| Esc | Exit fullscreen / cancel current gesture or selection |
| M | Mute |
| Delete | Delete selected annotation |
| Ctrl + S | Save project |
| Ctrl + O / Ctrl + Shift + O | Open video / project |
| Ctrl + Shift + S | Save project as |
| Ctrl + Z / Ctrl + Y | Undo / redo |
| Comma / Period | Previous / next frame |

Text fields capture typing shortcuts. Select a text annotation, then edit its
Text field in the Properties panel to the right of the video. Choose one effect
in Properties: None, Glow (second color and pulse speed), or Orbiting ball
(rectangles/ellipses/lines, ball color and turn duration). Set fades in the timeline.
With fade-in enabled, seek past the annotation's start
to see it. See README.md for full instructions and limitations.
Use the ✎ button beside the selected annotation's name in Properties to rename it.

## Diagnostics and limits

`runtime_check.exe` reports dependency paths/versions, checks libmpv's API and
software renderer with null audio, and returns 0 on success, 1 on failure. It
does not certify the physical audio device or every codec. Run it in PowerShell
so its output remains visible. To inspect a video without the editor:

```powershell
.\playback_probe.exe "C:\Videos\sample.mp4"
```

This preview decodes in software. Preview is capped at 1280×720; export at
9 megapixels and 32 annotations. Animated exports use temporary PNG files,
limited to 18000 frames and approximately 2 GiB; at 60 fps MP4 that means five
minutes. Large animated exports can be slow; use a lower GIF frame rate/size.
The temporary frames still use source resolution. Static long videos do not
use the animated-frame limit. HDR, hardware acceleration, crash recovery,
and an installer are not included. Save regularly.

The ZIP includes build information, a dependency inventory, third-party notices,
and SHA-256 checksums. These detect accidental changes, not publisher identity.
FFmpeg and libmpv binaries are downloaded separately, not redistributed in this
ZIP. Keep upstream notices with any downloaded runtimes. Do not assume this ZIP
is a ready-to-redistribute all-in-one bundle. Source:
https://github.com/Alocaly/VideoAnnotations

Updating: extract into a new folder, copy the trusted `tools` folders if desired,
then open your existing projects. Keep old projects backed up: saved version-5
projects require the current application. Legacy motion is removed; Glow takes
priority over a simultaneous old traveling outline. No registry changes or
file associations are made; removal is deletion of the extracted folder after
saving any projects or videos stored there.
