//! cargo test --test playback_integration -- --ignored --nocapture
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use video_annotations::playback::Player;

fn fixture(root: &Path, audio: bool, variable: bool) -> PathBuf {
    let path = root.join(format!("video é with spaces-{audio}-{variable}.mkv"));
    let ffmpeg = std::env::var_os("VIDEO_ANNOTATIONS_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tools/ffmpeg/bin/ffmpeg.exe"));
    let mut command = Command::new(ffmpeg);
    command.args([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=25:duration=4",
    ]);
    if audio {
        command.args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=4",
            "-c:a",
            "aac",
        ]);
    }
    if variable {
        command.args([
            "-vf",
            r"select=if(lt(t\,2)\,not(mod(n\,2))\,1)",
            "-fps_mode",
            "vfr",
        ]);
    }
    let result = command
        .args(["-c:v", if variable { "ffv1" } else { "libx264" }, "-y"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    path
}

fn pump_until(player: &mut Player, condition: impl Fn(&Player) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        player.poll();
        player.render([320, 180]).unwrap();
        assert!(player.state.error.is_none(), "{:?}", player.state);
        if condition(player) {
            break;
        }
        assert!(Instant::now() < deadline, "Timed out: {:?}", player.state);
        thread::sleep(Duration::from_millis(5));
    }
}

fn pump_for(player: &mut Player, duration: Duration) {
    let start = Instant::now();
    pump_until(player, |_| start.elapsed() >= duration);
}

#[test]
#[ignore = "requires local libmpv and FFmpeg"]
fn playback_pause_seek_step_audio_eof_and_replay() {
    let temp = tempfile::tempdir().unwrap();
    let path = fixture(temp.path(), true, false);
    let mut player = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
    player.load(&path).unwrap();
    pump_until(&mut player, |p| {
        p.state.loaded
            && p.state.width == 320
            && p.state.duration > 3.9
            && p.state.audio_output.is_some()
    });
    assert!(player.state.paused);
    assert_eq!(player.state.audio_rate, Some(48000.0));
    player.pause(false).unwrap();
    pump_until(&mut player, |p| {
        p.state.position > 0.6 && p.state.av_sync.is_some()
    });
    assert!(
        player.state.av_sync.unwrap().abs() < 0.15,
        "{:?}",
        player.state
    );
    player.pause(true).unwrap();
    pump_until(&mut player, |p| p.state.paused);
    pump_for(&mut player, Duration::from_millis(80));
    let paused = player.state.position;
    pump_for(&mut player, Duration::from_millis(200));
    assert!((player.state.position - paused).abs() < 0.001);
    player.seek(2.0).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 2.0).abs() < 0.005 && !p.state.seeking
    });
    player.step(true).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 2.04).abs() < 0.005 && p.state.paused
    });
    player.step(false).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 2.0).abs() < 0.005 && !p.state.seeking
    });
    player.seek(3.7).unwrap();
    player.pause(false).unwrap();
    pump_until(&mut player, |p| p.state.ended);
    pump_for(&mut player, Duration::from_millis(200));
    assert!(
        player.state.ended,
        "EOF must remain visible after keep-open pauses"
    );
    assert!(player.state.loaded, "Keep last frame at EOF");
    player.toggle().unwrap();
    pump_until(&mut player, |p| {
        !p.state.ended && !p.state.paused && p.state.position < 0.5
    });
    player.mute(true).unwrap();
    player.volume(20.0).unwrap();
    pump_for(&mut player, Duration::from_millis(80));
    println!("Audio/video sync after replay: {:?}", player.state.av_sync);
}

#[test]
#[ignore = "requires local libmpv and FFmpeg"]
fn silent_variable_rate_steps_use_real_frame_timestamps() {
    let temp = tempfile::tempdir().unwrap();
    let path = fixture(temp.path(), false, true);
    let mut player = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
    player.load(&path).unwrap();
    pump_until(&mut player, |p| {
        p.state.loaded && p.state.width > 0 && p.state.duration > 3.0
    });
    player.seek(0.8).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 0.8).abs() < 0.005 && !p.state.seeking
    });
    player.step(true).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 0.88).abs() < 0.005 && p.state.paused
    });
    player.step(false).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 0.8).abs() < 0.005 && !p.state.seeking
    });
    player.seek(2.4).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 2.4).abs() < 0.005 && !p.state.seeking
    });
    player.step(true).unwrap();
    pump_until(&mut player, |p| {
        (p.state.position - 2.44).abs() < 0.005 && p.state.paused
    });
    assert!(player.state.audio_rate.is_none());
}

#[test]
#[ignore = "requires local libmpv and FFmpeg"]
fn invalid_files_and_reopening_do_not_leave_stale_playback() {
    let temp = tempfile::tempdir().unwrap();
    let invalid = temp.path().join("invalid.mp4");
    std::fs::write(&invalid, b"not a video").unwrap();
    let mut player = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
    assert!(player.load(&temp.path().join("missing.mp4")).is_err());
    player.load(&invalid).unwrap();
    let start = Instant::now();
    while player.state.error.is_none() {
        player.poll();
        player.render([320, 180]).unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(5));
    }
    drop(player);
    let path = fixture(temp.path(), false, false);
    let mut replacement = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
    replacement.load(&path).unwrap();
    pump_until(&mut replacement, |p| p.state.loaded && p.state.width == 320);
    assert!(replacement.state.paused);
    assert!(replacement.state.position < 0.1);
}

#[test]
#[ignore = "requires libmpv, FFmpeg, and an accessible system audio device"]
fn system_audio_device_stays_synchronized() {
    let temp = tempfile::tempdir().unwrap();
    let path = fixture(temp.path(), true, false);
    let mut player = Player::new(Arc::new(|| {})).unwrap();
    player.volume(5.0).unwrap();
    player.load(&path).unwrap();
    pump_until(&mut player, |p| {
        p.state.loaded && p.state.audio_output.is_some()
    });
    assert_ne!(player.state.audio_output.as_deref(), Some("null"));
    player.pause(false).unwrap();
    pump_until(&mut player, |p| {
        p.state.position > 1.0 && p.state.av_sync.is_some()
    });
    assert!(
        player.state.av_sync.unwrap().abs() < 0.15,
        "{:?}",
        player.state
    );
    println!(
        "System output: {:?}; A/V offset: {:?}",
        player.state.audio_output, player.state.av_sync
    );
    player.seek(2.0).unwrap();
    pump_until(&mut player, |p| {
        p.state.position > 2.5 && p.state.av_sync.is_some()
    });
    assert!(
        player.state.av_sync.unwrap().abs() < 0.15,
        "{:?}",
        player.state
    );
}
