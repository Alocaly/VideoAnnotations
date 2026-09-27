//! FFmpeg subprocess backend. No GUI types cross this boundary.
use serde::Deserialize;
use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: Option<f64>,
}

impl VideoInfo {
    /// Avoid requesting EOF, where no frame can be returned.
    pub fn last_seek_time(&self) -> f64 {
        (self.duration - 1.0 / self.fps.unwrap_or(30.0)).max(0.0)
    }

    pub fn clamp_time(&self, seconds: f64) -> f64 {
        if seconds.is_finite() {
            seconds.clamp(0.0, self.last_seek_time())
        } else {
            0.0
        }
    }
}

pub struct VideoFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Requested seek position; not a decoded presentation timestamp.
    pub requested_seconds: f64,
}

#[derive(Clone)]
pub struct MediaBackend {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

impl Default for MediaBackend {
    fn default() -> Self {
        Self {
            ffmpeg: executable("ffmpeg"),
            ffprobe: executable("ffprobe"),
        }
    }
}

pub(crate) fn executable(name: &str) -> PathBuf {
    let variable = format!("VIDEO_ANNOTATIONS_{}", name.to_uppercase());
    if let Some(path) = std::env::var_os(variable) {
        return path.into();
    }
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        // Supports running target/debug/*.exe from Explorer as well as cargo run.
        roots.extend(exe.ancestors().skip(1).take(3).map(Path::to_path_buf));
    }
    for root in roots {
        let local = root.join("tools/ffmpeg/bin").join(&filename);
        if local.is_file() {
            return local;
        }
    }
    filename.into()
}

impl MediaBackend {
    pub fn probe(&self, path: &Path) -> Result<VideoInfo, String> {
        if !path.is_file() {
            return Err(format!("Video file not found: {}", path.display()));
        }
        let mut command = Command::new(&self.ffprobe);
        command.args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,duration:format=duration",
            "-of",
            "json",
        ]);
        command.arg(path);
        let bytes = run(command)?;
        parse_probe(path, &bytes)
    }

    pub fn frame(&self, info: &VideoInfo, seconds: f64) -> Result<VideoFrame, String> {
        let seconds = info.clamp_time(seconds);
        let mut command = Command::new(&self.ffmpeg);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-ss",
            &format!("{seconds:.6}"),
            "-i",
        ]);
        command.arg(&info.path);
        command.args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-vf",
            "scale=1280:720:force_original_aspect_ratio=decrease:force_divisible_by=2:reset_sar=1",
            "-c:v",
            "png",
            "-f",
            "image2pipe",
            "pipe:1",
        ]);
        let bytes = run(command)?;
        if bytes.is_empty() {
            return Err("No frame at this position. Try an earlier time.".into());
        }
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|e| format!("Cannot read decoded frame: {e}"))?
            .into_rgba8();
        Ok(VideoFrame {
            width: image.width() as usize,
            height: image.height() as usize,
            rgba: image.into_raw(),
            requested_seconds: seconds,
        })
    }
}

#[derive(Deserialize)]
struct Probe {
    streams: Vec<Stream>,
    format: Option<Format>,
}
#[derive(Deserialize)]
struct Stream {
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
    avg_frame_rate: Option<String>,
}
#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
}

fn positive(text: &str) -> Option<f64> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn frame_rate(text: &str) -> Option<f64> {
    let (a, b) = text.split_once('/')?;
    let rate = positive(a)? / positive(b)?;
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

fn parse_probe(path: &Path, bytes: &[u8]) -> Result<VideoInfo, String> {
    let probe: Probe =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid video metadata: {e}"))?;
    let stream = probe
        .streams
        .first()
        .ok_or("This file has no video stream.")?;
    let width = stream
        .width
        .filter(|w| *w > 0)
        .ok_or("Missing video width.")?;
    let height = stream
        .height
        .filter(|h| *h > 0)
        .ok_or("Missing video height.")?;
    let duration = stream
        .duration
        .as_deref()
        .and_then(positive)
        .or_else(|| {
            probe
                .format
                .as_ref()?
                .duration
                .as_deref()
                .and_then(positive)
        })
        .ok_or("This prototype requires a video with a known, finite duration.")?;
    Ok(VideoInfo {
        path: path.into(),
        width,
        height,
        duration,
        fps: stream.avg_frame_rate.as_deref().and_then(frame_rate),
    })
}

/// Read both pipes concurrently, hide console windows on Windows, and bound hangs.
fn run(mut command: Command) -> Result<Vec<u8>, String> {
    let program: OsString = command.get_program().into();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Cannot start {}: {e}. Install FFmpeg (see README).",
                program.to_string_lossy()
            )
        })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let output = thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).map(|_| b)
    });
    let errors = thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).map(|_| b)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(10))
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(match result {
                    Err(e) => e.to_string(),
                    _ => "Video operation timed out after 30 seconds.".into(),
                });
            }
        }
    };
    let output = output
        .join()
        .map_err(|_| "Frame reader stopped.")?
        .map_err(|e| e.to_string())?;
    let errors = errors
        .join()
        .map_err(|_| "Error reader stopped.")?
        .map_err(|e| e.to_string())?;
    if !status?.success() {
        return Err(format!(
            "Video operation failed: {}",
            String::from_utf8_lossy(&errors).trim()
        ));
    }
    Ok(output)
}

pub struct Request {
    pub id: u64,
    pub path: PathBuf,
    pub info: Option<VideoInfo>,
    pub seconds: f64,
}

pub struct Response {
    pub id: u64,
    pub result: Result<(VideoInfo, VideoFrame), String>,
}

pub struct MediaWorker {
    pub requests: Sender<Request>,
    pub responses: Receiver<Response>,
}

impl MediaWorker {
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (requests, incoming) = mpsc::channel::<Request>();
        let (outgoing, responses) = mpsc::channel();
        thread::spawn(move || {
            let backend = MediaBackend::default();
            while let Ok(mut request) = incoming.recv() {
                // Scrubbing may enqueue requests faster than decoding: keep only the newest.
                while let Ok(newer) = incoming.try_recv() {
                    request = newer;
                }
                let result = (|| {
                    let info = match request.info {
                        Some(info) => info,
                        None => backend.probe(&request.path)?,
                    };
                    let frame = backend.frame(&info, request.seconds)?;
                    Ok((info, frame))
                })();
                if outgoing
                    .send(Response {
                        id: request.id,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                wake();
            }
        });
        Self {
            requests,
            responses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fractional_rate_and_fallback_duration() {
        let info = parse_probe(Path::new("test.mp4"), br#"{"streams":[{"width":1920,"height":1080,"duration":"N/A","avg_frame_rate":"30000/1001"}],"format":{"duration":"2.5"}}"#).unwrap();
        assert!((info.fps.unwrap() - 29.97002997).abs() < 0.00001);
        assert_eq!(info.duration, 2.5);
        assert_eq!(info.clamp_time(-1.0), 0.0);
        assert!(info.clamp_time(3.0) < info.duration);
        assert_eq!(info.clamp_time(f64::NAN), 0.0);
    }

    #[test]
    fn rejects_non_video_and_invalid_duration() {
        assert!(
            parse_probe(
                Path::new("audio.mp3"),
                br#"{"streams":[],"format":{"duration":"2"}}"#
            )
            .is_err()
        );
        assert!(
            parse_probe(
                Path::new("bad.mp4"),
                br#"{"streams":[{"width":100,"height":100}],"format":{"duration":"NaN"}}"#
            )
            .is_err()
        );
        assert_eq!(frame_rate("0/0"), None);
    }
}
