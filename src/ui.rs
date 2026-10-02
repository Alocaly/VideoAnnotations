use eframe::egui;
use std::{path::PathBuf, sync::Arc, time::Duration};
use video_annotations::{
    document::{self, History},
    media::VideoInfo,
    playback::{PlaybackState, Player},
    project::Project,
};

pub struct VideoApp {
    gif_width: u32,
    gif_fps: u32,
    gif_dialog: Option<GifExportOptions>,
    show_shortcuts_dialog: bool,
    export: Option<video_annotations::export::Job>,
    export_progress: f32,
    export_message: Option<String>,
    project_path: Option<PathBuf>,
    saved: Option<Project>,
    history: History,
    loading: Option<Loading>,
    pending: Option<Pending>,
    allow_close: bool,
    editor: crate::editor::Editor,
    player: Option<Player>,
    project: Option<Project>,
    source: Option<PathBuf>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    fullscreen: bool,
    show_video_info: bool,
    volume: f32,
    muted: bool,
    scrub: Option<f64>,
    context: egui::Context,
}

impl VideoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, source: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self {
            gif_width: 640,
            gif_fps: 15,
            gif_dialog: None,
            show_shortcuts_dialog: false,
            export: None,
            export_progress: 0.0,
            export_message: None,
            project_path: None,
            saved: None,
            history: History::default(),
            loading: None,
            pending: None,
            allow_close: false,
            editor: Default::default(),
            player: None,
            project: None,
            source: None,
            texture: None,
            error: None,
            fullscreen: false,
            show_video_info: true,
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
        if self.dirty() {
            self.act(|p| p.pause(true));
            self.pending = Some(Pending::Open(path));
        } else {
            self.load_path(path);
        }
    }

    fn dirty(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|p| self.saved.as_ref() != Some(p))
    }

    fn header_title(&self) -> String {
        let Some(project) = &self.project else {
            return "VideoAnnotations".into();
        };
        let project_name = self
            .project_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "Untitled.vannot".into(),
                |name| name.to_string_lossy().into_owned(),
            );
        let video_name = self
            .source
            .as_ref()
            .unwrap_or(&project.video.path)
            .file_name()
            .map_or_else(
                || "Unknown video".into(),
                |name| name.to_string_lossy().into_owned(),
            );
        let unsaved = if self.dirty() { " *" } else { "" };
        format!("VideoAnnotations ( {project_name}{unsaved} - {video_name} )")
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(self.header_title());
            ui.menu_button("File", |ui| {
                if ui.button("Open video…    Ctrl+O").clicked() {
                    ui.close();
                    self.picker();
                }
                if ui.button("Open project…    Ctrl+Shift+O").clicked() {
                    ui.close();
                    self.project_picker();
                }
                ui.separator();
                if ui
                    .add_enabled(self.project.is_some(), egui::Button::new("Save    Ctrl+S"))
                    .clicked()
                {
                    ui.close();
                    self.save(false);
                }
                if ui
                    .add_enabled(
                        self.project.is_some(),
                        egui::Button::new("Save as…    Ctrl+Shift+S"),
                    )
                    .clicked()
                {
                    ui.close();
                    self.save(true);
                }
            });
            ui.menu_button("Export", |ui| {
                if ui
                    .add_enabled(self.project.is_some(), egui::Button::new("Export MP4…"))
                    .on_hover_text("H.264 / AAC, SDR; up to 32 annotations and 9 megapixels")
                    .clicked()
                {
                    ui.close();
                    self.start_export(video_annotations::export::Format::Mp4);
                }
                if ui
                    .add_enabled(self.project.is_some(), egui::Button::new("Export GIF…"))
                    .clicked()
                {
                    ui.close();
                    self.act(|p| p.pause(true));
                    self.gif_dialog = Some(GifExportOptions {
                        width: self.gif_width,
                        fps: self.gif_fps,
                    });
                }
            });
            ui.menu_button("Misc", |ui| {
                if ui.button("Full screen    F11").clicked() {
                    ui.close();
                    self.toggle_fullscreen();
                }
                if ui
                    .checkbox(
                        &mut self.show_video_info,
                        "Show video information    Ctrl+I",
                    )
                    .clicked()
                {
                    ui.close();
                }
                ui.separator();
                if ui.button("Keyboard shortcuts…").clicked() {
                    ui.close();
                    self.show_shortcuts_dialog = true;
                }
            });
            if ui
                .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                self.restore_history(false);
            }
            if ui
                .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
                .clicked()
            {
                self.restore_history(true);
            }
        });
    }

    fn gif_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(options) = self.gif_dialog.as_mut() else {
            return;
        };
        let mut confirm = false;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if !cancel {
            egui::Modal::new(egui::Id::new("gif-export-options")).show(ctx, |ui| {
                ui.heading("Export GIF");
                ui.label("Choose the GIF settings before selecting a destination.");
                egui::Grid::new("gif-export-settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Max width");
                        ui.add(
                            egui::DragValue::new(&mut options.width)
                                .range(64..=1920)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("Frame rate");
                        ui.add(
                            egui::DragValue::new(&mut options.fps)
                                .range(1..=30)
                                .suffix(" fps"),
                        );
                        ui.end_row();
                    });
                ui.small("No audio • loops forever");
                ui.horizontal(|ui| {
                    confirm = ui.button("Continue to Save…").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        }
        if cancel {
            self.gif_dialog = None;
        } else if confirm {
            let options = self.gif_dialog.take().unwrap();
            self.gif_width = options.width;
            self.gif_fps = options.fps;
            self.start_export(video_annotations::export::Format::Gif {
                width: options.width,
                fps: options.fps,
            });
        }
    }

    fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts_dialog {
            return;
        }
        let mut close = false;
        egui::Modal::new(egui::Id::new("keyboard-shortcuts")).show(ctx, |ui| {
            ui.heading("Keyboard shortcuts");
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    shortcut_section(
                        ui,
                        "File and editing",
                        &[
                            ("Open video", "Ctrl+O"),
                            ("Open project", "Ctrl+Shift+O"),
                            ("Save project", "Ctrl+S"),
                            ("Save as", "Ctrl+Shift+S"),
                            ("Undo", "Ctrl+Z"),
                            ("Redo", "Ctrl+Y / Ctrl+Shift+Z"),
                        ],
                    );
                    ui.add_space(8.0);
                    shortcut_section(
                        ui,
                        "Playback",
                        &[
                            ("Play / pause", "Space"),
                            ("Seek 1 second", "Left / Right"),
                            ("Seek 5 seconds", "Shift+Left / Right"),
                            ("Step one frame", "Ctrl+Left / Right, or , / ."),
                            ("Mute", "M"),
                        ],
                    );
                    ui.add_space(8.0);
                    shortcut_section(
                        ui,
                        "View and annotations",
                        &[
                            ("Full screen", "F11"),
                            ("Exit full screen", "Esc"),
                            ("Show / hide video information", "Ctrl+I"),
                            ("Delete selected annotation", "Delete"),
                            ("Cancel current gesture / selection", "Esc"),
                        ],
                    );
                });
            ui.separator();
            close = ui.button("Close").clicked();
        });
        if close {
            self.show_shortcuts_dialog = false;
        }
    }

    fn start_export(&mut self, format: video_annotations::export::Format) {
        self.act(|p| p.pause(true));
        let Some(project) = &self.project else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(format!(
                "Export annotated {}",
                format.extension().to_uppercase()
            ))
            .add_filter(format.extension().to_uppercase(), &[format.extension()])
            .set_file_name(format!("annotated.{}", format.extension()))
            .save_file()
        else {
            return;
        };
        self.export = Some(video_annotations::export::Job::start_with_format(
            project.clone(),
            path,
            format,
        ));
        self.export_progress = 0.0;
        self.export_message = Some("Preparing export…".into());
        self.error = None;
    }

    fn poll_export(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.export else {
            return;
        };
        let mut finished = false;
        for event in job.events.try_iter() {
            match event {
                video_annotations::export::Event::Progress { fraction, message } => {
                    self.export_progress = fraction;
                    self.export_message = Some(message);
                }
                video_annotations::export::Event::Finished(result) => {
                    finished = true;
                    self.export_message = Some(match result {
                        Ok(path) => format!("Export complete: {}", path.display()),
                        Err(error) => error,
                    });
                }
            }
        }
        if finished {
            self.export = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn load_path(&mut self, path: PathBuf) {
        self.act(|p| p.pause(true));
        self.error = None;
        let is_project = path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("vannot"));
        let mut stored = if is_project {
            match document::load(&path) {
                Ok(project) => Some(project),
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            }
        } else {
            None
        };
        let saved = stored.clone();
        let mut source = stored
            .as_ref()
            .map_or_else(|| path.clone(), |p| p.video.path.clone());
        if let Some(project) = stored.as_mut()
            && !source.is_file()
        {
            let Some(replacement) = rfd::FileDialog::new()
                .set_title("Source video missing — locate the original video")
                .pick_file()
            else {
                return;
            };
            source = replacement;
            project.video.path = source.clone();
        }
        let context = self.context.clone();
        let result = (|| {
            let mut player = Player::new(Arc::new(move || context.request_repaint()))?;
            player.volume(self.volume)?;
            player.mute(self.muted)?;
            player.load(&source)?;
            Ok::<_, String>(player)
        })();
        match result {
            Ok(player) => {
                self.loading = Some(Loading {
                    started: std::time::Instant::now(),
                    player,
                    source,
                    project_path: is_project.then_some(path),
                    stored,
                    saved,
                })
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn save(&mut self, save_as: bool) -> bool {
        let Some(project) = self.project.as_ref() else {
            return false;
        };
        let path = if save_as || self.project_path.is_none() {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save project")
                .add_filter("VideoAnnotations project", &["vannot"]);
            if let Some(path) = &self.project_path {
                if let Some(parent) = path.parent() {
                    dialog = dialog.set_directory(parent);
                }
                dialog =
                    dialog.set_file_name(path.file_name().unwrap_or_default().to_string_lossy());
            } else {
                dialog = dialog.set_file_name("Untitled.vannot");
            }
            let Some(path) = dialog.save_file() else {
                return false;
            };
            // Preserve the exact path for which the native dialog confirmed overwrite.
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("vannot"))
            {
                self.error = Some("Use the .vannot extension to save a project.".into());
                return false;
            }
            path
        } else {
            self.project_path.clone().unwrap()
        };
        match document::save(project, &path) {
            Ok(()) => {
                self.history.observe(&project.annotations, false);
                self.saved = Some(project.clone());
                self.project_path = Some(path);
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    fn restore_history(&mut self, redo: bool) {
        if let Some(project) = self.project.as_mut() {
            self.editor.cancel(project);
            self.history.observe(&project.annotations, false);
            let state = if redo {
                self.history.redo()
            } else {
                self.history.undo()
            };
            if let Some(annotations) = state {
                project.annotations = annotations;
                self.editor = Default::default();
                self.act(|p| p.pause(true));
            }
        }
    }

    fn project_picker(&mut self) {
        self.act(|p| p.pause(true));
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open project")
            .add_filter("VideoAnnotations project", &["vannot"])
            .pick_file()
        {
            self.open(path);
        }
    }

    fn poll_loading(&mut self) {
        let Some(loading) = self.loading.as_mut() else {
            return;
        };
        loading.player.poll();
        if let Some(error) = loading.player.state.error.take() {
            self.error = Some(error);
            self.loading = None;
            return;
        }
        let s = &loading.player.state;
        if !s.loaded || s.width == 0 || s.height == 0 || s.duration <= 0.0 {
            if loading.started.elapsed() > Duration::from_secs(20) {
                self.error = Some(
                    "Timed out opening the source video. The current project was kept.".into(),
                );
                self.loading = None;
            }
            return;
        }
        if let Some(project) = &loading.stored
            && (project.video.width != s.width
                || project.video.height != s.height
                || (project.video.duration - s.duration).abs() > 0.05)
        {
            self.error = Some("Source dimensions or duration differ from the saved project. Locate the original video; the current project was kept.".into());
            self.loading = None;
            return;
        }
        let loading = self.loading.take().unwrap();
        let s = &loading.player.state;
        let project = loading.stored.unwrap_or_else(|| {
            Project::new(VideoInfo {
                path: loading.source.clone(),
                width: s.width,
                height: s.height,
                duration: s.duration,
                fps: s.fps,
            })
        });
        self.history = History::new(&project.annotations);
        self.project = Some(project);
        self.saved = loading.saved;
        self.project_path = loading.project_path;
        self.source = Some(loading.source);
        self.player = Some(loading.player);
        self.editor = Default::default();
        self.texture = None;
        self.scrub = None;
        self.error = None;
    }

    fn act(&mut self, action: impl FnOnce(&mut Player) -> Result<(), String>) {
        if let Some(player) = &mut self.player
            && let Err(e) = action(player)
        {
            self.error = Some(e);
        }
    }

    fn toggle_fullscreen(&mut self) {
        if let Some(project) = &mut self.project {
            self.editor.cancel(project);
        }
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
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::S)
        }) {
            self.save(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save(false);
        }
        if ctx.text_edit_focused() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::I)) {
            self.show_video_info = !self.show_video_info;
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
        }) || ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Y))
        {
            self.restore_history(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
            self.restore_history(false);
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::O)
        }) {
            self.project_picker();
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
        self.poll_loading();
        if self.loading.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
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

    fn video_view(&mut self, ui: &mut egui::Ui, size: egui::Vec2, fullscreen: bool) {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
        let state = self.player.as_ref().map(|p| p.state.clone());
        let time = self
            .scrub
            .unwrap_or_else(|| state.as_ref().map_or(0.0, |s| s.position));
        let editing = !fullscreen && state.as_ref().is_none_or(|s| s.paused);
        let mut pause = false;
        if let Some(texture) = &self.texture {
            let native = texture.size_vec2();
            let scale = (rect.width() / native.x).min(rect.height() / native.y);
            let image_rect = egui::Rect::from_center_size(rect.center(), native * scale);
            ui.painter().image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            if let Some(project) = self.project.as_mut() {
                if editing {
                    pause = self.editor.canvas(ui, &response, image_rect, project, time);
                } else {
                    // Viewing never draws editor bounds or resize handles.
                    paint_preview_annotations(ui.painter(), project, image_rect, time);
                    pause = !fullscreen && response.clicked();
                }
            }
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Your video appears here",
                egui::FontId::proportional(20.0),
                egui::Color32::GRAY,
            );
        }
        if pause {
            self.act(|p| p.pause(true));
        }
        if self.show_video_info
            && let Some(state) = state.as_ref().filter(|s| s.loaded)
        {
            paint_video_information(ui.painter(), rect, state);
        }
        if state.as_ref().is_some_and(|s| s.seeking) {
            paint_seeking_indicator(ui, rect);
        }
        if response.double_clicked() && (fullscreen || self.editor.can_toggle_fullscreen()) {
            self.toggle_fullscreen();
        }
    }
}

impl eframe::App for VideoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_export(&ctx);
        if self.show_shortcuts_dialog
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.show_shortcuts_dialog = false;
        }
        if self.editor.renaming()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.editor.dismiss_rename();
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.gif_dialog = None;
            self.show_shortcuts_dialog = false;
            self.editor.dismiss_rename();
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.export.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.error = Some("Wait for export to finish or cancel it before closing.".into());
        } else if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.loading = None;
            self.act(|p| p.pause(true));
            self.pending = Some(Pending::Close);
        }
        self.update_player(&ctx);
        if self.pending.is_none()
            && self.loading.is_none()
            && self.export.is_none()
            && self.gif_dialog.is_none()
            && !self.show_shortcuts_dialog
            && !self.editor.renaming()
        {
            self.shortcuts(&ctx);
        }
        if self.pending.is_none()
            && self.export.is_none()
            && self.loading.is_none()
            && self.gif_dialog.is_none()
            && !self.show_shortcuts_dialog
            && !self.editor.renaming()
            && let Some(path) = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .first()
                    .map(|file| file.path().to_path_buf())
            })
        {
            self.open(path);
        }
        let fullscreen = self.fullscreen;
        let panel = if fullscreen {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
        } else {
            egui::CentralPanel::default()
        };
        panel.show(ui, |ui| {
            if fullscreen {
                self.video_view(ui, ui.available_size(), true);
                return;
            }
            if let Some(message) = &self.export_message {
                ui.label(message);
            }
            if let Some(job) = &self.export {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(self.export_progress)
                            .desired_width(240.0)
                            .show_percentage(),
                    );
                    if ui.button("Cancel export").clicked() {
                        job.cancel();
                    }
                });
                ui.disable();
            }
            if self.loading.is_some() {
                ui.label(
                    "Opening source video… Current project is kept until validation succeeds.",
                );
            }
            if self.pending.is_some() || self.loading.is_some() || self.editor.renaming() {
                ui.disable();
            }
            self.top_bar(ui);
            if self.project.is_none() {
                ui.label("Open a local video or drop one into this window.");
            }
            let state = self.player.as_ref().map(|p| p.state.clone());
            if state.as_ref().is_some_and(|s| !s.loaded) && self.error.is_none() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Opening video...");
                });
            }
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if let Some(project) = self.project.as_mut() {
                let pause = egui::Panel::right("annotation-properties")
                    .default_size(280.0)
                    .min_size(220.0)
                    .max_size(360.0)
                    .resizable(true)
                    .show(ui, |ui| self.editor.properties(ui, project))
                    .inner;
                if pause {
                    self.act(|p| p.pause(true));
                }
                if self.editor.tools(ui) {
                    self.act(|p| p.pause(true));
                }
            }
            if let Some(project) = self.project.as_mut() {
                // Keep room for the player controls and a usable video preview.
                let max_timeline_height = (ui.available_height() - 180.0).max(180.0);
                let action = egui::Panel::bottom("annotation-timeline")
                    .default_size(200.0)
                    .min_size(180.0)
                    .max_size(max_timeline_height)
                    .resizable(true)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        self.editor.timeline(
                            ui,
                            project,
                            self.scrub
                                .unwrap_or_else(|| state.as_ref().map_or(0.0, |s| s.position)),
                        )
                    })
                    .inner;
                if action.pause {
                    self.act(|p| p.pause(true));
                }
                if let Some(time) = action.seek {
                    self.act(|p| p.seek(time));
                }
            }
            if let Some(s) = &state {
                egui::Panel::bottom("player-controls")
                    .default_size(100.0)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(s.loaded, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .button(if s.ended {
                                        "Replay"
                                    } else if s.paused {
                                        "Play"
                                    } else {
                                        "Pause"
                                    })
                                    .on_hover_text("Space")
                                    .clicked()
                                {
                                    self.act(Player::toggle);
                                }
                                for (label, delta) in [("-5 s", -5.0), ("-1 s", -1.0)] {
                                    if ui.button(label).clicked() {
                                        self.act(|p| p.seek(s.position + delta));
                                    }
                                }
                                if ui
                                    .button("-1 frame")
                                    .on_hover_text("Ctrl+Left or ,")
                                    .clicked()
                                {
                                    self.act(|p| p.step(false));
                                }
                                if ui
                                    .button("+1 frame")
                                    .on_hover_text("Ctrl+Right or .")
                                    .clicked()
                                {
                                    self.act(|p| p.step(true));
                                }
                                for (label, delta) in [("+1 s", 1.0), ("+5 s", 5.0)] {
                                    if ui.button(label).clicked() {
                                        self.act(|p| p.seek(s.position + delta));
                                    }
                                }
                                ui.label(format!(
                                    "{} / {}",
                                    timecode(self.scrub.unwrap_or(s.position)),
                                    timecode(s.duration)
                                ));
                            });
                            if let Some(position) =
                                seek_slider(ui, &mut self.scrub, s.position, s.duration)
                            {
                                if !s.paused {
                                    self.act(|p| p.pause(true));
                                }
                                self.act(|p| p.seek(position));
                            }
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut self.muted, "Mute").changed() {
                                    let muted = self.muted;
                                    self.act(|p| p.mute(muted));
                                }
                                ui.spacing_mut().slider_width = 120.0;
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.volume, 0.0..=100.0)
                                            .text("Volume"),
                                    )
                                    .changed()
                                {
                                    let volume = self.volume;
                                    self.act(|p| p.volume(volume));
                                }
                            });
                        })
                    });
            }
            ui.separator();
            let available = ui.available_size();
            self.video_view(
                ui,
                egui::vec2(available.x.max(1.0), available.y.max(1.0)),
                false,
            );
        });
        if let Some(project) = &self.project {
            let editing = ctx.input(|i| i.pointer.any_down()) || ctx.text_edit_focused();
            self.history.observe(&project.annotations, editing);
        }
        if let Some(project) = self.project.as_mut()
            && self.editor.rename_dialog(&ctx, project)
        {
            self.act(|p| p.pause(true));
        }
        self.gif_export_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        if self.pending.is_some() {
            let mut discard = false;
            let mut cancel = false;
            let mut save = false;
            egui::Modal::new(egui::Id::new("discard-annotations")).show(&ctx, |ui| {
                ui.heading("Unsaved changes");
                ui.label("Save the current project before continuing?");
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                ui.horizontal(|ui| {
                    save = ui.button("Save and continue").clicked();
                    discard = ui.button("Discard changes").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if cancel {
                self.pending = None;
            }
            if discard || (save && self.save(false)) {
                match self.pending.take() {
                    Some(Pending::Open(path)) => self.load_path(path),
                    Some(Pending::Close) => {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    None => {}
                }
            }
        }
    }
}

enum Pending {
    Open(PathBuf),
    Close,
}

fn shortcut_section(ui: &mut egui::Ui, title: &str, rows: &[(&str, &str)]) {
    ui.strong(title);
    egui::Grid::new(title)
        .num_columns(2)
        .spacing([24.0, 4.0])
        .show(ui, |ui| {
            for &(action, keys) in rows {
                ui.label(action);
                ui.label(egui::RichText::new(keys).monospace());
                ui.end_row();
            }
        });
}

fn video_information_text(state: &PlaybackState) -> String {
    format!(
        "{} × {}\n{}\n{}",
        state.width,
        state.height,
        state
            .fps
            .map(|fps| format!("{fps:.3} fps"))
            .unwrap_or_else(|| "Variable / unknown fps".into()),
        if state.audio_output.is_some() {
            "Audio enabled"
        } else {
            "No active audio output"
        }
    )
}

fn paint_video_information(painter: &egui::Painter, video_rect: egui::Rect, state: &PlaybackState) {
    let text = painter.layout_no_wrap(
        video_information_text(state),
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );
    let padding = egui::vec2(8.0, 6.0);
    let size = text.size() + padding * 2.0;
    let origin = video_rect.right_top() + egui::vec2(-size.x - 8.0, 8.0);
    let rect = egui::Rect::from_min_size(origin, size);
    let painter = painter.with_clip_rect(video_rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(180));
    painter.galley(rect.min + padding, text, egui::Color32::WHITE);
}

fn paint_seeking_indicator(ui: &mut egui::Ui, video_rect: egui::Rect) {
    let text = ui.painter().layout_no_wrap(
        "Seeking…".into(),
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );
    let padding = egui::vec2(8.0, 6.0);
    let spinner_size = 16.0;
    let gap = 6.0;
    let size = egui::vec2(
        padding.x * 2.0 + spinner_size + gap + text.size().x,
        padding.y * 2.0 + spinner_size.max(text.size().y),
    );
    if video_rect.width() < size.x + 16.0 || video_rect.height() < size.y + 16.0 {
        return;
    }
    let rect = egui::Rect::from_min_size(
        video_rect.right_bottom() - size - egui::vec2(8.0, 8.0),
        size,
    );
    ui.painter().with_clip_rect(video_rect).rect_filled(
        rect,
        4.0,
        egui::Color32::from_black_alpha(180),
    );
    let spinner_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + padding.x, rect.center().y - spinner_size / 2.0),
        egui::vec2(spinner_size, spinner_size),
    );
    ui.place(spinner_rect, egui::Spinner::new().size(spinner_size));
    ui.painter().with_clip_rect(video_rect).galley(
        egui::pos2(
            spinner_rect.right() + gap,
            rect.center().y - text.size().y / 2.0,
        ),
        text,
        egui::Color32::WHITE,
    );
}

struct GifExportOptions {
    width: u32,
    fps: u32,
}

struct Loading {
    started: std::time::Instant,
    player: Player,
    source: PathBuf,
    project_path: Option<PathBuf>,
    stored: Option<Project>,
    saved: Option<Project>,
}

fn paint_preview_annotations(
    painter: &egui::Painter,
    project: &Project,
    rect: egui::Rect,
    time: f64,
) {
    let painter = painter.with_clip_rect(rect);
    for a in project
        .annotations
        .iter()
        .filter(|a| a.visible_at(time, project.video.duration))
    {
        video_annotations::render::paint_at(
            &painter,
            a,
            rect,
            [project.video.width as f32, project.video.height as f32],
            time,
        );
    }
}

fn seek_slider(
    ui: &mut egui::Ui,
    scrub: &mut Option<f64>,
    playback: f64,
    duration: f64,
) -> Option<f64> {
    let mut position = scrub.unwrap_or(playback);
    ui.spacing_mut().slider_width = (ui.available_width() - 150.0).max(100.0);
    let slider = ui.add(
        egui::Slider::new(&mut position, 0.0..=duration.max(0.0))
            // Always clamping also rounds the externally updated clock and marks it
            // changed on idle frames. Only user edits may produce seek commands.
            .clamping(egui::SliderClamping::Edits)
            .text("seconds")
            .fixed_decimals(3),
    );
    if slider.changed() {
        *scrub = Some(position);
        if !slider.dragged() {
            *scrub = None;
        }
        // User edits seek immediately so the video follows the pointer. The
        // untouched playback clock never enters this branch.
        return Some(position);
    }
    if slider.drag_stopped() {
        *scrub = None;
        return Some(position);
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use video_annotations::annotations::{Annotation, Kind};

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        }
    }

    #[test]
    fn playback_clock_does_not_seek_when_slider_is_untouched() {
        let ctx = egui::Context::default();
        let mut scrub = None;
        // 24/30 fps clocks are not exact multiples of one millisecond.
        for frame in 0..120 {
            let mut output = ctx.run_ui(input(vec![]), |ui| {
                assert_eq!(seek_slider(ui, &mut scrub, frame as f64 / 24.0, 5.0), None);
                assert!(scrub.is_none());
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    #[ignore = "requires local libmpv and FFmpeg"]
    fn live_playback_with_seek_slider_keeps_realtime_speed() {
        use std::time::Instant;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clock.mp4");
        let ffmpeg = std::env::var_os("VIDEO_ANNOTATIONS_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("tools/ffmpeg/bin/ffmpeg.exe"));
        let status = std::process::Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=24:duration=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=4",
                "-c:v",
                "libx264",
                "-c:a",
                "aac",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        let mut player = Player::with_audio_output(Arc::new(|| {}), "null").unwrap();
        player.load(&source).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            player.poll();
            player.render([320, 180]).unwrap();
            if player.state.loaded && player.state.duration > 0.0 && !player.state.seeking {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let ctx = egui::Context::default();
        let mut scrub = None;
        let start = Instant::now();
        player.pause(false).unwrap();
        while start.elapsed() < Duration::from_secs(2) {
            player.poll();
            player.render([320, 180]).unwrap();
            let mut output = ctx.run_ui(input(vec![]), |ui| {
                assert_eq!(
                    seek_slider(ui, &mut scrub, player.state.position, player.state.duration),
                    None,
                    "UI must never seek in response to a playback clock update"
                );
            });
            output.textures_delta.clear();
            assert!(player.state.error.is_none());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!player.state.seeking);
        let offset = player.state.position - start.elapsed().as_secs_f64();
        assert!(offset.abs() < 0.25, "Playback clock offset: {offset}");
        assert!(player.state.av_sync.is_some_and(|v| v.abs() < 0.15));

        // The same slider must seek the real decoder before mouse-up.
        let pressed = egui::pos2(250.0, 10.0);
        let moved = egui::pos2(460.0, 10.0);
        for (frame, events) in [
            vec![
                egui::Event::PointerMoved(pressed),
                egui::Event::PointerButton {
                    pos: pressed,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![egui::Event::PointerMoved(moved)],
        ]
        .into_iter()
        .enumerate()
        {
            let mut target = None;
            let mut output = ctx.run_ui(input(events), |ui| {
                target = seek_slider(ui, &mut scrub, player.state.position, player.state.duration);
            });
            output.textures_delta.clear();
            let target = target.expect("User drag should seek on each change");
            player.pause(true).unwrap();
            player.seek(target).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                player.poll();
                player.render([320, 180]).unwrap();
                if !player.state.seeking && (player.state.position - target).abs() < 0.06 {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Frame {frame}: {:?}",
                    player.state
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            if frame == 1 {
                assert!(scrub.is_some(), "The pointer is still down");
                assert!(target > 2.0, "Expected a later frame during drag: {target}");
            }
        }
    }

    #[test]
    fn seek_slider_updates_while_dragging_and_commits_on_release() {
        let ctx = egui::Context::default();
        let mut scrub = None;
        let mut seeks = Vec::new();
        let pointer = egui::pos2(250.0, 10.0);
        let moved = egui::pos2(460.0, 10.0);
        for (frame, events) in [
            vec![],
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![egui::Event::PointerMoved(moved)],
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ]
        .into_iter()
        .enumerate()
        {
            let mut output = ctx.run_ui(input(events), |ui| {
                if let Some(time) = seek_slider(ui, &mut scrub, 0.0, 5.0) {
                    seeks.push(time);
                }
            });
            output.textures_delta.clear();
            if frame == 2 {
                assert!(
                    seeks.len() >= 2,
                    "Video must seek at press and during movement, before release: {seeks:?}"
                );
            }
        }
        assert!(
            seeks.len() >= 2,
            "Expected seeks before mouse release: {seeks:?}"
        );
        assert!(seeks[0] > 1.0 && seeks[0] < 3.0);
        assert!(
            seeks.iter().any(|time| *time > 3.0),
            "Drag did not seek while moving: {seeks:?}"
        );
        assert!(scrub.is_none());
    }

    #[test]
    fn fullscreen_video_fills_available_area_without_editor_handles() {
        let mut app = app();
        let ctx = app.context.clone();
        app.editor.selected = Some(0);
        app.texture = Some(ctx.load_texture(
            "test",
            egui::ColorImage::filled([16, 9], egui::Color32::BLACK),
            Default::default(),
        ));
        let before = app.project.clone();
        let mut output = ctx.run_ui(input(vec![]), |ui| {
            let size = ui.available_size();
            app.video_view(ui, size, true);
            assert!(ui.min_rect().height() >= size.y);
        });
        // No light-blue editor rectangle or handles in viewing mode.
        assert!(!output.shapes.iter().any(|s| match &s.shape {
            egui::Shape::Rect(r) =>
                r.stroke.color == egui::Color32::LIGHT_BLUE || r.fill == egui::Color32::LIGHT_BLUE,
            _ => false,
        }));
        assert_eq!(app.project, before);
        assert_eq!(app.editor.selected, Some(0));
        output.textures_delta.clear();
    }

    fn app() -> VideoApp {
        let mut project = Project::new(VideoInfo {
            path: "fixture.mp4".into(),
            width: 640,
            height: 360,
            duration: 5.0,
            fps: Some(30.0),
        });
        project.annotations.push(Annotation::new(
            Kind::Text,
            [10.0, 10.0],
            [100.0, 100.0],
            5.0,
        ));
        VideoApp {
            gif_width: 640,
            gif_fps: 15,
            gif_dialog: None,
            show_shortcuts_dialog: false,
            export: None,
            export_progress: 0.0,
            export_message: None,
            project_path: None,
            saved: Some(project.clone()),
            history: History::new(&project.annotations),
            loading: None,
            pending: None,
            allow_close: false,
            editor: Default::default(),
            player: None,
            project: Some(project),
            source: None,
            texture: None,
            error: None,
            fullscreen: false,
            show_video_info: true,
            volume: 70.0,
            muted: false,
            scrub: None,
            context: egui::Context::default(),
        }
    }

    #[test]
    fn header_shows_project_video_and_unsaved_state() {
        let mut app = app();
        assert_eq!(
            app.header_title(),
            "VideoAnnotations ( Untitled.vannot - fixture.mp4 )"
        );
        app.project_path = Some("Blowing.vannot".into());
        app.source = Some("Blowing.mp4".into());
        assert_eq!(
            app.header_title(),
            "VideoAnnotations ( Blowing.vannot - Blowing.mp4 )"
        );
        app.project.as_mut().unwrap().annotations.clear();
        assert_eq!(
            app.header_title(),
            "VideoAnnotations ( Blowing.vannot * - Blowing.mp4 )"
        );
        app.project = None;
        assert_eq!(app.header_title(), "VideoAnnotations");
    }

    #[test]
    fn video_information_shows_resolution_fps_and_audio_status() {
        let state = PlaybackState {
            width: 1920,
            height: 1080,
            fps: Some(29.97),
            audio_output: Some("default".into()),
            ..Default::default()
        };
        assert_eq!(
            video_information_text(&state),
            "1920 × 1080\n29.970 fps\nAudio enabled"
        );
        let state = PlaybackState {
            fps: None,
            audio_output: None,
            ..state
        };
        assert_eq!(
            video_information_text(&state),
            "1920 × 1080\nVariable / unknown fps\nNo active audio output"
        );
    }

    #[test]
    fn seeking_overlay_does_not_change_player_layout() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(input(vec![]), |ui| {
            let (video_rect, _) =
                ui.allocate_exact_size(egui::vec2(400.0, 220.0), egui::Sense::hover());
            let remaining = ui.available_rect_before_wrap();
            paint_seeking_indicator(ui, video_rect);
            assert_eq!(ui.available_rect_before_wrap(), remaining);
        });
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
    }

    #[test]
    fn ctrl_i_toggles_video_information() {
        let mut app = app();
        let ctx = app.context.clone();
        assert!(app.show_video_info);
        let mut output = ctx.run_ui(
            input(vec![egui::Event::Key {
                key: egui::Key::I,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }]),
            |ui| app.shortcuts(ui.ctx()),
        );
        output.textures_delta.clear();
        assert!(!app.show_video_info);
    }

    #[test]
    fn bottom_panel_tracks_content_height_without_gap() {
        let ctx = egui::Context::default();
        let mut heights = Vec::new();
        for (frame, content_height) in [90.0, 90.0, 20.0, 20.0].into_iter().enumerate() {
            let mut output = ctx.run_ui(input(vec![]), |ui| {
                let bottom = ui.available_rect_before_wrap().bottom();
                let panel = egui::Panel::bottom("layout-test")
                    .default_size(90.0)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), content_height),
                            egui::Sense::hover(),
                        );
                    });
                if frame == 1 || frame == 3 {
                    assert!(
                        (panel.response.rect.bottom() - bottom).abs() <= 1.0,
                        "panel {:?}, available bottom {bottom}",
                        panel.response.rect
                    );
                }
                heights.push(panel.response.rect.height());
            });
            output.textures_delta.clear();
        }
        assert!(heights[3] < heights[1], "panel did not shrink: {heights:?}");
    }

    #[test]
    fn bottom_panel_splitter_changes_timeline_height() {
        let ctx = egui::Context::default();
        let render = |events| {
            let mut rect = egui::Rect::NOTHING;
            let mut output = ctx.run_ui(input(events), |ui| {
                rect = egui::Panel::bottom("resizable-timeline-test")
                    .default_size(200.0)
                    .min_size(180.0)
                    .max_size(400.0)
                    .resizable(true)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.take_available_space();
                    })
                    .response
                    .rect;
            });
            output.textures_delta.clear();
            rect
        };
        render(vec![]);
        let initial = render(vec![]);
        let handle = egui::pos2(400.0, initial.top());
        render(vec![egui::Event::PointerMoved(handle)]);
        render(vec![egui::Event::PointerButton {
            pos: handle,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let moved = handle - egui::vec2(0.0, 80.0);
        render(vec![egui::Event::PointerMoved(moved)]);
        render(vec![egui::Event::PointerMoved(moved)]);
        render(vec![egui::Event::PointerButton {
            pos: moved,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let resized = render(vec![]);
        assert!(
            resized.height() > initial.height() + 40.0,
            "splitter did not grow timeline: {initial:?} -> {resized:?}"
        );
    }

    #[test]
    fn properties_panel_stays_right_of_video_and_timeline() {
        let mut app = app();
        let ctx = app.context.clone();
        for frame in 0..2 {
            let mut output = ctx.run_ui(input(vec![]), |ui| {
                let outer = ui.available_rect_before_wrap();
                let properties = egui::Panel::right("test-properties")
                    .default_size(280.0)
                    .min_size(220.0)
                    .max_size(360.0)
                    .resizable(true)
                    .show(ui, |ui| {
                        app.editor.properties(ui, app.project.as_mut().unwrap())
                    });
                let timeline = egui::Panel::bottom("test-timeline")
                    .default_size(180.0)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        app.editor.timeline(ui, app.project.as_mut().unwrap(), 0.0)
                    });
                if frame == 1 {
                    assert!((properties.response.rect.right() - outer.right()).abs() <= 1.0);
                    assert!(
                        timeline.response.rect.right() <= properties.response.rect.left() + 1.0
                    );
                    assert!(ui.available_width() < outer.width() - 220.0);
                }
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn undo_back_to_saved_state_clears_dirty_and_redo_restores_it() {
        let mut app = app();
        assert!(!app.dirty());
        app.project.as_mut().unwrap().annotations[0].font_size = 72.0;
        assert!(app.dirty());
        app.restore_history(false);
        assert!(!app.dirty());
        app.restore_history(true);
        assert!(app.dirty());
    }

    #[test]
    fn invalid_project_open_keeps_current_document_and_history() {
        let mut app = app();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.vannot");
        std::fs::write(&path, b"not json").unwrap();
        let original = app.project.clone();
        app.load_path(path);
        assert_eq!(app.project, original);
        assert!(app.error.is_some());
        assert!(app.loading.is_none());
        assert!(!app.dirty());
    }

    #[test]
    fn failed_save_keeps_dirty_document_and_pending_close() {
        let mut app = app();
        let dir = tempfile::tempdir().unwrap();
        app.project_path = Some(dir.path().join("missing").join("test.vannot"));
        app.project.as_mut().unwrap().annotations.clear();
        app.pending = Some(Pending::Close);
        assert!(!app.save(false));
        assert!(app.dirty());
        assert!(app.pending.is_some());
        assert!(!app.allow_close);
        assert_eq!(app.saved.unwrap().annotations.len(), 1);
    }
}
