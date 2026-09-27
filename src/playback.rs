//! Persistent playback through libmpv's version-2 C API.
//! Handles stay on the creating thread. Commands and properties are asynchronous;
//! callbacks only flag work and wake the UI, never call back into libmpv.
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_ulong, c_void},
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type Handle = *mut c_void;
type Callback = Option<unsafe extern "C" fn(*mut c_void)>;
#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct Property {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}
#[repr(C)]
struct Log {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

struct Api {
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    destroy: unsafe extern "C" fn(Handle),
    option: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, u64, *const *const c_char) -> c_int,
    observe: unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int,
    wait: unsafe extern "C" fn(Handle, f64) -> *const Event,
    error: unsafe extern "C" fn(c_int) -> *const c_char,
    log: unsafe extern "C" fn(Handle, *const c_char) -> c_int,
    wakeup: unsafe extern "C" fn(Handle, Callback, *mut c_void),
    render_create: unsafe extern "C" fn(*mut Handle, Handle, *mut Param) -> c_int,
    render_free: unsafe extern "C" fn(Handle),
    render_callback: unsafe extern "C" fn(Handle, Callback, *mut c_void),
    render_update: unsafe extern "C" fn(Handle) -> u64,
    render: unsafe extern "C" fn(Handle, *mut Param) -> c_int,
    _library: Library,
}

impl Api {
    fn load() -> Result<Self, String> {
        let path = library_path();
        // SAFETY: symbols use the published C ABI in client.h/render.h. Check
        // the major ABI and retain the DLL until every handle is destroyed.
        unsafe {
            let library = Library::new(&path).map_err(|e| {
                format!(
                    "Cannot load {}: {e}. Run scripts/setup-mpv.ps1 (see README).",
                    path.display()
                )
            })?;
            let version = library
                .get::<unsafe extern "C" fn() -> c_ulong>(b"mpv_client_api_version\0")
                .map_err(|e| e.to_string())?();
            if version >> 16 != 2 {
                return Err("libmpv client API version 2 is required.".into());
            }
            macro_rules! symbol {
                ($name:literal) => {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            Ok(Self {
                create: symbol!("mpv_create"),
                initialize: symbol!("mpv_initialize"),
                destroy: symbol!("mpv_terminate_destroy"),
                option: symbol!("mpv_set_option_string"),
                command: symbol!("mpv_command_async"),
                observe: symbol!("mpv_observe_property"),
                wait: symbol!("mpv_wait_event"),
                error: symbol!("mpv_error_string"),
                log: symbol!("mpv_request_log_messages"),
                wakeup: symbol!("mpv_set_wakeup_callback"),
                render_create: symbol!("mpv_render_context_create"),
                render_free: symbol!("mpv_render_context_free"),
                render_callback: symbol!("mpv_render_context_set_update_callback"),
                render_update: symbol!("mpv_render_context_update"),
                render: symbol!("mpv_render_context_render"),
                _library: library,
            })
        }
    }

    fn check(&self, code: c_int) -> Result<(), String> {
        if code >= 0 {
            Ok(())
        } else {
            // SAFETY: mpv_error_string returns a static NUL-terminated string.
            Err(unsafe { CStr::from_ptr((self.error)(code)) }
                .to_string_lossy()
                .into_owned())
        }
    }
}

pub fn library_path() -> PathBuf {
    let filename = if cfg!(windows) {
        "libmpv-2.dll"
    } else if cfg!(target_os = "macos") {
        "libmpv.2.dylib"
    } else {
        "libmpv.so.2"
    };
    crate::runtime::resolve(
        "VIDEO_ANNOTATIONS_MPV",
        &Path::new("tools/mpv").join(filename),
        filename,
    )
}

struct Notify {
    dirty: AtomicBool,
    wake: Arc<dyn Fn() + Send + Sync>,
}
unsafe extern "C" fn notify(data: *mut c_void) {
    // SAFETY: this boxed Notify lives until both callbacks are removed and the
    // render context/core destroyed. Never unwind across the C boundary.
    let notify = unsafe { &*(data as *const Notify) };
    notify.dirty.store(true, Ordering::Release);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (notify.wake)()));
}

#[derive(Clone, Debug)]
pub struct PlaybackState {
    pub loaded: bool,
    pub paused: bool,
    pub seeking: bool,
    pub ended: bool,
    pub position: f64,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub audio_rate: Option<f64>,
    pub audio_output: Option<String>,
    pub av_sync: Option<f64>,
    pub error: Option<String>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            loaded: false,
            paused: true,
            seeking: false,
            ended: false,
            position: 0.0,
            duration: 0.0,
            width: 0,
            height: 0,
            fps: None,
            audio_rate: None,
            audio_output: None,
            av_sync: None,
            error: None,
        }
    }
}

pub struct RenderedFrame {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}

pub struct Player {
    api: Api,
    handle: Handle,
    renderer: Handle,
    notify: Box<Notify>,
    pixels: Vec<u32>, // At least 4-byte aligned, as required by rgb0.
    size: [usize; 2],
    display_size: [u32; 2],
    rotation: Option<f64>,
    pub state: PlaybackState,
}

impl Player {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        Self::create(wake, None)
    }

    /// Tests can use mpv's clocked null audio output without a physical speaker.
    pub fn with_audio_output(
        wake: Arc<dyn Fn() + Send + Sync>,
        output: &str,
    ) -> Result<Self, String> {
        Self::create(wake, Some(output))
    }

    fn create(wake: Arc<dyn Fn() + Send + Sync>, output: Option<&str>) -> Result<Self, String> {
        let api = Api::load()?;
        // SAFETY: loaded function, no arguments.
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err("Cannot create the playback engine.".into());
        }
        let mut player = Self {
            api,
            handle,
            renderer: ptr::null_mut(),
            notify: Box::new(Notify {
                dirty: AtomicBool::new(true),
                wake,
            }),
            pixels: Vec::new(),
            size: [0, 0],
            display_size: [0, 0],
            rotation: None,
            state: PlaybackState::default(),
        };
        for (name, value) in [
            ("config", "no"),
            ("terminal", "no"),
            ("load-scripts", "no"),
            ("vo", "libmpv"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("pause", "yes"),
            ("video-sync", "audio"),
            ("hwdec", "no"),
            // The libmpv software renderer does not rotate pixels itself.
            // Render unrotated, then apply display metadata to the RGB buffer.
            ("video-rotate", "no"),
            ("osd-level", "0"),
            ("audio-display", "no"),
            ("sub-auto", "no"),
            ("sid", "no"),
            ("hr-seek-framedrop", "no"),
            ("volume", "70"),
            ("demuxer-max-bytes", "64MiB"),
            ("demuxer-max-back-bytes", "16MiB"),
        ] {
            player.option(name, value)?;
        }
        if let Some(output) = output {
            player.option("ao", output)?;
        }
        // SAFETY: live handle, initialization exactly once.
        player
            .api
            .check(unsafe { (player.api.initialize)(handle) })?;
        let mut advanced = 1_i32;
        let mut params = [
            Param {
                kind: 1,
                data: c"sw".as_ptr().cast_mut().cast(),
            },
            Param {
                kind: 10,
                data: (&mut advanced as *mut i32).cast(),
            },
            Param {
                kind: 0,
                data: ptr::null_mut(),
            },
        ];
        // SAFETY: terminated parameter array with correctly typed live values.
        player.api.check(unsafe {
            (player.api.render_create)(&mut player.renderer, handle, params.as_mut_ptr())
        })?;
        let callback_data = (&mut *player.notify as *mut Notify).cast();
        // SAFETY: stable boxed data, unregistered by Drop before release.
        unsafe {
            (player.api.wakeup)(handle, Some(notify), callback_data);
            (player.api.render_callback)(player.renderer, Some(notify), callback_data);
            (player.api.log)(handle, c"error".as_ptr());
        }
        for (id, name, format) in [
            (1, "time-pos", 5),
            (2, "duration", 5),
            (3, "pause", 3),
            (4, "eof-reached", 3),
            (5, "seeking", 3),
            (6, "dwidth", 5),
            (7, "dheight", 5),
            (8, "container-fps", 5),
            (9, "audio-params/samplerate", 5),
            (10, "current-ao", 1),
            (11, "avsync", 5),
            (12, "video-dec-params/rotate", 5),
        ] {
            let name = CString::new(name).unwrap();
            // SAFETY: libmpv copies the name, event formats decoded in poll().
            player
                .api
                .check(unsafe { (player.api.observe)(handle, id, name.as_ptr(), format) })?;
        }
        Ok(player)
    }

    fn option(&self, name: &str, value: &str) -> Result<(), String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        let value = CString::new(value).map_err(|e| e.to_string())?;
        // SAFETY: valid handle and strings for the duration of the call.
        self.api
            .check(unsafe { (self.api.option)(self.handle, name.as_ptr(), value.as_ptr()) })
    }

    fn command(&self, args: &[&str]) -> Result<(), String> {
        let strings = args
            .iter()
            .map(|s| CString::new(*s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut pointers = strings.iter().map(|s| s.as_ptr()).collect::<Vec<_>>();
        pointers.push(ptr::null());
        // SAFETY: null-terminated argv copied by libmpv before returning.
        self.api
            .check(unsafe { (self.api.command)(self.handle, 0, pointers.as_ptr()) })
    }

    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let path = std::fs::canonicalize(path).map_err(|e| format!("Cannot open video: {e}"))?;
        let path = path
            .to_str()
            .ok_or("The video path is not valid Unicode.")?;
        let path = path
            .strip_prefix(r"\\?\UNC\")
            .map(|p| format!(r"\\{p}"))
            .unwrap_or_else(|| path.strip_prefix(r"\\?\").unwrap_or(path).to_owned());
        self.command(&["loadfile", &path, "replace"])?;
        // Observed properties need not emit again when the next file has the
        // same value. Keep their cache, but do not present the old file as loaded.
        self.state.loaded = false;
        self.state.ended = false;
        self.state.position = 0.0;
        self.state.error = None;
        Ok(())
    }
    pub fn pause(&self, paused: bool) -> Result<(), String> {
        self.command(&["set", "pause", if paused { "yes" } else { "no" }])
    }
    pub fn toggle(&mut self) -> Result<(), String> {
        if self.state.ended {
            self.seek(0.0)?;
            self.pause(false)
        } else {
            self.command(&["cycle", "pause"])
        }
    }
    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        self.state.ended = false;
        let target = clamp_seek(seconds, self.state.duration);
        self.command(&["seek", &format!("{target:.6}"), "absolute+exact"])
    }
    pub fn step(&mut self, forward: bool) -> Result<(), String> {
        self.state.ended = false;
        self.pause(true)?;
        if forward {
            self.command(&["frame-step", "1", "mute"])
        } else {
            self.command(&["frame-back-step"])
        }
    }
    pub fn volume(&self, volume: f32) -> Result<(), String> {
        self.command(&["set", "volume", &volume.clamp(0.0, 100.0).to_string()])
    }
    pub fn mute(&self, muted: bool) -> Result<(), String> {
        self.command(&["set", "mute", if muted { "yes" } else { "no" }])
    }

    pub fn poll(&mut self) {
        // SAFETY: event/payload pointers are valid until the next wait_event.
        // Copy data before advancing; ignore unknown event IDs and formats.
        unsafe {
            loop {
                let event = &*(self.api.wait)(self.handle, 0.0);
                match event.id {
                    0 => break,
                    8 => self.state.loaded = true,
                    5 if event.error < 0 => self.state.error = self.api.check(event.error).err(),
                    7 if !event.data.is_null() => {
                        let end = &*(event.data as *const EndFile);
                        self.state.loaded = false;
                        if end.error < 0 {
                            self.state.error = self.api.check(end.error).err();
                        }
                    }
                    2 if !event.data.is_null() => {
                        let log = &*(event.data as *const Log);
                        if !log.text.is_null() {
                            self.state.error =
                                Some(CStr::from_ptr(log.text).to_string_lossy().trim().to_owned());
                        }
                    }
                    22 if !event.data.is_null() => {
                        let property = &*(event.data as *const Property);
                        let number = if property.format == 5 && !property.data.is_null() {
                            let n = *(property.data as *const f64);
                            n.is_finite().then_some(n)
                        } else {
                            None
                        };
                        let flag = property.format == 3
                            && !property.data.is_null()
                            && *(property.data as *const c_int) != 0;
                        match event.userdata {
                            1 => {
                                if let Some(v) = number {
                                    self.state.position = v.max(0.0);
                                }
                            }
                            2 => self.state.duration = number.unwrap_or(0.0).max(0.0),
                            3 => {
                                if property.format == 3 {
                                    self.state.paused = flag;
                                }
                            }
                            // keep-open can clear eof-reached after pausing;
                            // retain the UI state until an explicit seek/step.
                            4 => self.state.ended |= flag,
                            5 => self.state.seeking = flag,
                            6 => self.display_size[0] = number.unwrap_or(0.0) as u32,
                            7 => self.display_size[1] = number.unwrap_or(0.0) as u32,
                            8 => self.state.fps = number.filter(|v| *v > 0.0),
                            9 => self.state.audio_rate = number,
                            10 => {
                                self.state.audio_output = if property.format == 1
                                    && !property.data.is_null()
                                {
                                    let text = *(property.data as *const *const c_char);
                                    if text.is_null() {
                                        None
                                    } else {
                                        Some(CStr::from_ptr(text).to_string_lossy().into_owned())
                                    }
                                } else {
                                    None
                                };
                            }
                            11 => self.state.av_sync = number,
                            12 => self.rotation = number,
                            _ => {}
                        }
                    }
                    24 => {
                        self.state.error =
                            Some("Playback event queue overflowed. Reopen the video.".into())
                    }
                    _ => {}
                }
            }
        }
        // dwidth/dheight include pixel aspect ratio, but not display rotation.
        // Wait for all metadata before exposing dimensions to project creation.
        [self.state.width, self.state.height] = match self.rotation {
            Some(rotation) if rotation.rem_euclid(180.0) == 90.0 => {
                [self.display_size[1], self.display_size[0]]
            }
            Some(_) => self.display_size,
            None => [0, 0],
        };
    }

    pub fn render(&mut self, size: [usize; 2]) -> Result<Option<RenderedFrame>, String> {
        let resized = size != self.size;
        if size.contains(&0) || size[0] > 1920 || size[1] > 1080 {
            return Err("Invalid preview dimensions.".into());
        }
        if !self.notify.dirty.swap(false, Ordering::AcqRel) && !resized {
            return Ok(None);
        }
        // SAFETY: render context used only on this thread.
        let flags = unsafe { (self.api.render_update)(self.renderer) };
        if flags & 1 == 0 && !resized {
            return Ok(None);
        }
        self.size = size;
        self.pixels.resize(size[0] * size[1], 0);
        let rotation = self.rotation.unwrap_or(0.0).rem_euclid(360.0);
        if rotation % 90.0 != 0.0 {
            return Err("Only display rotations in multiples of 90 degrees are supported.".into());
        }
        let rotation = rotation as u32;
        let render_size = if rotation == 90 || rotation == 270 {
            [size[1], size[0]]
        } else {
            size
        };
        let mut dimensions = [render_size[0] as c_int, render_size[1] as c_int];
        let mut stride = render_size[0] * 4;
        let mut params = [
            Param {
                kind: 17,
                data: dimensions.as_mut_ptr().cast(),
            },
            Param {
                kind: 18,
                data: c"rgb0".as_ptr().cast_mut().cast(),
            },
            Param {
                kind: 19,
                data: (&mut stride as *mut usize).cast(),
            },
            Param {
                kind: 20,
                data: self.pixels.as_mut_ptr().cast(),
            },
            Param {
                kind: 0,
                data: ptr::null_mut(),
            },
        ];
        // SAFETY: aligned allocation covers stride*height; parameters stay alive
        // until rendering completes. Preserve mpv's audio-driven render timing.
        self.api
            .check(unsafe { (self.api.render)(self.renderer, params.as_mut_ptr()) })?;
        let mut rgba = vec![0; self.pixels.len() * 4];
        for (index, pixel) in self.pixels.iter().enumerate() {
            let mut bytes = pixel.to_ne_bytes();
            bytes[3] = 255;
            if rotation == 0 {
                rgba[index * 4..index * 4 + 4].copy_from_slice(&bytes);
                continue;
            }
            let x = index % render_size[0];
            let y = index / render_size[0];
            let (x, y) = match rotation {
                90 => (render_size[1] - 1 - y, x),
                180 => (render_size[0] - 1 - x, render_size[1] - 1 - y),
                270 => (y, render_size[0] - 1 - x),
                _ => (x, y),
            };
            let target = (y * size[0] + x) * 4;
            rgba[target..target + 4].copy_from_slice(&bytes);
        }
        Ok(Some(RenderedFrame { size, rgba }))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: remove callbacks and renderer before core destruction, retaining
        // callback storage and the DLL until shutdown completes.
        unsafe {
            (self.api.wakeup)(self.handle, None, ptr::null_mut());
            if !self.renderer.is_null() {
                (self.api.render_callback)(self.renderer, None, ptr::null_mut());
                (self.api.render_free)(self.renderer);
            }
            (self.api.destroy)(self.handle);
        }
    }
}

pub fn clamp_seek(seconds: f64, duration: f64) -> f64 {
    if !seconds.is_finite() || !duration.is_finite() {
        return 0.0;
    }
    seconds.clamp(0.0, duration.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeking_is_bounded_and_never_nan() {
        assert_eq!(clamp_seek(-5.0, 10.0), 0.0);
        assert_eq!(clamp_seek(15.0, 10.0), 10.0);
        assert_eq!(clamp_seek(f64::NAN, 10.0), 0.0);
        assert_eq!(clamp_seek(1.0, 0.0), 0.0);
    }
}
