use eframe::egui;
use std::path::PathBuf;
use video_annotations::{
    media::{MediaWorker, Request},
    project::Project,
};

pub struct VideoApp {
    worker: MediaWorker,
    project: Option<Project>,
    texture: Option<egui::TextureHandle>,
    request_id: u64,
    position: f64,
    shown_position: f64,
    busy: bool,
    error: Option<String>,
}

impl VideoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, source: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let context = cc.egui_ctx.clone();
        let mut app = Self {
            worker: MediaWorker::start(move || context.request_repaint()),
            project: None,
            texture: None,
            request_id: 0,
            position: 0.0,
            shown_position: 0.0,
            busy: false,
            error: None,
        };
        if let Some(path) = source {
            app.open(path);
        }
        app
    }

    fn open(&mut self, path: PathBuf) {
        self.project = None;
        self.texture = None;
        self.position = 0.0;
        self.shown_position = 0.0;
        self.send(path);
    }

    fn send(&mut self, path: PathBuf) {
        self.request_id += 1;
        self.error = None;
        self.busy = true;
        let request = Request {
            id: self.request_id,
            path,
            info: self.project.as_ref().map(|p| p.video.clone()),
            seconds: self.position,
        };
        if self.worker.requests.send(request).is_err() {
            self.busy = false;
            self.error = Some("The video worker stopped. Please restart the application.".into());
        }
    }

    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(response) = self.worker.responses.try_recv() {
            // A previous file or seek must never replace the newest request.
            if response.id != self.request_id {
                continue;
            }
            self.busy = false;
            match response.result {
                Ok((info, frame)) => {
                    self.shown_position = frame.requested_seconds;
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                        [frame.width, frame.height],
                        &frame.rgba,
                    );
                    if let Some(texture) = self.texture.as_mut() {
                        texture.set(pixels, egui::TextureOptions::LINEAR);
                    } else {
                        self.texture = Some(ctx.load_texture(
                            "video-frame",
                            pixels,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    if self.project.is_none() {
                        self.project = Some(Project::new(info));
                    }
                }
                Err(message) => self.error = Some(message),
            }
        }
    }
}

impl eframe::App for VideoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.open(path);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("VideoAnnotations");
                ui.label(egui::RichText::new("TECHNICAL PREVIEW").small().color(egui::Color32::from_rgb(108, 196, 210)));
                if ui.button("Open video...").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Video", &["mp4", "mov", "mkv", "avi", "webm", "m4v", "mpeg", "ts"])
                        .add_filter("All files", &["*"]).pick_file() {
                    self.open(path);
                }
                if self.busy { ui.spinner(); ui.label("Decoding frame..."); }
            });
            ui.separator();
            if let Some(project) = &self.project {
                let video = &project.video;
                ui.label(video.path.file_name().unwrap_or_default().to_string_lossy());
                let fps = video.fps.map(|v| format!("{v:.3} fps average")).unwrap_or_else(|| "Unknown frame rate".into());
                ui.label(format!("{} x {} source  |  {:.2} s  |  {fps}", video.width, video.height, video.duration));
            } else {
                ui.label("One video. A workspace for your annotations.");
                ui.label("Open a local video or drop one into this window.");
            }
            if let Some(error) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
            ui.add_space(8.0);
            let preview_size = egui::vec2(ui.available_width(), (ui.available_height() - 135.0).max(100.0));
            let (rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(12, 15, 21));
            if let Some(texture) = &self.texture {
                let size = texture.size_vec2();
                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                let image_rect = egui::Rect::from_center_size(rect.center(), size * scale);
                ui.painter().image(texture.id(), image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            } else {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER,
                    if self.busy { "Opening video..." } else { "Your video preview appears here" },
                    egui::FontId::proportional(20.0), egui::Color32::GRAY);
            }
            ui.add_space(8.0);
            if let Some(project) = &self.project {
                let video = project.video.clone();
                let mut seek = false;
                ui.horizontal(|ui| {
                    for (label, delta) in [("-5 s", -5.0), ("-1 s", -1.0), ("+1 s", 1.0), ("+5 s", 5.0)] {
                        if ui.button(label).clicked() {
                            self.position = video.clamp_time(self.position + delta);
                            seek = true;
                        }
                    }
                    ui.label(format!("Seek: {:.3} s  |  Displayed request: {:.3} s", self.position, self.shown_position));
                });
                ui.spacing_mut().slider_width = (ui.available_width() - 130.0).max(100.0);
                seek |= ui.add(egui::Slider::new(&mut self.position, 0.0..=video.last_seek_time())
                    .text("seconds").fixed_decimals(3)).changed();
                if seek { self.send(video.path); }
            }
            ui.separator();
            ui.label("Step 1 / Open, inspect and seek");
            ui.small("Playback, audio and frame-by-frame navigation arrive in step 2. Annotations arrive in step 3.");
        });
    }
}
