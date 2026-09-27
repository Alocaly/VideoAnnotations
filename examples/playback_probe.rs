//! Diagnostic for a local file without the GUI: cargo run --example playback_probe -- path
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use video_annotations::playback::Player;
fn main() {
    let path = PathBuf::from(std::env::args_os().nth(1).expect("Pass a video path"));
    let mut player = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
    player.load(&path).unwrap();
    let start = Instant::now();
    let mut played = false;
    let mut previous = 0;
    while start.elapsed() < Duration::from_secs(8) {
        player.poll();
        player.render([1280, 720]).unwrap();
        if player.state.loaded && !played {
            player.pause(false).unwrap();
            played = true;
        }
        let now = start.elapsed().as_millis() / 500;
        if now != previous {
            println!("{:.2}s {:?}", start.elapsed().as_secs_f64(), player.state);
            previous = now;
        }
        thread::sleep(Duration::from_millis(5));
    }
}
