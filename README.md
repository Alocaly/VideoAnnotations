# VideoAnnotations

A planned Rust desktop application for annotating a single video per project.

## Planned features

- Video playback with audio, fullscreen, and keyboard/button navigation by one frame, one second, or five seconds in either direction.
- Text, rectangles, ellipses, and arrows, with editable appearance and placement.
- A timeline controlling annotation start and end times and stacking order.
- Annotation animations, including fades, movement, and glow.
- Save and reopen annotation projects.
- Export annotated videos, including MP4 and GIF.
- An English-language interface.

## Status

Project planning and repository setup. Application implementation has not started.
The initial target is Windows. GUI and video-decoding choices will be validated in the first prototype; FFmpeg may be used.

See [ROADMAP.md](ROADMAP.md) for implementation milestones and [Agent.md](Agent.md) for project requirements.

## Development

Install the stable Rust toolchain through rustup. There is no Cargo package yet.
Local input videos belong in `Movies_in/`; that directory and generated media are intentionally excluded from Git.
