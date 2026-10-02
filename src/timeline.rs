use crate::editor::Editor;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use video_annotations::{annotations::Annotation, project::Project};

#[derive(Default)]
pub struct TimelineAction {
    pub pause: bool,
    pub seek: Option<f64>,
}

// Keep the video endpoint and its drag handles clear of the floating scrollbar.
const TIMELINE_RIGHT_MARGIN: f32 = 24.0;
const HANDLE_EDGE_INSET: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handle {
    Start,
    End,
    FadeIn,
    FadeOut,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TimelineDrag {
    index: usize,
    handle: Handle,
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
        action.pause |= self.tools(ui);
        ui.horizontal_wrapped(|ui| {
            if let Some(index) = self.selected.filter(|i| *i < project.annotations.len()) {
                let a = &mut project.annotations[index];
                ui.label(a.display_name(index));
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
            let (allocated, response) = ui.allocate_exact_size(
                egui::vec2(timeline_width(ui.available_width()), 20.0),
                egui::Sense::click_and_drag(),
            );
            let rect = time_rect(allocated);
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
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if project.annotations.is_empty() {
                    ui.label("No annotations. Choose a tool above, then draw on the video.");
                }
                for index in (0..project.annotations.len()).rev() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            let a = &project.annotations[index];
                            let start = a.start_seconds;
                            let end = a.end_seconds;
                            let fade_in = a.effects.fade_in;
                            let fade_out = a.effects.fade_out;
                            let color = Color32::from_rgb(a.color[0], a.color[1], a.color[2]);
                            let label = a.display_name(index);
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
                            let (allocated, response) = ui.allocate_exact_size(
                                egui::vec2(timeline_width(ui.available_width()), 30.0),
                                egui::Sense::click_and_drag(),
                            );
                            let rect = time_rect(allocated);
                            let p = ui.painter();
                            p.rect_filled(rect, 2.0, Color32::from_gray(35));
                            let x = |t: f64| x_at_time(rect, t, duration);
                            let bar = Rect::from_min_max(
                                egui::pos2(x(start), rect.top() + 3.0),
                                egui::pos2(
                                    x(end).max(x(start) + 1.0).min(rect.right()),
                                    rect.bottom() - 3.0,
                                ),
                            );
                            let positions =
                                handle_positions(rect, start, end, fade_in, fade_out, duration);
                            let profile = fade_profile(start, end, fade_in, fade_out);
                            let mut filled = Vec::with_capacity(profile.len() + 2);
                            if profile.first().is_some_and(|&(_, alpha)| alpha > 0.0) {
                                filled.push(egui::pos2(x(start), bar.bottom()));
                            }
                            filled.extend(profile.iter().map(|&(t, alpha)| {
                                egui::pos2(x(t), bar.bottom() - alpha as f32 * bar.height())
                            }));
                            if profile.last().is_some_and(|&(_, alpha)| alpha > 0.0) {
                                filled.push(egui::pos2(x(end), bar.bottom()));
                            }
                            p.add(egui::Shape::convex_polygon(
                                filled,
                                color.gamma_multiply(0.75),
                                Stroke::NONE,
                            ));
                            let focused = self.selected == Some(index);
                            if focused {
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
                            if focused {
                                for (handle, center) in positions {
                                    let fill = match handle {
                                        Handle::Start | Handle::End => Color32::WHITE,
                                        Handle::FadeIn | Handle::FadeOut => Color32::LIGHT_BLUE,
                                    };
                                    p.circle_filled(center, 4.5, fill);
                                    p.circle_stroke(center, 4.5, Stroke::new(1.0, Color32::BLACK));
                                }
                            }
                            let hovered = focused
                                .then(|| {
                                    response
                                        .hover_pos()
                                        .and_then(|pos| handle_at(positions, pos))
                                })
                                .flatten();
                            if hovered.is_some()
                                || self.timeline_drag.is_some_and(|drag| drag.index == index)
                            {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            }
                            let tooltip = match hovered {
                                Some(Handle::Start) => {
                                    format!("Start: {start:.3} s — drag to change")
                                }
                                Some(Handle::End) => format!("End: {end:.3} s — drag to change"),
                                Some(Handle::FadeIn) => {
                                    format!("Fade in: {fade_in:.3} s — drag to change")
                                }
                                Some(Handle::FadeOut) => {
                                    format!("Fade out: {fade_out:.3} s — drag to change")
                                }
                                None => format!("{start:.3} – {end:.3} s"),
                            };
                            let response = response.on_hover_text(tooltip);
                            if response.drag_started()
                                && let Some(pos) = ui
                                    .input(|i| i.pointer.press_origin())
                                    .or_else(|| response.interact_pointer_pos())
                            {
                                self.timeline_drag = focused
                                    .then(|| handle_at(positions, pos))
                                    .flatten()
                                    .map(|handle| TimelineDrag { index, handle });
                                self.select(project, index);
                                if self.timeline_drag.is_some() {
                                    action.pause = true;
                                }
                            }
                            if response.dragged()
                                && let Some(pos) = response.interact_pointer_pos()
                            {
                                if let Some(drag) =
                                    self.timeline_drag.filter(|drag| drag.index == index)
                                {
                                    apply_handle(
                                        &mut project.annotations[index],
                                        drag.handle,
                                        time_at_x(rect, pos.x, duration),
                                        duration,
                                    );
                                    action.pause = true;
                                } else {
                                    self.select(project, index);
                                    action.seek = Some(time_at_x(rect, pos.x, duration));
                                }
                            } else if response.clicked()
                                && let Some(pos) = response.interact_pointer_pos()
                            {
                                self.select(project, index);
                                if !focused || handle_at(positions, pos).is_none() {
                                    action.seek = Some(time_at_x(rect, pos.x, duration));
                                } else {
                                    action.pause = true;
                                }
                            }
                            if response.drag_stopped() {
                                self.timeline_drag = None;
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

fn timeline_width(available: f32) -> f32 {
    (available - TIMELINE_RIGHT_MARGIN).max(1.0)
}

fn time_rect(allocated: Rect) -> Rect {
    let inset = HANDLE_EDGE_INSET.min((allocated.width() / 4.0).max(0.0));
    Rect::from_min_max(
        egui::pos2(allocated.left() + inset, allocated.top()),
        egui::pos2(allocated.right() - inset, allocated.bottom()),
    )
}

fn x_at_time(rect: Rect, time: f64, duration: f64) -> f32 {
    rect.left() + rect.width() * (time / duration.max(f64::EPSILON)).clamp(0.0, 1.0) as f32
}

fn fade_profile(start: f64, end: f64, fade_in: f64, fade_out: f64) -> Vec<(f64, f64)> {
    let mut times = vec![start, end];
    if fade_in > 0.0 {
        times.push((start + fade_in).min(end));
    }
    if fade_out > 0.0 {
        times.push((end - fade_out).max(start));
    }
    if fade_in > 0.0 && fade_out > 0.0 {
        times.push(start + (end - start) * fade_in / (fade_in + fade_out));
    }
    times.sort_by(f64::total_cmp);
    times.dedup();
    times
        .into_iter()
        .map(|t| {
            let incoming = if fade_in > 0.0 {
                ((t - start) / fade_in).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let outgoing = if fade_out > 0.0 {
                ((end - t) / fade_out).clamp(0.0, 1.0)
            } else {
                1.0
            };
            (t, incoming.min(outgoing))
        })
        .collect()
}

fn handle_positions(
    rect: Rect,
    start: f64,
    end: f64,
    fade_in: f64,
    fade_out: f64,
    duration: f64,
) -> [(Handle, Pos2); 4] {
    let top = rect.top() + 6.0;
    let bottom = rect.bottom() - 8.0;
    [
        (
            Handle::Start,
            egui::pos2(x_at_time(rect, start, duration), bottom),
        ),
        (
            Handle::End,
            egui::pos2(x_at_time(rect, end, duration), bottom),
        ),
        (
            Handle::FadeIn,
            egui::pos2(x_at_time(rect, (start + fade_in).min(end), duration), top),
        ),
        (
            Handle::FadeOut,
            egui::pos2(x_at_time(rect, (end - fade_out).max(start), duration), top),
        ),
    ]
}

fn handle_at(positions: [(Handle, Pos2); 4], pos: Pos2) -> Option<Handle> {
    positions
        .into_iter()
        .filter_map(|(handle, center)| {
            let distance = center.distance_sq(pos);
            (distance <= 8.0_f32.powi(2)).then_some((handle, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| handle)
}

fn apply_handle(a: &mut Annotation, handle: Handle, time: f64, duration: f64) {
    match handle {
        Handle::Start => a.set_start(time, duration),
        Handle::End => a.set_end(time, duration),
        Handle::FadeIn => {
            a.effects.fade_in = (time - a.start_seconds).clamp(0.0, a.end_seconds - a.start_seconds)
        }
        Handle::FadeOut => {
            a.effects.fade_out = (a.end_seconds - time).clamp(0.0, a.end_seconds - a.start_seconds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use video_annotations::annotations::Kind;
    #[test]
    fn ruler_maps_to_bounded_video_time() {
        let rect = Rect::from_min_size(egui::pos2(120.0, 0.0), egui::vec2(400.0, 20.0));
        assert_eq!(time_at_x(rect, 320.0, 10.0), 5.0);
        assert_eq!(time_at_x(rect, 0.0, 10.0), 0.0);
        assert_eq!(time_at_x(rect, 900.0, 10.0), 10.0);
    }

    #[test]
    fn timeline_keeps_endpoint_handles_inside_track_and_away_from_scrollbar() {
        let panel_right = 500.0;
        let allocated = Rect::from_min_size(
            egui::pos2(100.0, 0.0),
            egui::vec2(timeline_width(panel_right - 100.0), 30.0),
        );
        let rect = time_rect(allocated);
        let positions = handle_positions(rect, 0.0, 10.0, 0.0, 0.0, 10.0);
        assert_eq!(positions[1].1.x, rect.right());
        assert!(allocated.contains(positions[1].1));
        assert!(panel_right - positions[1].1.x >= TIMELINE_RIGHT_MARGIN + HANDLE_EDGE_INSET);
        assert_eq!(time_at_x(rect, positions[1].1.x, 10.0), 10.0);
    }

    #[test]
    fn timing_and_fade_points_remain_distinct_when_fades_are_zero() {
        let rect = Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(400.0, 30.0));
        let positions = handle_positions(rect, 2.0, 8.0, 0.0, 0.0, 10.0);
        for (handle, center) in positions {
            assert_eq!(handle_at(positions, center), Some(handle));
        }
        let positions = handle_positions(rect, 2.0, 8.0, 1.0, 1.5, 10.0);
        assert_eq!(positions[2].1.x, x_at_time(rect, 3.0, 10.0));
        assert_eq!(positions[3].1.x, x_at_time(rect, 6.5, 10.0));
    }

    #[test]
    fn fade_profile_has_one_ramp_per_side_without_overlay_triangles() {
        assert_eq!(
            fade_profile(2.0, 8.0, 1.0, 2.0),
            vec![(2.0, 0.0), (3.0, 1.0), (4.0, 1.0), (6.0, 1.0), (8.0, 0.0)]
        );
        assert_eq!(
            fade_profile(2.0, 8.0, 4.0, 4.0),
            vec![(2.0, 0.0), (4.0, 0.5), (5.0, 0.75), (6.0, 0.5), (8.0, 0.0)]
        );
        assert_eq!(
            fade_profile(2.0, 8.0, 0.0, 0.0),
            vec![(2.0, 1.0), (8.0, 1.0)]
        );
    }

    #[test]
    fn dragging_points_edits_timing_or_fade_without_inverting_interval() {
        let mut a = Annotation::new(Kind::Rectangle, [0.0; 2], [10.0; 2], 10.0);
        a.start_seconds = 2.0;
        a.end_seconds = 8.0;
        apply_handle(&mut a, Handle::Start, 9.0, 10.0);
        assert_eq!(a.start_seconds, 7.999);
        apply_handle(&mut a, Handle::Start, 2.0, 10.0);
        apply_handle(&mut a, Handle::End, 6.0, 10.0);
        assert_eq!(a.end_seconds, 6.0);
        apply_handle(&mut a, Handle::FadeIn, 4.0, 10.0);
        apply_handle(&mut a, Handle::FadeOut, 5.0, 10.0);
        assert_eq!(a.effects.fade_in, 2.0);
        assert_eq!(a.effects.fade_out, 1.0);
        apply_handle(&mut a, Handle::FadeOut, 0.0, 10.0);
        assert_eq!(a.effects.fade_out, 4.0);
        apply_handle(&mut a, Handle::FadeIn, 10.0, 10.0);
        assert_eq!(a.effects.fade_in, 4.0);
    }
}
