//! Background MP4/GIF export; the destination is replaced only on success.
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

#[derive(Clone, Copy, Debug)]
pub enum Format {
    Mp4,
    Gif { width: u32, fps: u32 },
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Gif { .. } => "gif",
        }
    }
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
        Self::start_with_format(project, destination, Format::Mp4)
    }
    pub fn start_with_format(project: Project, destination: PathBuf, format: Format) -> Self {
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                export_with_format(
                    &project,
                    &destination,
                    format,
                    &signal,
                    |fraction, message| {
                        let _ = tx.send(Event::Progress { fraction, message });
                    },
                )
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
    progress: impl FnMut(f32, String),
) -> Result<(), String> {
    export_with_format(project, destination, Format::Mp4, cancel, progress)
}

pub fn export_with_format(
    project: &Project,
    destination: &Path,
    format: Format,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f32, String),
) -> Result<(), String> {
    document::validate(project)?;
    canceled(cancel)?;
    if let Format::Gif { width, fps } = format
        && (!(64..=1920).contains(&width) || !(1..=30).contains(&fps))
    {
        return Err("GIF width must be 64–1920 pixels and frame rate 1–30 fps.".into());
    }
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
        .is_some_and(|e| e.eq_ignore_ascii_case(format.extension()))
    {
        return Err(format!("Choose a .{} destination.", format.extension()));
    }
    let directory = destination.parent().ok_or("Invalid destination")?;
    let output = tempfile::Builder::new()
        .prefix(".video-annotations-")
        .suffix(&format!(".{}", format.extension()))
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
    let animated = project.annotations.iter().any(|a| a.animated());
    let preparation = if animated { 0.5 } else { 0.1 };
    if animated {
        // Disk-backed, lossless overlay frames keep encoding memory bounded.
        let fps = match format {
            Format::Mp4 => 60,
            Format::Gif { fps, .. } => fps,
        };
        let frames = (project.video.duration * fps as f64).ceil();
        if frames > 18000.0 {
            return Err("Animated export is limited to 18000 overlay frames. Shorten the video or lower GIF fps.".into());
        }
        let mut bytes = 0_u64;
        for frame in 0..frames as u32 {
            canceled(cancel)?;
            progress(
                preparation * frame as f32 / frames as f32,
                format!("Rendering animation {}/{}", frame + 1, frames as u32),
            );
            let image = render::rasterize_scene(
                &project.annotations,
                size,
                frame as f64 / fps as f64,
                project.video.duration,
                cancel,
            )?;
            let file = overlays.path().join(format!("frame-{frame:06}.png"));
            image
                .save(&file)
                .map_err(|e| format!("Cannot prepare animation: {e}"))?;
            bytes += std::fs::metadata(file).map_err(|e| e.to_string())?.len();
            if bytes > 2 * 1024 * 1024 * 1024 {
                return Err("Animation temporary files exceed the 2 GiB limit.".into());
            }
        }
        command
            .args(["-threads", "1", "-framerate", &fps.to_string(), "-i"])
            .arg(overlays.path().join("frame-%06d.png"));
    } else {
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
    }
    // FFmpeg autorotates the source before this filter. Normalize display pixels/SAR.
    // Resample GIF source frames before overlay: overlay's EOF timestamp can
    // omit the last frame duration, which would make a downstream fps filter
    // drop that frame. Effects now also line up with the GIF sampling clock.
    let clock = match format {
        Format::Mp4 => String::new(),
        Format::Gif { fps, .. } => format!(
            "fps={fps}:start_time=0:eof_action=pass,tpad=stop_mode=clone:stop_duration=1,trim=duration={:.9},",
            project.video.duration
        ),
    };
    let mut filters = format!(
        "[0:v:0]{clock}scale={}:{}:flags=lanczos,setsar=1[v0];",
        size[0], size[1]
    );
    let layers = if animated {
        1
    } else {
        project.annotations.len()
    };
    if animated {
        filters.push_str("[v0][1:v:0]overlay=0:0:format=auto:eof_action=repeat[v1];");
    } else {
        for (i, a) in project.annotations.iter().enumerate() {
            filters.push_str(&format!("[v{i}][{}:v:0]overlay=0:0:format=auto:eof_action=repeat:enable='gte(t,{:.9})*lt(t,{:.9})'[v{}];",i+1,a.start_seconds,a.end_seconds,i+1));
        }
    }
    let mut encoding_start = preparation;
    match format {
        Format::Mp4 => filters.push_str(&format!(
            "[v{layers}]pad=ceil(iw/2)*2:ceil(ih/2)*2,format=yuv420p[out]"
        )),
        Format::Gif { width, .. } => {
            let width = width.min(size[0]);
            let height = (size[1] as f64 * width as f64 / size[0] as f64)
                .round()
                .max(1.0) as u32;
            let conversion = format!(
                "[v{layers}]trim=duration={:.9},scale={width}:{height}:flags=lanczos",
                project.video.duration
            );
            // Two passes avoid buffering all frames and use a stable global palette.
            let palette = overlays.path().join("palette.png");
            let mut palette_command = Command::new(command.get_program());
            palette_command.args(command.get_args());
            palette_command
                .args([
                    "-filter_complex",
                    &format!("{filters}{conversion},palettegen[palette]"),
                    "-map",
                    "[palette]",
                    "-an",
                    "-frames:v",
                    "1",
                    "-threads",
                    "1",
                    "-update",
                    "1",
                ])
                .arg(&palette);
            progress(preparation, "Building GIF palette…".into());
            run_encoder(
                palette_command,
                cancel,
                project.video.duration,
                [preparation, 0.7],
                &mut progress,
            )?;
            encoding_start = 0.7;
            command.args(["-threads", "1", "-i"]).arg(palette);
            filters.push_str(&format!(
                "{conversion}[pixels];[pixels][{}:v:0]paletteuse=dither=sierra2_4a[out]",
                layers + 1
            ));
        }
    }
    command.args([
        "-filter_complex",
        &filters,
        "-map",
        "[out]",
        "-map_metadata",
        "-1",
    ]);
    if matches!(format, Format::Mp4) {
        command.args([
            "-map",
            "0:a:0?",
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
            "-movflags",
            "+faststart",
        ]);
    } else {
        command.args([
            "-an",
            "-c:v",
            "gif",
            "-fps_mode",
            "passthrough",
            "-loop",
            "0",
        ]);
    }
    command
        .args([
            "-t",
            &format!("{:.9}", project.video.duration),
            "-f",
            format.extension(),
        ])
        .arg(&output);
    progress(
        encoding_start,
        format!("Encoding {}…", format.extension().to_uppercase()),
    );
    run_encoder(
        command,
        cancel,
        project.video.duration,
        [encoding_start, 0.99],
        &mut progress,
    )?;
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

fn run_encoder(
    mut command: Command,
    cancel: &AtomicBool,
    duration: f64,
    range: [f32; 2],
    progress: &mut impl FnMut(f32, String),
) -> Result<(), String> {
    command.args(["-progress", "pipe:1", "-nostats"]);
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
                range[0] + (range[1] - range[0]) * (time / duration).clamp(0.0, 1.0) as f32,
                format!("Encoding: {:.1} / {:.1} seconds", time.max(0.0), duration),
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
    Ok(())
}
