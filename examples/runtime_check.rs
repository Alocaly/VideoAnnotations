//! Console companion for portable packages; no GUI, network, or system audio.
use std::{process::Command, sync::Arc};
use video_annotations::{media, playback};

fn main() {
    let mut healthy = true;
    println!(
        "VideoAnnotations {} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for name in ["ffmpeg", "ffprobe"] {
        let path = media::executable(name);
        println!("{name}: {}", path.display());
        match Command::new(&path).arg("-version").output() {
            Ok(output) if output.status.success() => {
                println!(
                    "  {}",
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .unwrap_or("No version output")
                );
            }
            result => {
                eprintln!("  FAILED: {result:?}");
                healthy = false;
            }
        }
    }
    println!("libmpv: {}", playback::library_path().display());
    match playback::Player::with_audio_output(Arc::new(|| {}), "null") {
        Ok(_) => println!("  Client API and software renderer initialized successfully."),
        Err(error) => {
            eprintln!("  FAILED: {error}");
            healthy = false;
        }
    }
    if !healthy {
        eprintln!(
            "Run scripts/setup-ffmpeg.ps1 and scripts/setup-mpv.ps1, or configure VIDEO_ANNOTATIONS_* paths. See PORTABLE.md."
        );
        std::process::exit(1);
    }
}
