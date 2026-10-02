//! Explicitly run with local FFmpeg: cargo test --test export_integration -- --ignored
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};
use video_annotations::{
    annotations::{Annotation, Effect, Kind},
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
    pixel_with_width(path, time, x, y, 160)
}
fn pixel_with_width(path: &Path, time: f64, x: usize, y: usize, width: usize) -> [u8; 3] {
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
    let offset = (y * width + x) * 3;
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
    project.annotations[0].effects.fade_in = 0.4;
    export::export(&project, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let animated_times = timestamps(&out);
    assert_eq!(input_times.len(), animated_times.len());
    for (a, b) in input_times.iter().zip(animated_times) {
        assert!((a - b).abs() < 0.001, "animated: {a} vs {b}");
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

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn animated_mp4_and_gif_respect_effects_size_rate_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    let mut project = fixture(&source, true);
    let mut a = Annotation::new(Kind::Rectangle, [10.0, 20.0], [40.0, 70.0], 2.0);
    a.color = [255, 0, 0, 255];
    a.thickness = 10.0;
    a.effects.fade_in = 1.0;
    a.effects.fade_out = 0.5;
    project.annotations.push(a);
    let mp4 = dir.path().join("animated.mp4");
    export::export(&project, &mp4, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(pixel(&mp4, 0.0, 25, 24)[0] < 15);
    let faded = pixel(&mp4, 0.5, 25, 24)[0];
    assert!((80..180).contains(&faded), "{faded}");
    assert!(pixel(&mp4, 1.2, 25, 24)[0] > 180);
    assert!(pixel(&mp4, 1.2, 75, 24)[0] < 15);
    let gif = dir.path().join("animated é.gif");
    let format = export::Format::Gif { width: 80, fps: 10 };
    assert_eq!(project.video.duration, 2.0, "Unexpected fixture duration");
    export::export_with_format(&project, &gif, format, &AtomicBool::new(false), |_, _| {}).unwrap();
    let probe = Command::new(tool("ffprobe"))
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(&gif)
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = json["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["codec_name"], "gif");
    assert_eq!(streams[0]["width"], 80);
    assert_eq!(streams[0]["height"], 50);
    assert_eq!(streams[0]["nb_read_frames"], "20", "{json}");
    assert_eq!(streams[0]["r_frame_rate"], "10/1");
    let last_red = pixel_with_width(&gif, 1.9, 12, 12, 80)[0];
    assert!(
        (20..80).contains(&last_red),
        "Last GIF frame should show the final fade sample, got {last_red}"
    );
    let before = std::fs::read(&gif).unwrap();
    let cancel = AtomicBool::new(false);
    assert!(
        export::export_with_format(&project, &gif, format, &cancel, |_, message| {
            if message.starts_with("Rendering animation 2/") {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err()
        .contains("canceled")
    );
    assert_eq!(std::fs::read(&gif).unwrap(), before);
    assert!(
        export::export_with_format(
            &project,
            &gif,
            export::Format::Gif { width: 0, fps: 0 },
            &AtomicBool::new(false),
            |_, _| {}
        )
        .is_err()
    );
    assert!(
        export::export_with_format(
            &project,
            &source,
            format,
            &AtomicBool::new(false),
            |_, _| {}
        )
        .is_err()
    );
    project.video.duration = 400.0;
    assert!(
        export::export(&project, &mp4, &AtomicBool::new(false), |_, _| {})
            .unwrap_err()
            .contains("18000")
    );
    assert_eq!(std::fs::read(&gif).unwrap(), before);
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn static_gif_without_annotations_and_palette_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let project = fixture(&dir.path().join("silent.mp4"), false);
    let out = dir.path().join("silent.gif");
    let format = export::Format::Gif { width: 80, fps: 8 };
    export::export_with_format(&project, &out, format, &AtomicBool::new(false), |_, _| {}).unwrap();
    let times = timestamps(&out);
    assert_eq!(times.len(), 16);
    assert!((times[15] - 1.875).abs() < 0.015);
    let original = std::fs::read(&out).unwrap();
    let cancel = AtomicBool::new(false);
    assert!(
        export::export_with_format(&project, &out, format, &cancel, |_, message| {
            if message.starts_with("Building GIF palette") {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err()
        .contains("canceled")
    );
    assert_eq!(std::fs::read(&out).unwrap(), original);
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn orbiting_ball_color_and_path_survive_mp4_and_gif_export() {
    let dir = tempfile::tempdir().unwrap();
    for kind in [Kind::Rectangle, Kind::Ellipse] {
        let source = dir.path().join(format!("{kind:?}-source.mp4"));
        let mut project = fixture(&source, false);
        let mut a = Annotation::new(kind, [20.0, 20.0], [100.0, 70.0], 2.0);
        a.color = [255, 0, 0, 255];
        a.thickness = 6.0;
        a.effects.effect = Effect::Orbit {
            color: [0, 180, 255],
            period: 2.0,
        };
        project.annotations.push(a);
        let (start, opposite) = if kind == Kind::Rectangle {
            ([20, 20], [100, 70])
        } else {
            ([100, 45], [20, 45])
        };
        for (format, extension, width) in [
            (export::Format::Mp4, "mp4", 160),
            (export::Format::Gif { width: 80, fps: 10 }, "gif", 80),
        ] {
            let out = dir.path().join(format!("{kind:?}.{extension}"));
            export::export_with_format(&project, &out, format, &AtomicBool::new(false), |_, _| {})
                .unwrap();
            let at = |t, point: [usize; 2]| {
                pixel_with_width(
                    &out,
                    t,
                    point[0] * width / 160,
                    point[1] * width / 160,
                    width,
                )
            };
            assert!(at(0.0, start)[2] > 150, "Ball starts in its own blue color");
            assert!(
                at(1.0, opposite)[2] > 150,
                "Ball reaches the opposite contour point"
            );
            assert!(
                at(1.0, start)[2] < 80,
                "Ball moves away from its starting point"
            );
        }
    }
}

#[test]
#[ignore = "requires FFmpeg and ffprobe"]
fn glow_only_is_time_varying_in_mp4_and_gif() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    let mut project = fixture(&source, false);
    let mut annotation = Annotation::new(Kind::Rectangle, [20.0, 20.0], [100.0, 70.0], 2.0);
    annotation.color = [30, 60, 90, 255];
    annotation.thickness = 10.0;
    annotation.effects.effect = Effect::Glow {
        color: [200, 220, 240],
        pulse_hz: 0.5,
    };
    assert!(annotation.animated());
    project.annotations.push(annotation);

    let mp4 = dir.path().join("pulse.mp4");
    export::export(&project, &mp4, &AtomicBool::new(false), |_, _| {}).unwrap();
    let mp4_start = pixel(&mp4, 0.0, 50, 24);
    let mp4_peak = pixel(&mp4, 1.0, 50, 24);
    assert!(
        mp4_peak[1] > mp4_start[1] + 65,
        "{mp4_start:?} -> {mp4_peak:?}"
    );
    assert!(
        pixel(&mp4, 0.0, 50, 12).iter().all(|v| *v < 15),
        "No halo should appear outside the shape"
    );

    let gif = dir.path().join("pulse.gif");
    export::export_with_format(
        &project,
        &gif,
        export::Format::Gif { width: 80, fps: 10 },
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let gif_start = pixel_with_width(&gif, 0.0, 25, 12, 80);
    let gif_peak = pixel_with_width(&gif, 1.0, 25, 12, 80);
    assert!(
        gif_peak[1] > gif_start[1] + 45,
        "{gif_start:?} -> {gif_peak:?}"
    );
}
