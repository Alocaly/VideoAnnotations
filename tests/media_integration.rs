//! Run explicitly with FFmpeg installed: cargo test --test media_integration -- --ignored
use std::{path::Path, process::Command};
use video_annotations::media::MediaBackend;

fn ffmpeg() -> std::path::PathBuf {
    std::env::var_os("VIDEO_ANNOTATIONS_FFMPEG")
        .map(Into::into)
        .unwrap_or_else(|| {
            let local = Path::new("tools/ffmpeg/bin/ffmpeg.exe");
            if local.is_file() {
                local.into()
            } else {
                "ffmpeg".into()
            }
        })
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn open_seek_and_reject_invalid_media() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sample with spaces é.mp4");
    let output = Command::new(ffmpeg())
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25:duration=2",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(&path)
        .output()
        .expect("FFmpeg must be installed");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let backend = MediaBackend::default();
    let info = backend.probe(&path).unwrap();
    assert_eq!((info.width, info.height), (320, 180));
    assert!((info.duration - 2.0).abs() < 0.1);
    assert_eq!(info.fps, Some(25.0));
    let first = backend.frame(&info, 0.0).unwrap();
    let middle = backend.frame(&info, 1.0).unwrap();
    assert_eq!(first.rgba.len(), first.width * first.height * 4);
    assert_ne!(
        first.rgba, middle.rgba,
        "Seeking must change the decoded image"
    );
    assert!(
        backend.frame(&info, info.duration + 5.0).is_ok(),
        "EOF seeks are clamped"
    );
    assert!(backend.probe(&temp.path().join("missing.mp4")).is_err());
    let invalid = temp.path().join("invalid.mp4");
    std::fs::write(&invalid, b"not a video").unwrap();
    assert!(backend.probe(&invalid).is_err());
}
