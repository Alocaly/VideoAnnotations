use eframe::egui;
use std::{path::PathBuf, sync::Arc, time::Duration};
use video_annotations::{media::VideoInfo, playback::Player, project::Project};

pub struct VideoApp {
    player: Option<Player>,
    project: Option<Project>,
    source: Option<PathBuf>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    fullscreen: bool,
    volume: f32,
    muted: bool,
    scrub: Option<f64>,
    context: egui::Context,
}

impl VideoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, source: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self {
            player: None,
            project: None,
            source: None,
            texture: None,
            error: None,
            fullscreen: false,
            volume: 70.0,
            muted: false,
            scrub: None,
            context: cc.egui_ctx.clone(),
        };
        if let Some(path) = source {
            app.open(path);
        }
        app
    }

    fn open(&mut self, path: PathBuf) {
        // A fresh core isolates asynchronous file events and stops the old audio.
        self.player = None;
        self.project = None;
        self.texture = None;
        self.scrub = None;
        self.error = None;
        self.source = Some(path.clone());
        let context = self.context.clone();
        let result = (|| {
            let mut player = Player::new(Arc::new(move || context.request_repaint()))?;
            player.volume(self.volume)?;
            player.mute(self.muted)?;
            player.load(&path)?;
            Ok::<_, String>(player)
        })();
        match result {
            Ok(player) => self.player = Some(player),
            Err(e) => self.error = Some(e),
        }
    }

    fn act(&mut self, action: impl FnOnce(&mut Player) -> Result<(), String>) {
        if let Some(player) = &mut self.player
            && let Err(e) = action(player)
        {
            self.error = Some(e);
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.context
            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn picker(&mut self) {
        self.act(|p| p.pause(true));
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open video")
            .add_filter(
                "Video",
                &["mp4", "mov", "mkv", "avi", "webm", "m4v", "mpeg", "ts"],
            )
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.open(path);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.fullscreen {
            self.toggle_fullscreen();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.toggle_fullscreen();
        }
        if ctx.text_edit_focused() {
            return;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.picker();
        }
        if !self.player.as_ref().is_some_and(|p| p.state.loaded) {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
            self.act(Player::toggle);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowLeft)) {
            self.act(|p| p.step(false));
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowLeft)) {
            self.act(|p| p.seek(p.state.position - 5.0));
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)) {
            self.act(|p| p.seek(p.state.position - 1.0));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowRight)) {
            self.act(|p| p.step(true));
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowRight)) {
            self.act(|p| p.seek(p.state.position + 5.0));
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)) {
            self.act(|p| p.seek(p.state.position + 1.0));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Comma)) {
            self.act(|p| p.step(false));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Period)) {
            self.act(|p| p.step(true));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.muted = !self.muted;
            let muted = self.muted;
            self.act(|p| p.mute(muted));
        }
    }

    fn update_player(&mut self, ctx: &egui::Context) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        player.poll();
        if let Some(error) = player.state.error.take() {
            self.error = Some(error);
        }
        let s = &player.state;
        if s.loaded && s.width > 0 && s.height > 0 && self.project.is_none() {
            self.project = Some(Project::new(VideoInfo {
                path: self.source.clone().unwrap(),
                width: s.width,
                height: s.height,
                duration: s.duration,
                fps: s.fps,
            }));
        }
        if let Some(project) = self.project.as_mut() {
            project.video.duration = s.duration;
            project.video.fps = s.fps;
        }
        let ratio = if s.width > 0 && s.height > 0 {
            s.width as f64 / s.height as f64
        } else {
            16.0 / 9.0
        };
        let width = 1280_usize.min((720.0 * ratio).round().max(1.0) as usize);
        let height = ((width as f64 / ratio).round() as usize).clamp(1, 720);
        match player.render([width, height]) {
            Ok(Some(frame)) => {
                let image = egui::ColorImage::from_rgba_unmultiplied(frame.size, &frame.rgba);
                if let Some(texture) = self.texture.as_mut() {
                    texture.set(image, egui::TextureOptions::LINEAR);
                } else {
                    self.texture =
                        Some(ctx.load_texture("video", image, egui::TextureOptions::LINEAR));
                }
            }
            Err(e) => self.error = Some(e),
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(if player.state.paused {
            100
        } else {
            16
        }));
    }
}

impl eframe::App for VideoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.update_player(&ctx);
        self.shortcuts(&ctx);
        if let Some(path) = ctx.input(|i| {
            i.raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        }) {
            self.open(path);
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("VideoAnnotations");
                if ui.button("Open video...").on_hover_text("Ctrl+O").clicked() { self.picker(); }
                if ui.button(if self.fullscreen { "Exit fullscreen" } else { "Fullscreen" }).on_hover_text("F11 / Esc").clicked() { self.toggle_fullscreen(); }
            });
            if let Some(path) = &self.source { ui.label(path.file_name().unwrap_or_default().to_string_lossy()); }
            else { ui.label("Open a local video or drop one into this window."); }
            let state = self.player.as_ref().map(|p| p.state.clone());
            if let Some(s) = &state {
                ui.horizontal(|ui| {
                    ui.label(format!("{} x {}  |  {}  |  {}", s.width, s.height,
                        s.fps.map(|f| format!("{f:.3} fps")).unwrap_or_else(|| "Variable / unknown fps".into()),
                        if s.audio_output.is_some() { "Audio enabled" } else { "No active audio output" }));
                    if !s.loaded && self.error.is_none() { ui.spinner(); ui.label("Opening video..."); }
                    else if s.seeking { ui.spinner(); ui.label("Seeking..."); }
                });
            }
            if let Some(error) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
            ui.separator();
            let size = egui::vec2(ui.available_width(), (ui.available_height() - 150.0).max(100.0));
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(12,15,21));
            if let Some(texture) = &self.texture {
                let native = texture.size_vec2();
                let scale = (rect.width()/native.x).min(rect.height()/native.y);
                ui.painter().image(texture.id(), egui::Rect::from_center_size(rect.center(), native*scale),
                    egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),egui::Color32::WHITE);
            } else {
                ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Your video appears here",
                    egui::FontId::proportional(20.0),egui::Color32::GRAY);
            }
            if response.double_clicked() { self.toggle_fullscreen(); }
            ui.add_space(8.0);
            if let Some(s) = &state {
                ui.add_enabled_ui(s.loaded, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(if s.ended { "Replay" } else if s.paused { "Play" } else { "Pause" }).on_hover_text("Space").clicked() { self.act(Player::toggle); }
                        for (label,delta) in [("-5 s",-5.0),("-1 s",-1.0)] {
                            if ui.button(label).clicked() { self.act(|p| p.seek(s.position+delta)); }
                        }
                        if ui.button("-1 frame").on_hover_text("Ctrl+Left or ,").clicked() { self.act(|p| p.step(false)); }
                        if ui.button("+1 frame").on_hover_text("Ctrl+Right or .").clicked() { self.act(|p| p.step(true)); }
                        for (label,delta) in [("+1 s",1.0),("+5 s",5.0)] {
                            if ui.button(label).clicked() { self.act(|p| p.seek(s.position+delta)); }
                        }
                        ui.label(format!("{} / {}", timecode(s.position), timecode(s.duration)));
                    });
                    let mut position = self.scrub.unwrap_or(s.position);
                    ui.spacing_mut().slider_width = (ui.available_width()-150.0).max(100.0);
                    let slider = ui.add(egui::Slider::new(&mut position,0.0..=s.duration.max(0.0)).text("seconds").fixed_decimals(3));
                    if slider.changed() {
                        self.scrub = Some(position);
                        if !slider.dragged() { self.act(|p| p.seek(position)); self.scrub = None; }
                    }
                    if slider.drag_stopped() { self.act(|p| p.seek(position)); self.scrub = None; }
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut self.muted,"Mute").changed() { let muted=self.muted; self.act(|p| p.mute(muted)); }
                        ui.spacing_mut().slider_width=120.0;
                        if ui.add(egui::Slider::new(&mut self.volume,0.0..=100.0).text("Volume")).changed() { let volume=self.volume; self.act(|p| p.volume(volume)); }
                    });
                });
            }
            ui.separator();
            ui.small("Space: play/pause  |  Left/Right: 1 s  |  Shift+Left/Right: 5 s  |  Ctrl+Left/Right: one frame  |  F11: fullscreen  |  Esc: exit  |  M: mute");
        });
    }
}

fn timecode(seconds: f64) -> String {
    let millis = (seconds.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        millis / 3_600_000,
        (millis / 60_000) % 60,
        (millis / 1000) % 60,
        millis % 1000
    )
}
