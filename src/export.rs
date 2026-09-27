//! Background MP4 export; the destination is replaced only on success.
use crate::{document, media, project::Project, render};
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Event {
    Progress { fraction: f32, message: String },
    Finished(Result<PathBuf, String>),
}
pub struct Job {
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

// Clean up even if the worker unexpectedly panics after spawning FFmpeg.
struct Process(std::process::Child);
impl std::ops::Deref for Process {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Process {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Job {
    pub fn start(project: Project, destination: PathBuf) -> Self {
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                export(&project, &destination, &signal, |fraction, message| {
                    let _ = tx.send(Event::Progress { fraction, message });
                })
            }))
            .unwrap_or_else(|_| Err("Export worker failed unexpectedly.".into()));
            let _ = tx.send(Event::Finished(result.map(|()| destination)));
        });
        Self { events, cancel }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn canceled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Export canceled. Destination unchanged.".into())
    } else {
        Ok(())
    }
}

/// Synchronous entry point, also used by integration tests.
pub fn export(
    project: &Project,
    destination: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f32, String),
) -> Result<(), String> {
    document::validate(project)?;
    canceled(cancel)?;
    if project.annotations.len() > 32 {
        return Err("This first exporter supports at most 32 annotations.".into());
    }
    let size = [project.video.width, project.video.height];
    if u64::from(size[0]) * u64::from(size[1]) > 9_000_000 {
        return Err("Export supports up to 9 megapixels.".into());
    }
    let destination = std::path::absolute(destination).map_err(|e| e.to_string())?;
    let source = project
        .video
        .path
        .canonicalize()
        .map_err(|e| format!("Cannot read source video: {e}"))?;
    if destination == source || destination.canonicalize().ok().as_ref() == Some(&source) {
        return Err("Export cannot overwrite the source video.".into());
    }
    if !destination
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
    {
        return Err("Choose an .mp4 destination.".into());
    }
    let directory = destination.parent().ok_or("Invalid destination")?;
    let output = tempfile::Builder::new()
        .prefix(".video-annotations-")
        .suffix(".mp4")
        .tempfile_in(directory)
        .map_err(|e| format!("Cannot create output: {e}"))?
        .into_temp_path();
    let overlays = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut command = Command::new(media::executable("ffmpeg"));
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-filter_complex_threads",
            "1",
            "-threads",
            "2",
            "-i",
        ])
        .arg(&source);
    for (i, a) in project.annotations.iter().enumerate() {
        canceled(cancel)?;
        progress(
            0.1 * i as f32 / project.annotations.len().max(1) as f32,
            format!(
                "Preparing annotation {}/{}",
                i + 1,
                project.annotations.len()
            ),
        );
        let image = render::rasterize(a, size, cancel)?;
        let file = overlays.path().join(format!("overlay-{i}.png"));
        image
            .save(&file)
            .map_err(|e| format!("Cannot prepare annotation: {e}"))?;
        command.args(["-threads", "1", "-i"]).arg(file);
    }
    // FFmpeg autorotates the source before this filter. Normalize display pixels/SAR.
    let mut filters = format!(
        "[0:v:0]scale={}:{}:flags=lanczos,setsar=1[v0];",
        size[0], size[1]
    );
    for (i, a) in project.annotations.iter().enumerate() {
        filters.push_str(&format!("[v{i}][{}:v:0]overlay=0:0:format=auto:eof_action=repeat:enable='gte(t,{:.9})*lt(t,{:.9})'[v{}];",i+1,a.start_seconds,a.end_seconds,i+1));
    }
    filters.push_str(&format!(
        "[v{}]pad=ceil(iw/2)*2:ceil(ih/2)*2,format=yuv420p[out]",
        project.annotations.len()
    ));
    command
        .args([
            "-filter_complex",
            &filters,
            "-map",
            "[out]",
            "-map",
            "0:a:0?",
            "-map_metadata",
            "-1",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "18",
            "-threads",
            "2",
            "-fps_mode",
            "vfr",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-t",
            &format!("{:.9}", project.video.duration),
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
            "-f",
            "mp4",
        ])
        .arg(&output);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    canceled(cancel)?;
    progress(0.1, "Encoding MP4 (H.264 + AAC)…".into());
    let mut child = Process(command.spawn().map_err(|e| {
        format!("Cannot start FFmpeg: {e}. Install FFmpeg or set VIDEO_ANNOTATIONS_FFMPEG.")
    })?);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (times, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time_us=")
                && let Ok(value) = value.parse::<f64>()
            {
                let _ = times.send(value / 1_000_000.0);
            }
        }
    });
    let errors = thread::spawn(move || {
        let mut stderr = stderr;
        let mut tail = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(n) = stderr.read(&mut buffer) {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..n]);
            if tail.len() > 16384 {
                tail.drain(..tail.len() - 16384);
            }
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let mut last = Instant::now();
    let status = loop {
        for time in rx.try_iter() {
            last = Instant::now();
            progress(
                (0.1 + 0.89 * (time / project.video.duration).clamp(0.0, 1.0)) as f32,
                format!(
                    "Encoding: {:.1} / {:.1} seconds",
                    time.max(0.0),
                    project.video.duration
                ),
            );
        }
        if cancel.load(Ordering::Relaxed) || last.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = errors.join();
            canceled(cancel)?;
            return Err("FFmpeg made no progress for 120 seconds. Destination unchanged.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                let _ = errors.join();
                return Err(e.to_string());
            }
        }
    };
    let _ = reader.join();
    let error = errors.join().unwrap_or_default();
    if !status.success() {
        return Err(format!("FFmpeg export failed: {error}"));
    }
    canceled(cancel)?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&output)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Cannot flush exported video: {e}"))?;
    canceled(cancel)?;
    output
        .persist(&destination)
        .map_err(|e| format!("Cannot finalize export: {}", e.error))?;
    progress(1.0, "Export complete".into());
    Ok(())
}
