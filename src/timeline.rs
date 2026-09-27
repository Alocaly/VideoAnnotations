use crate::editor::Editor;
use eframe::egui::{self, Color32, Rect, Stroke};
use video_annotations::{
    annotations::{Annotation, Kind},
    project::Project,
};

#[derive(Default)]
pub struct TimelineAction {
    pub pause: bool,
    pub seek: Option<f64>,
}

impl Editor {
    pub fn timeline(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        time: f64,
    ) -> TimelineAction {
        let mut action = TimelineAction::default();
        let duration = project.video.duration;
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.strong("Annotation timeline");
            ui.label("Add at playhead:");
            for kind in [Kind::Text, Kind::Rectangle, Kind::Ellipse, Kind::Arrow] {
                if ui
                    .add_enabled(duration > 0.0, egui::Button::new(kind.label()))
                    .clicked()
                {
                    self.cancel(project);
                    let w = project.video.width as f32;
                    let h = project.video.height as f32;
                    let a =
                        Annotation::new(kind, [w * 0.25, h * 0.25], [w * 0.65, h * 0.55], duration)
                            .at_playhead(time, duration);
                    action.seek = Some(a.start_seconds);
                    project.annotations.push(a);
                    self.selected = Some(project.annotations.len() - 1);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if let Some(index) = self.selected.filter(|i| *i < project.annotations.len()) {
                let a = &mut project.annotations[index];
                ui.label(format!("{}: {}", index + 1, a.kind.label()));
                ui.label("Start");
                let mut start = a.start_seconds;
                if ui
                    .add(
                        egui::DragValue::new(&mut start)
                            .speed(0.01)
                            .fixed_decimals(3)
                            .suffix(" s"),
                    )
                    .changed()
                {
                    a.set_start(start, duration);
                    action.pause = true;
                }
                if ui.small_button("Start = playhead").clicked() {
                    a.set_start(time, duration);
                    action.pause = true;
                }
                ui.label("End");
                let mut end = a.end_seconds;
                if ui
                    .add(
                        egui::DragValue::new(&mut end)
                            .speed(0.01)
                            .fixed_decimals(3)
                            .suffix(" s"),
                    )
                    .changed()
                {
                    a.set_end(end, duration);
                    action.pause = true;
                }
                if ui.small_button("End = playhead").clicked() {
                    a.set_end(time, duration);
                    action.pause = true;
                }
                if ui.small_button("Go to start").clicked() {
                    action.seek = Some(a.start_seconds);
                }
                if !a.visible_at(time, duration) {
                    ui.label("Outside active interval");
                }
            } else {
                ui.label("Select an annotation to edit its start/end times.");
            }
        });
        ui.horizontal(|ui| {
            let selected = self.selected;
            if ui
                .add_enabled(
                    selected.is_some_and(|i| i + 1 < project.annotations.len()),
                    egui::Button::new("Bring forward"),
                )
                .clicked()
            {
                self.reorder(project, true);
                action.pause = true;
            }
            if ui
                .add_enabled(
                    selected.is_some_and(|i| i > 0),
                    egui::Button::new("Send backward"),
                )
                .clicked()
            {
                self.reorder(project, false);
                action.pause = true;
            }
            ui.small("Top row = front layer. Click a track to select and seek.");
        });
        ui.horizontal(|ui| {
            ui.add_sized([120.0, 20.0], egui::Label::new("Time (seconds)"));
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 20.0),
                egui::Sense::click_and_drag(),
            );
            for tick in 0..=4 {
                let f = tick as f32 / 4.0;
                let align = if tick == 0 {
                    egui::Align2::LEFT_CENTER
                } else if tick == 4 {
                    egui::Align2::RIGHT_CENTER
                } else {
                    egui::Align2::CENTER_CENTER
                };
                ui.painter().text(
                    egui::pos2(rect.left() + rect.width() * f, rect.center().y),
                    align,
                    format!("{:.2}", duration * f as f64),
                    egui::FontId::proportional(11.0),
                    Color32::GRAY,
                );
            }
            if (response.clicked() || response.dragged())
                && let Some(pos) = response.interact_pointer_pos()
            {
                action.seek = Some(time_at_x(rect, pos.x, duration));
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("annotation-tracks")
            .max_height(90.0)
            .min_scrolled_height(90.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if project.annotations.is_empty() {
                    ui.label("No annotations. Add one above or draw on the video.");
                }
                for index in (0..project.annotations.len()).rev() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            let a = &project.annotations[index];
                            let start = a.start_seconds;
                            let end = a.end_seconds;
                            let color = Color32::from_rgb(a.color[0], a.color[1], a.color[2]);
                            let label = format!("{}: {}", index + 1, a.kind.label());
                            if ui
                                .add_sized(
                                    [120.0, 22.0],
                                    egui::Button::new(label).selected(self.selected == Some(index)),
                                )
                                .clicked()
                            {
                                self.select(project, index);
                                action.seek = Some(start);
                            }
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 22.0),
                                egui::Sense::click_and_drag(),
                            );
                            let p = ui.painter();
                            p.rect_filled(rect, 2.0, Color32::from_gray(35));
                            let x = |t: f64| {
                                rect.left()
                                    + rect.width()
                                        * (t / duration.max(f64::EPSILON)).clamp(0.0, 1.0) as f32
                            };
                            let bar = Rect::from_min_max(
                                egui::pos2(x(start), rect.top() + 2.0),
                                egui::pos2(
                                    x(end).max(x(start) + 1.0).min(rect.right()),
                                    rect.bottom() - 2.0,
                                ),
                            );
                            p.rect_filled(bar, 2.0, color.gamma_multiply(0.65));
                            if self.selected == Some(index) {
                                p.rect_stroke(
                                    bar,
                                    2.0,
                                    Stroke::new(1.0, Color32::WHITE),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            p.line_segment(
                                [
                                    egui::pos2(x(time), rect.top()),
                                    egui::pos2(x(time), rect.bottom()),
                                ],
                                Stroke::new(1.5, Color32::LIGHT_RED),
                            );
                            let response =
                                response.on_hover_text(format!("{start:.3} – {end:.3} s"));
                            if (response.clicked() || response.dragged())
                                && let Some(pos) = response.interact_pointer_pos()
                            {
                                self.select(project, index);
                                action.seek = Some(time_at_x(rect, pos.x, duration));
                            }
                        });
                    });
                }
            });
        action.pause |= action.seek.is_some();
        action
    }
}

fn time_at_x(rect: Rect, x: f32, duration: f64) -> f64 {
    ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0) as f64 * duration
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ruler_maps_to_bounded_video_time() {
        let rect = Rect::from_min_size(egui::pos2(120.0, 0.0), egui::vec2(400.0, 20.0));
        assert_eq!(time_at_x(rect, 320.0, 10.0), 5.0);
        assert_eq!(time_at_x(rect, 0.0, 10.0), 0.0);
        assert_eq!(time_at_x(rect, 900.0, 10.0), 10.0);
    }
}
