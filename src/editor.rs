use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use video_annotations::{
    annotations::{Annotation, Effect, Kind},
    project::Project,
};

#[derive(Default)]
pub struct Editor {
    tool: Option<Kind>,
    pub(crate) selected: Option<usize>,
    drag: Option<Drag>,
    rename: Option<RenameDraft>,
    inline_text: Option<InlineText>,
    pub(crate) timeline_drag: Option<crate::timeline::TimelineDrag>,
}
struct InlineText {
    index: usize,
    original: String,
    focus: bool,
}
struct RenameDraft {
    index: usize,
    name: String,
    error: Option<String>,
}
enum Drag {
    Create {
        index: usize,
        start: [f32; 2],
    },
    Move {
        index: usize,
        original: Annotation,
        start: [f32; 2],
    },
    Resize {
        index: usize,
        original: Annotation,
        handle: usize,
    },
}

#[derive(Clone, Copy)]
struct Mapping {
    rect: Rect,
    extent: [f32; 2],
}
impl Mapping {
    fn screen(self, p: [f32; 2]) -> Pos2 {
        self.rect.min
            + Vec2::new(
                p[0] / self.extent[0] * self.rect.width(),
                p[1] / self.extent[1] * self.rect.height(),
            )
    }
    fn video(self, p: Pos2) -> [f32; 2] {
        [
            ((p.x - self.rect.min.x) / self.rect.width() * self.extent[0])
                .clamp(0.0, self.extent[0]),
            ((p.y - self.rect.min.y) / self.rect.height() * self.extent[1])
                .clamp(0.0, self.extent[1]),
        ]
    }
    fn scale(self) -> f32 {
        self.rect.width() / self.extent[0]
    }
}

impl Editor {
    pub(crate) fn select(&mut self, project: &mut Project, index: usize) {
        self.inline_text = None; // Changing selection accepts the text edit.
        self.cancel(project);
        self.selected = Some(index);
    }

    pub(crate) fn reorder(&mut self, project: &mut Project, forward: bool) {
        self.inline_text = None;
        self.cancel(project);
        if let Some(index) = self.selected {
            let target = if forward {
                index + 1
            } else {
                index.saturating_sub(1)
            };
            if target < project.annotations.len() {
                project.annotations.swap(index, target);
                self.selected = Some(target);
            }
        }
    }

    pub fn can_toggle_fullscreen(&self) -> bool {
        self.tool.is_none() && self.selected.is_none() && self.drag.is_none()
    }

    pub fn renaming(&self) -> bool {
        self.rename.is_some()
    }

    pub fn dismiss_rename(&mut self) {
        self.rename = None;
    }
    pub fn tools(&mut self, ui: &mut egui::Ui) -> bool {
        let mut pause = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong("Annotation timeline");
            if ui.selectable_label(self.tool.is_none(), "Select").clicked() {
                self.tool = None;
            }
            for kind in [
                Kind::Text,
                Kind::Rectangle,
                Kind::Ellipse,
                Kind::Arrow,
                Kind::Line,
                Kind::Spotlight,
            ] {
                if ui
                    .selectable_label(self.tool == Some(kind), kind.label())
                    .clicked()
                {
                    self.tool = Some(kind);
                    pause = true;
                }
            }
            ui.small(match self.tool {
                Some(Kind::Text) => "Click the video to add text. Edit its content in Properties.",
                Some(_) => "Drag on the video to draw. Esc cancels.",
                None => "Click to select; double-click text to edit; drag to move or resize.",
            });
        });
        pause
    }

    pub fn properties(&mut self, ui: &mut egui::Ui, project: &mut Project) -> bool {
        let mut pause = false;
        ui.heading("Properties");
        egui::ScrollArea::vertical()
            .id_salt("annotation-properties-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name = self
                        .selected
                        .and_then(|i| project.annotations.get(i).map(|a| a.display_name(i)))
                        .unwrap_or_else(|| "No annotation selected".into());
                    let button_width = 30.0;
                    let gap = ui.spacing().item_spacing.x;
                    let name_width = (ui.available_width() - 2.0 * (button_width + gap)).max(1.0);
                    ui.allocate_space(egui::vec2(button_width, 24.0));
                    ui.add_sized(
                        [name_width, 24.0],
                        egui::Label::new(egui::RichText::new(name).strong())
                            .truncate()
                            .halign(egui::Align::Center),
                    );
                    let rename_button = ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("").min_size(egui::vec2(button_width, 24.0)),
                        )
                        .on_hover_text("Rename annotation");
                    paint_pencil_icon(ui, &rename_button);
                    if rename_button.clicked()
                        && let Some(index) = self.selected
                    {
                        self.rename = Some(RenameDraft {
                            index,
                            name: project.annotations[index].display_name(index),
                            error: None,
                        });
                        ui.ctx().memory_mut(|m| {
                            m.request_focus(egui::Id::new("rename-annotation-input"))
                        });
                        pause = true;
                    }
                });
                ui.separator();
                if let Some(a) = self.selected.and_then(|i| project.annotations.get_mut(i)) {
                    ui.strong("Appearance");
                    if a.kind == Kind::Spotlight {
                        pause |= ui
                            .add(
                                egui::Slider::new(&mut a.dimming, 0.0..=1.0)
                                    .text("Dimming")
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            )
                            .changed();
                        ui.small("The rectangle stays clear.");
                        ui.small("Timeline fades gradually dim and restore the surrounding video.");
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Color");
                            pause |= ui
                                .color_edit_button_srgba_unmultiplied(&mut a.color)
                                .changed();
                        });
                        if a.kind == Kind::Text {
                            ui.horizontal(|ui| {
                                ui.label("Font size");
                                pause |= ui
                                    .add(
                                        egui::DragValue::new(&mut a.font_size)
                                            .range(6.0..=300.0)
                                            .speed(1.0)
                                            .suffix(" px"),
                                    )
                                    .changed();
                            });
                            ui.label("Text content (also editable by double-clicking the video)");
                            if self.inline_text.is_none() {
                                let response = ui.add(
                                    egui::TextEdit::multiline(&mut a.text)
                                        .id(egui::Id::new(("annotation-text", self.selected)))
                                        .desired_rows(3)
                                        .desired_width(ui.available_width()),
                                );
                                pause |= response.has_focus();
                            } else {
                                ui.small("Editing directly on the video.");
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Thickness");
                                pause |= ui
                                    .add(
                                        egui::DragValue::new(&mut a.thickness)
                                            .range(1.0..=60.0)
                                            .speed(0.5)
                                            .suffix(" px"),
                                    )
                                    .changed();
                            });
                        }
                    }
                    if a.kind != Kind::Spotlight {
                        ui.separator();
                        egui::CollapsingHeader::new("Effects")
                            .id_salt(("effects", self.selected))
                            .default_open(true)
                            .show(ui, |ui| {
                                let mut selected = match a.effects.effect {
                                    Effect::None => 0,
                                    Effect::Glow { .. } => 1,
                                    Effect::Orbit { .. } => 2,
                                };
                                let previous = selected;
                                egui::ComboBox::from_id_salt(("annotation-effect", self.selected))
                                    .selected_text(["None", "Glow", "Orbiting ball"][selected])
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut selected, 0, "None");
                                        ui.selectable_value(&mut selected, 1, "Glow");
                                        if a.kind.supports_orbit() {
                                            ui.selectable_value(&mut selected, 2, "Orbiting ball");
                                        }
                                    });
                                if selected != previous {
                                    a.effects.effect = match selected {
                                        1 => Effect::Glow {
                                            color: [80, 220, 255],
                                            pulse_hz: 0.5,
                                        },
                                        2 => Effect::Orbit {
                                            color: [80, 220, 255],
                                            period: 2.0,
                                        },
                                        _ => Effect::None,
                                    };
                                    pause = true;
                                }
                                match &mut a.effects.effect {
                                    Effect::Glow { color, pulse_hz } => {
                                        ui.horizontal(|ui| {
                                            ui.label("Pulse color");
                                            pause |= ui.color_edit_button_srgb(color).changed();
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Pulse speed");
                                            pause |= ui
                                                .add(
                                                    egui::DragValue::new(pulse_hz)
                                                        .range(0.05..=10.0)
                                                        .speed(0.05)
                                                        .suffix(" Hz"),
                                                )
                                                .changed();
                                        });
                                        ui.small("One pulse is a full color cycle and return.");
                                    }
                                    Effect::Orbit { color, period } => {
                                        ui.horizontal(|ui| {
                                            ui.label("Ball color");
                                            pause |= ui.color_edit_button_srgb(color).changed();
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Turn duration");
                                            pause |= ui
                                                .add(
                                                    egui::DragValue::new(period)
                                                        .range(0.1..=60.0)
                                                        .speed(0.1)
                                                        .suffix(" s"),
                                                )
                                                .changed();
                                        });
                                    }
                                    Effect::None => {}
                                }
                            });
                    }
                    ui.separator();
                    if ui.button("Delete annotation").clicked() {
                        self.delete(project);
                        pause = true;
                    }
                } else {
                    ui.label(
                        "Select an annotation on the video or timeline to edit its properties.",
                    );
                }
            });
        pause
    }

    pub fn rename_dialog(&mut self, ctx: &egui::Context, project: &mut Project) -> bool {
        let Some(draft) = self.rename.as_mut() else {
            return false;
        };
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("rename-annotation")).show(ctx, |ui| {
            ui.heading("Rename annotation");
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .id(egui::Id::new("rename-annotation-input"))
                    .desired_width(320.0),
            );
            if let Some(error) = &draft.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            ui.horizontal(|ui| {
                confirm =
                    ui.button("Rename").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel {
            self.rename = None;
            return false;
        }
        if !confirm {
            return false;
        }
        let draft = self.rename.as_mut().unwrap();
        match normalize_annotation_name(&draft.name) {
            Ok(name) => {
                let index = draft.index;
                let name = name.to_owned();
                self.rename = None;
                if let Some(annotation) = project.annotations.get_mut(index) {
                    let changed = annotation.name != name;
                    annotation.name = name;
                    return changed;
                }
            }
            Err(error) => draft.error = Some(error),
        }
        false
    }

    fn delete(&mut self, project: &mut Project) {
        self.cancel(project);
        if let Some(index) = self.selected.take()
            && index < project.annotations.len()
        {
            project.annotations.remove(index);
        }
    }
    pub fn cancel(&mut self, project: &mut Project) {
        if let Some(edit) = self.inline_text.take()
            && let Some(a) = project.annotations.get_mut(edit.index)
        {
            a.text = edit.original;
        }
        match self.drag.take() {
            Some(Drag::Create { index, .. }) => {
                project.annotations.remove(index);
                self.selected = None;
            }
            Some(
                Drag::Move {
                    index, original, ..
                }
                | Drag::Resize {
                    index, original, ..
                },
            ) => project.annotations[index] = original,
            None => {}
        }
        self.tool = None;
    }

    /// Returns whether an editing gesture should pause playback.
    pub fn canvas(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: Rect,
        project: &mut Project,
        time: f64,
    ) -> bool {
        let map = Mapping {
            rect,
            extent: [project.video.width as f32, project.video.height as f32],
        };
        let mut pause = false;
        if self.inline_text.is_none() && ui.is_enabled() && !ui.ctx().text_edit_focused() {
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.delete(project);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cancel(project);
            }
        }
        if self.inline_text.is_none()
            && response.drag_started()
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            && rect.contains(origin)
        {
            pause = true;
            let point = map.video(origin);
            if let Some(kind) = self.tool {
                if kind != Kind::Text {
                    let index = project.annotations.len();
                    project.annotations.push(
                        Annotation::new(kind, point, point, project.video.duration)
                            .at_playhead(time, project.video.duration),
                    );
                    self.selected = Some(index);
                    self.drag = Some(Drag::Create {
                        index,
                        start: point,
                    });
                }
            } else {
                let handle = self.selected.and_then(|index| {
                    project
                        .annotations
                        .get(index)
                        .filter(|a| a.visible_at(time, project.video.duration))
                        .and_then(|a| {
                            handles(&a.evaluated(time))
                                .iter()
                                .position(|p| map.screen(*p).distance(origin) <= 9.0)
                                .map(|h| (index, h))
                        })
                });
                if let Some((index, handle)) = handle {
                    self.drag = Some(Drag::Resize {
                        index,
                        original: project.annotations[index].clone(),
                        handle,
                    });
                } else {
                    self.selected = pick(project, point, 6.0 / map.scale(), time);
                    if let Some(index) = self.selected {
                        self.drag = Some(Drag::Move {
                            index,
                            original: project.annotations[index].clone(),
                            start: point,
                        });
                    }
                }
            }
        }
        if self.inline_text.is_none()
            && response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && rect.contains(pos)
        {
            pause = true;
            let point = map.video(pos);
            if self.tool == Some(Kind::Text) {
                let width = (map.extent[0] * 0.35).min(420.0);
                let height = (map.extent[1] * 0.2).min(120.0);
                let a = [
                    point[0].min(map.extent[0] - width),
                    point[1].min(map.extent[1] - height),
                ];
                let index = project.annotations.len();
                project.annotations.push(
                    Annotation::new(
                        Kind::Text,
                        a,
                        [a[0] + width, a[1] + height],
                        project.video.duration,
                    )
                    .at_playhead(time, project.video.duration),
                );
                self.selected = Some(index);
                self.tool = None;
            } else if self.tool.is_none() {
                self.selected = pick(project, point, 6.0 / map.scale(), time);
            }
        }
        if self.inline_text.is_none()
            && self.tool.is_none()
            && response.double_clicked()
            && let Some(pos) = response
                .interact_pointer_pos()
                .filter(|p| rect.contains(*p))
            && let Some(index) = pick(project, map.video(pos), 6.0 / map.scale(), time)
            && project.annotations[index].kind == Kind::Text
        {
            self.selected = Some(index);
            self.drag = None;
            self.inline_text = Some(InlineText {
                index,
                original: project.annotations[index].text.clone(),
                focus: true,
            });
        }
        if let Some(drag) = &self.drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let p = map.video(pos);
            match drag {
                Drag::Create { index, start } => {
                    project.annotations[*index].a = *start;
                    project.annotations[*index].b = p;
                }
                Drag::Move {
                    index,
                    original,
                    start,
                } => {
                    let mut a = original.clone();
                    a.translate([p[0] - start[0], p[1] - start[1]], map.extent);
                    project.annotations[*index] = a;
                }
                Drag::Resize {
                    index,
                    original,
                    handle,
                } => {
                    let mut a = original.clone();
                    let evaluated = original.evaluated(time);
                    let p = [
                        p[0] - (evaluated.a[0] - original.a[0]),
                        p[1] - (evaluated.a[1] - original.a[1]),
                    ];
                    let p = [
                        p[0].clamp(0.0, map.extent[0]),
                        p[1].clamp(0.0, map.extent[1]),
                    ];
                    if matches!(a.kind, Kind::Arrow | Kind::Line) {
                        if *handle == 0 {
                            a.a = p;
                        } else {
                            a.b = p;
                        }
                    } else {
                        let opposite = handles(original)[(handle + 2) % 4];
                        a.a = opposite;
                        a.b = p;
                    }
                    project.annotations[*index] = a;
                }
            }
        }
        if response.drag_stopped()
            && let Some(drag) = self.drag.take()
        {
            match drag {
                Drag::Create { index, .. } => {
                    if !valid(&project.annotations[index], map.scale()) {
                        project.annotations.remove(index);
                        self.selected = None;
                    } else {
                        self.tool = None;
                    }
                }
                Drag::Resize {
                    index, original, ..
                } if !valid(&project.annotations[index], map.scale()) => {
                    project.annotations[index] = original;
                }
                _ => {}
            }
        }
        let painter = ui.painter().with_clip_rect(rect);
        for (index, a) in project
            .annotations
            .iter()
            .enumerate()
            .filter(|(_, a)| a.visible_at(time, project.video.duration))
        {
            if self
                .inline_text
                .as_ref()
                .is_some_and(|edit| edit.index == index)
            {
                continue;
            }
            video_annotations::render::paint_at(&painter, a, map.rect, map.extent, time);
        }
        if let Some(a) = self
            .selected
            .and_then(|i| project.annotations.get(i))
            .filter(|a| a.visible_at(time, project.video.duration))
        {
            let a = &a.evaluated(time);
            if !matches!(a.kind, Kind::Rectangle | Kind::Line) {
                let (min, max) = a.bounds();
                painter.rect_stroke(
                    Rect::from_two_pos(map.screen(min), map.screen(max)),
                    0.0,
                    Stroke::new(1.0, Color32::LIGHT_BLUE),
                    egui::StrokeKind::Outside,
                );
            }
            for p in handles(a) {
                if self.tool.is_none()
                    && ui.is_enabled()
                    && response.hovered()
                    && response
                        .hover_pos()
                        .is_some_and(|pos| map.screen(p).distance(pos) <= 9.0)
                {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                painter.rect_filled(
                    Rect::from_center_size(map.screen(p), Vec2::splat(8.0)),
                    1.0,
                    Color32::LIGHT_BLUE,
                );
            }
        }
        if matches!(self.drag, Some(Drag::Resize { .. })) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if self.tool.is_some() && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if let Some(mut edit) = self.inline_text.take() {
            pause = true;
            if let Some(a) = project.annotations.get_mut(edit.index) {
                let (min, max) = a.bounds();
                let edit_rect = Rect::from_two_pos(map.screen(min), map.screen(max));
                let id = egui::Id::new(("inline-annotation-text", edit.index));
                let first = edit.focus;
                if first {
                    ui.ctx().memory_mut(|m| m.request_focus(id));
                    edit.focus = false;
                }
                let mut child =
                    ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(edit_rect));
                child.set_clip_rect(rect.intersect(edit_rect));
                let response = child.add_sized(
                    edit_rect.size(),
                    egui::TextEdit::multiline(&mut a.text)
                        .id(id)
                        .font(egui::FontId::proportional(a.font_size * map.scale()))
                        .text_color(Color32::from_rgb(a.color[0], a.color[1], a.color[2]))
                        .desired_width(edit_rect.width())
                        .margin(egui::Margin::ZERO)
                        .hint_text("Type your text"),
                );
                if first {
                    response.request_focus();
                }
                let cancel =
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                let accept = ui
                    .input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter))
                    || self.tool.is_some()
                    || (!first && response.lost_focus())
                    || (!first
                        && ui.input(|i| {
                            i.pointer.any_click()
                                && i.pointer
                                    .interact_pos()
                                    .is_some_and(|p| !edit_rect.contains(p))
                        }));
                if cancel {
                    a.text = edit.original;
                } else if !accept {
                    self.inline_text = Some(edit);
                }
                if cancel || accept {
                    ui.ctx().memory_mut(|m| m.surrender_focus(id));
                }
            }
        }
        pause
    }
}

fn normalize_annotation_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        Err("Enter a name for the annotation.".into())
    } else if name.len() > 128 {
        Err("Annotation names must be at most 128 bytes long.".into())
    } else if name.chars().any(char::is_control) {
        Err("Annotation names cannot contain control characters.".into())
    } else {
        Ok(name)
    }
}

fn paint_pencil_icon(ui: &egui::Ui, button: &egui::Response) {
    if !ui.is_rect_visible(button.rect) {
        return;
    }
    let center = button.rect.center();
    let point = |x: f32, y: f32| center + egui::vec2(x, y);
    let color = ui.style().interact(button).fg_stroke.color;
    let stroke = Stroke::new(1.5, color);
    ui.painter().add(egui::Shape::closed_line(
        vec![
            point(-6.0, 6.0),
            point(-4.0, 1.0),
            point(3.0, -6.0),
            point(6.0, -3.0),
            point(-1.0, 4.0),
        ],
        stroke,
    ));
    ui.painter()
        .line_segment([point(-4.0, 1.0), point(-1.0, 4.0)], stroke);
}

fn pick(project: &Project, p: [f32; 2], tolerance: f32, time: f64) -> Option<usize> {
    project.annotations.iter().rposition(|a| {
        a.visible_at(time, project.video.duration) && a.evaluated(time).hit(p, tolerance)
    })
}
fn handles(a: &Annotation) -> Vec<[f32; 2]> {
    if matches!(a.kind, Kind::Arrow | Kind::Line) {
        return vec![a.a, a.b];
    }
    let (min, max) = a.bounds();
    vec![min, [max[0], min[1]], max, [min[0], max[1]]]
}
fn valid(a: &Annotation, scale: f32) -> bool {
    let (min, max) = a.bounds();
    if matches!(a.kind, Kind::Arrow | Kind::Line) {
        ((max[0] - min[0]).hypot(max[1] - min[1])) * scale >= 3.0
    } else {
        (max[0] - min[0]) * scale >= 3.0 && (max[1] - min[1]) * scale >= 3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_dialog_commits_trimmed_name_and_rejects_blank_name() {
        assert!(normalize_annotation_name("  ").is_err());
        assert!(normalize_annotation_name(&"x".repeat(129)).is_err());
        let ctx = egui::Context::default();
        let mut project = project();
        let mut editor = Editor {
            selected: Some(0),
            rename: Some(RenameDraft {
                index: 0,
                name: "  Opening rectangle  ".into(),
                error: None,
            }),
            ..Default::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 600.0))),
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut renamed = false;
        let mut output = ctx.run_ui(input, |ui| {
            renamed = editor.rename_dialog(ui.ctx(), &mut project);
        });
        output.textures_delta.clear();
        assert!(renamed);
        assert_eq!(project.annotations[0].name, "Opening rectangle");
        assert!(!editor.renaming());
    }

    #[test]
    fn double_click_edits_text_on_video_and_accepts_or_cancels() {
        for mode in 0..3 {
            let cancel = mode == 1;
            let ctx = egui::Context::default();
            let mut project = project();
            let a = &mut project.annotations[0];
            a.kind = Kind::Text;
            a.a = [200.0, 100.0];
            a.b = [900.0, 300.0];
            a.text.clear();
            let mut editor = Editor::default();
            let pos = egui::pos2(150.0, 75.0);
            for frame in 0..6 {
                let modifiers = if frame == 5 && mode == 0 {
                    egui::Modifiers::CTRL
                } else {
                    egui::Modifiers::NONE
                };
                let events = match frame {
                    0 | 2 => vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers,
                        },
                    ],
                    1 | 3 => vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    }],
                    4 => vec![egui::Event::Text("Edited on video".into())],
                    _ if mode == 2 => vec![
                        egui::Event::PointerMoved(egui::pos2(800.0, 400.0)),
                        egui::Event::PointerButton {
                            pos: egui::pos2(800.0, 400.0),
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers,
                        },
                        egui::Event::PointerButton {
                            pos: egui::pos2(800.0, 400.0),
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers,
                        },
                    ],
                    _ => vec![egui::Event::Key {
                        key: if cancel {
                            egui::Key::Escape
                        } else {
                            egui::Key::Enter
                        },
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                };
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1000.0, 600.0),
                        )),
                        time: Some(frame as f64 * 0.08),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(960.0, 540.0),
                            egui::Sense::click_and_drag(),
                        );
                        editor.canvas(ui, &response, rect, &mut project, 1.0);
                    },
                );
                output.textures_delta.clear();
                if frame == 3 || frame == 4 {
                    assert!(editor.inline_text.is_some());
                    assert!(ctx.memory(|m| {
                        m.has_focus(egui::Id::new(("inline-annotation-text", 0_usize)))
                    }));
                }
            }
            assert!(editor.inline_text.is_none());
            assert_eq!(
                project.annotations[0].text,
                if cancel { "" } else { "Edited on video" }
            );
        }
    }
    #[test]
    fn text_editor_retains_focus_when_status_widgets_change() {
        let ctx = egui::Context::default();
        let mut project = project();
        project.annotations[0].kind = Kind::Text;
        project.annotations[0].text.clear();
        let mut editor = Editor {
            selected: Some(0),
            ..Default::default()
        };
        let id = egui::Id::new(("annotation-text", Some(0_usize)));
        for frame in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 600.0))),
                events: if frame > 0 {
                    vec![egui::Event::Text("a".into())]
                } else {
                    vec![]
                },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                for _ in 0..frame {
                    ui.label("Variable playback status");
                }
                if frame == 0 {
                    ctx.memory_mut(|m| m.request_focus(id));
                }
                assert!(editor.properties(ui, &mut project));
            });
            output.textures_delta.clear();
            assert!(ctx.memory(|m| m.has_focus(id)));
        }
        assert_eq!(project.annotations[0].text, "aaa");
    }

    fn project() -> Project {
        let mut project = Project::new(video_annotations::media::VideoInfo {
            path: "fixture.mp4".into(),
            width: 1920,
            height: 1080,
            duration: 10.0,
            fps: Some(30.0),
        });
        project.annotations.push(Annotation::new(
            Kind::Rectangle,
            [10.0, 10.0],
            [100.0, 100.0],
            10.0,
        ));
        project
    }

    #[test]
    fn topmost_selection_and_deletion() {
        let mut project = project();
        project.annotations.push(project.annotations[0].clone());
        assert_eq!(pick(&project, [50.0, 50.0], 1.0, 0.0), Some(1));
        let mut editor = Editor {
            selected: Some(1),
            ..Default::default()
        };
        editor.delete(&mut project);
        assert_eq!(project.annotations.len(), 1);
        assert_eq!(editor.selected, None);
        editor.delete(&mut project);
        assert_eq!(project.annotations.len(), 1);
    }

    #[test]
    fn glow_keeps_picking_on_stored_geometry() {
        let mut project = project();
        project.annotations[0].effects.effect = Effect::Glow {
            color: [0, 255, 0],
            pulse_hz: 0.5,
        };
        assert_eq!(pick(&project, [50.0, 50.0], 0.0, 0.0), Some(0));
        assert_eq!(pick(&project, [50.0, 50.0], 0.0, 5.0), Some(0));
        assert_eq!(pick(&project, [150.0, 50.0], 0.0, 5.0), None);
        assert_eq!(project.annotations[0].a, [10.0, 10.0]);
    }

    #[test]
    fn hidden_shapes_are_not_picked_and_reordering_keeps_selection() {
        let mut project = project();
        let mut front = project.annotations[0].clone();
        front.kind = Kind::Ellipse;
        front.start_seconds = 2.0;
        project.annotations.push(front);
        assert_eq!(pick(&project, [50.0, 50.0], 1.0, 1.0), Some(0));
        assert_eq!(pick(&project, [50.0, 50.0], 1.0, 2.0), Some(1));
        let mut editor = Editor {
            selected: Some(1),
            ..Default::default()
        };
        editor.reorder(&mut project, false);
        assert_eq!(editor.selected, Some(0));
        assert_eq!(project.annotations[0].kind, Kind::Ellipse);
        assert_eq!(pick(&project, [50.0, 50.0], 1.0, 2.0), Some(1));
        editor.reorder(&mut project, false);
        assert_eq!(editor.selected, Some(0));
        editor.reorder(&mut project, true);
        assert_eq!(editor.selected, Some(1));
    }

    #[test]
    fn cancelling_restores_geometry_or_removes_draft() {
        let mut project = project();
        let original = project.annotations[0].clone();
        let mut editor = Editor {
            selected: Some(0),
            drag: Some(Drag::Move {
                index: 0,
                original: original.clone(),
                start: original.a,
            }),
            ..Default::default()
        };
        project.annotations[0].a = [20.0, 20.0];
        editor.cancel(&mut project);
        assert_eq!(project.annotations[0].a, original.a);
        editor.drag = Some(Drag::Resize {
            index: 0,
            original: original.clone(),
            handle: 2,
        });
        project.annotations[0].b = [200.0, 200.0];
        editor.cancel(&mut project);
        assert_eq!(project.annotations[0].b, original.b);
        editor.drag = Some(Drag::Create {
            index: 0,
            start: original.a,
        });
        editor.cancel(&mut project);
        assert!(project.annotations.is_empty());
        assert_eq!(editor.selected, None);
    }

    #[test]
    fn handles_and_minimum_size_respect_shape_geometry() {
        let mut a = Annotation::new(Kind::Rectangle, [100.0, 100.0], [10.0, 20.0], 10.0);
        assert_eq!(
            handles(&a),
            vec![[10.0, 20.0], [100.0, 20.0], [100.0, 100.0], [10.0, 100.0]]
        );
        a.kind = Kind::Arrow;
        a.b = [10.0, 100.0];
        assert_eq!(handles(&a), vec![a.a, a.b]);
        assert!(valid(&a, 0.5));
        a.kind = Kind::Rectangle;
        assert!(!valid(&a, 0.5));
    }
    #[test]
    fn line_uses_endpoints_and_supports_horizontal_and_vertical_drawing() {
        for b in [[100.0, 20.0], [20.0, 100.0]] {
            let a = Annotation::new(Kind::Line, [20.0, 20.0], b, 2.0);
            assert_eq!(handles(&a), vec![a.a, a.b]);
            assert!(valid(&a, 1.0));
            assert!(a.hit([(a.a[0] + b[0]) * 0.5, (a.a[1] + b[1]) * 0.5], 1.0));
            assert!(!a.hit([60.0, 60.0], 1.0));
        }
    }
    #[test]
    fn coordinates_survive_letterboxing_and_resizing() {
        let a = Mapping {
            rect: Rect::from_min_size(egui::pos2(100.0, 40.0), egui::vec2(640.0, 360.0)),
            extent: [1920.0, 1080.0],
        };
        let b = Mapping {
            rect: Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(1280.0, 720.0)),
            ..a
        };
        let point = [960.0, 540.0];
        assert_eq!(a.video(a.screen(point)), point);
        assert_eq!(b.video(b.screen(point)), point);
        assert_eq!(a.video(egui::pos2(0.0, 0.0)), [0.0, 0.0]);
    }
}
