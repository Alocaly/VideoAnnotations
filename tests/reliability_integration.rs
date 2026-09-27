//! Generated-media matrix: run explicitly with FFmpeg and libmpv installed.
use std::{
    path::Path,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};
use video_annotations::{
    annotations::{Annotation, Kind},
    export,
    media::{self, VideoInfo},
    playback::Player,
    project::Project,
};

fn ffmpeg(args: &[&str], output: &Path) {
    let result = Command::new(media::executable("ffmpeg"))
        .args(["-v", "error", "-y"])
        .args(args)
        .arg(output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn pump(player: &mut Player, condition: impl Fn(&Player) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        player.poll();
        player.render([320, 180]).unwrap();
        assert!(player.state.error.is_none(), "{:?}", player.state);
        if condition(player) {
            return;
        }
        assert!(Instant::now() < deadline, "Timed out: {:?}", player.state);
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires FFmpeg, ffprobe, and libmpv; generated resolution/timing matrix"]
fn resolution_orientation_rate_and_long_duration_matrix() {
    let directory = tempfile::tempdir().unwrap();
    let cases = [
        ("portrait", "1080x1920", "30", "0.2", "1", 1080, 1920),
        ("4k", "3840x2160", "24", "0.125", "1", 3840, 2160),
        ("fractional", "320x180", "30000/1001", "0.5", "1", 320, 180),
        ("fast", "320x180", "120", "0.25", "1", 320, 180),
        ("odd", "321x181", "25", "0.2", "1", 321, 181),
        ("anamorphic", "320x240", "25", "0.2", "4/3", 426, 240),
        ("rotated", "320x180", "25", "0.2", "1", 180, 320),
        ("rotated180", "320x180", "25", "0.2", "1", 320, 180),
        ("rotated270", "320x180", "25", "0.2", "1", 180, 320),
        ("hour", "160x90", "1", "3600", "1", 160, 90),
    ];
    for (name, size, rate, duration, sar, width, height) in cases {
        let start = Instant::now();
        let source = directory.path().join(format!("{name} é.mkv"));
        let filter = format!("testsrc=size={size}:rate={rate}:duration={duration},setsar={sar}");
        ffmpeg(
            &[
                "-f", "lavfi", "-i", &filter, "-c:v", "ffv1", "-threads", "1",
            ],
            &source,
        );
        let source = if name.starts_with("rotated") {
            let rotated = directory.path().join("rotated.mp4");
            ffmpeg(
                &[
                    "-i",
                    source.to_str().unwrap(),
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ],
                &rotated,
            );
            // Remux rotation metadata without changing encoded pixel dimensions.
            let metadata = directory.path().join("rotation-metadata.mp4");
            ffmpeg(
                &[
                    "-display_rotation",
                    match name {
                        "rotated180" => "180",
                        "rotated270" => "270",
                        _ => "90",
                    },
                    "-i",
                    rotated.to_str().unwrap(),
                    "-c",
                    "copy",
                ],
                &metadata,
            );
            metadata
        } else {
            source
        };
        let mut player = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
        player.load(&source).unwrap();
        pump(&mut player, |p| {
            p.state.loaded && p.state.width > 0 && p.state.duration > 0.0
        });
        assert_eq!(
            (player.state.width, player.state.height),
            (width, height),
            "{name}"
        );
        if name.starts_with("rotated") {
            player.render([1, 1]).unwrap();
            let frame = player
                .render([width as usize, height as usize])
                .unwrap()
                .unwrap();
            let reference = Command::new(media::executable("ffmpeg"))
                .args(["-v", "error", "-i"])
                .arg(&source)
                .args([
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "rgb24",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(reference.status.success());
            for (x, y) in [
                (30, 30),
                (width as usize - 30, 30),
                (30, height as usize - 30),
                (width as usize - 30, height as usize - 30),
            ] {
                let index = y * width as usize + x;
                for channel in 0..3 {
                    let actual = frame.rgba[index * 4 + channel];
                    let expected = reference.stdout[index * 3 + channel];
                    assert!(
                        actual.abs_diff(expected) < 25,
                        "Rotation pixel ({x},{y}): {actual} vs {expected}"
                    );
                }
            }
        }
        let actual_duration = player.state.duration;
        let target = if name == "hour" { 3598.0 } else { 0.08 };
        player.seek(target).unwrap();
        pump(&mut player, |p| {
            !p.state.seeking
                && (p.state.position - target).abs()
                    <= 1.0 / rate.parse::<f64>().unwrap_or(29.97) + 0.02
        });
        let mut project = Project::new(VideoInfo {
            path: source.clone(),
            width,
            height,
            duration: actual_duration,
            fps: player.state.fps,
        });
        let mut annotation =
            Annotation::new(Kind::Rectangle, [10.0, 10.0], [70.0, 60.0], actual_duration);
        annotation.color = [255, 0, 0, 255];
        annotation.thickness = 8.0;
        project.annotations.push(annotation);
        if name.starts_with("rotated") {
            player
                .load(&directory.path().join(format!("{name} é.mkv")))
                .unwrap();
            assert!(!player.state.loaded);
            pump(&mut player, |p| p.state.loaded && p.state.width > 0);
            assert_eq!((player.state.width, player.state.height), (320, 180));
        }
        drop(player);
        let output = directory.path().join(format!("{name}-export.mp4"));
        export::export(&project, &output, &AtomicBool::new(false), |_, _| {}).unwrap();
        let info = media::MediaBackend::default().probe(&output).unwrap();
        assert_eq!(
            (info.width, info.height),
            (width.next_multiple_of(2), height.next_multiple_of(2)),
            "{name}"
        );
        assert!(
            (info.duration - actual_duration).abs() < 0.1,
            "{name}: {} vs {actual_duration}",
            info.duration
        );
        let decoded = Command::new(media::executable("ffmpeg"))
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "crop=1:1:40:12:exact=1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert!(
            decoded.stdout[0] > 170 && decoded.stdout[1] < 90,
            "{name}: annotation pixel {:?}",
            decoded.stdout
        );
        if name == "hour" {
            project.annotations[0].effects.fade_in = 1.0;
            assert!(
                export::export(&project, &output, &AtomicBool::new(false), |_, _| {})
                    .unwrap_err()
                    .contains("18000")
            );
        }
        println!(
            "{name}: {width}x{height}, {actual_duration:.3}s, test elapsed {:?}",
            start.elapsed()
        );
    }
}

#[test]
#[ignore = "requires FFmpeg; measures a small 720p animated export (no speed threshold)"]
fn animated_720p_export_timing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=black:size=1280x720:rate=30:duration=1",
            "-c:v",
            "libx264",
        ],
        &source,
    );
    let mut project = Project::new(media::MediaBackend::default().probe(&source).unwrap());
    let mut annotation = Annotation::new(Kind::Ellipse, [100.0, 100.0], [500.0, 400.0], 1.0);
    annotation.effects.fade_in = 0.2;
    annotation.effects.movement = [200.0, 0.0];
    annotation.effects.glow = 12.0;
    annotation.effects.outline_period = 1.0;
    project.annotations.push(annotation);
    let start = Instant::now();
    let mut previous = 0.0;
    export::export(
        &project,
        &dir.path().join("out.mp4"),
        &AtomicBool::new(false),
        |fraction, _| {
            assert!(
                fraction >= previous,
                "Export progress must not move backwards"
            );
            previous = fraction;
        },
    )
    .unwrap();
    assert_eq!(previous, 1.0);
    println!(
        "720p/1-second animated MP4 (60 overlay frames): {:?}",
        start.elapsed()
    );
}
