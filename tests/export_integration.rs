//! Explicitly run with local FFmpeg: cargo test --test export_integration -- --ignored
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};
use video_annotations::{
    annotations::{Annotation, Kind},
    export,
    media::MediaBackend,
    project::Project,
};

fn tool(name: &str) -> PathBuf {
    std::env::var_os(format!("VIDEO_ANNOTATIONS_{}", name.to_uppercase()))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let local = PathBuf::from(format!("tools/ffmpeg/bin/{name}.exe"));
            if local.is_file() { local } else { name.into() }
        })
}
fn fixture(path: &Path, audio: bool) -> Project {
    let mut cmd = Command::new(tool("ffmpeg"));
    cmd.args([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=black:size=160x100:rate=10:duration=2",
    ]);
    if audio {
        cmd.args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=2",
            "-c:a",
            "aac",
        ]);
    }
    let result = cmd
        .args(["-c:v", "libx264", "-y"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Project::new(MediaBackend::default().probe(path).unwrap())
}
fn pixel(path: &Path, time: f64, x: usize, y: usize) -> [u8; 3] {
    let output = Command::new(tool("ffmpeg"))
        .args(["-v", "error", "-ss", &time.to_string(), "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let offset = (y * 160 + x) * 3;
    output.stdout[offset..offset + 3].try_into().unwrap()
}
#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn exports_timing_layering_audio_and_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source é.mp4");
    let mut project = fixture(&source, true);
    let original = std::fs::read(&source).unwrap();
    let mut a = Annotation::new(Kind::Rectangle, [20.0, 20.0], [100.0, 70.0], 2.0);
    a.color = [255, 0, 0, 255];
    a.thickness = 10.0;
    a.start_seconds = 0.5;
    a.end_seconds = 1.5;
    project.annotations.push(a.clone());
    a.color = [0, 0, 255, 255];
    a.start_seconds = 1.0;
    project.annotations.push(a);
    let out = dir.path().join("output é.mp4");
    let mut fractions = Vec::new();
    export::export(&project, &out, &AtomicBool::new(false), |f, _| {
        fractions.push(f)
    })
    .unwrap();
    assert_eq!(fractions.last(), Some(&1.0));
    assert!(pixel(&out, 0.2, 50, 24).iter().all(|v| *v < 15));
    let red = pixel(&out, 0.7, 50, 24);
    assert!(red[0] > 180 && red[2] < 40, "{red:?}");
    let blue = pixel(&out, 1.2, 50, 24);
    assert!(blue[2] > 180 && blue[0] < 40, "{blue:?}");
    assert!(pixel(&out, 1.6, 50, 24).iter().all(|v| *v < 15));
    let probe = Command::new(tool("ffprobe"))
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&out)
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = json["streams"].as_array().unwrap();
    assert!(streams.iter().any(|s| s["codec_name"] == "aac"));
    assert!(streams.iter().any(|s| s["codec_name"] == "h264"));
    assert_eq!(std::fs::read(source).unwrap(), original);
}
#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn silent_export_and_cancellation_preserve_destination() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("silent.mp4");
    let mut project = fixture(&source, false);
    let out = dir.path().join("out.mp4");
    export::export(&project, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let original = std::fs::read(&out).unwrap();
    let cancel = AtomicBool::new(false);
    project.annotations.push(Annotation::new(
        Kind::Text,
        [20.0, 20.0],
        [140.0, 90.0],
        2.0,
    ));
    assert!(
        export::export(&project, &out, &cancel, |_, _| cancel
            .store(true, Ordering::Relaxed))
        .is_err()
    );
    assert_eq!(std::fs::read(&out).unwrap(), original);
    let during_encode = AtomicBool::new(false);
    assert!(
        export::export(&project, &out, &during_encode, |_, message| {
            if message.starts_with("Encoding:") {
                during_encode.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err()
        .contains("canceled")
    );
    assert_eq!(std::fs::read(&out).unwrap(), original);
    assert!(export::export(&project, &source, &AtomicBool::new(false), |_, _| {}).is_err());
    assert!(!dir.path().read_dir().unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".video-annotations-")
    }));
}

fn timestamps(path: &Path) -> Vec<f64> {
    let output = Command::new(tool("ffprobe"))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    json["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            f["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn preserves_variable_timestamps_and_rejects_failed_encoding_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("vfr.mp4");
    let result = Command::new(tool("ffmpeg"))
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x100:rate=10:duration=3",
            "-vf",
            "select='lt(n,10)+gte(n,20)'",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-y",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(result.status.success());
    let mut project = Project::new(MediaBackend::default().probe(&source).unwrap());
    project.annotations.push(Annotation::new(
        Kind::Text,
        [20.0, 20.0],
        [140.0, 90.0],
        project.video.duration,
    ));
    let out = dir.path().join("vfr-out.mp4");
    export::export(&project, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let input_times = timestamps(&source);
    let output_times = timestamps(&out);
    assert_eq!(input_times.len(), output_times.len());
    for (a, b) in input_times.iter().zip(output_times) {
        assert!((a - b).abs() < 0.001, "{a} vs {b}");
    }
    let before = std::fs::read(&out).unwrap();
    let broken = dir.path().join("broken.mp4");
    std::fs::write(&broken, b"not media").unwrap();
    project.video.path = broken;
    assert!(
        export::export(&project, &out, &AtomicBool::new(false), |_, _| {})
            .unwrap_err()
            .contains("FFmpeg export failed")
    );
    assert_eq!(std::fs::read(out).unwrap(), before);
}
