use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use video_annotations::{
    annotations::{Annotation, Kind},
    project::Project,
};

#[derive(Default)]
pub struct Editor {
    tool: Option<Kind>,
    pub(crate) selected: Option<usize>,
    drag: Option<Drag>,
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
        self.cancel(project);
        self.selected = Some(index);
    }

    pub(crate) fn reorder(&mut self, project: &mut Project, forward: bool) {
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
    pub fn toolbar(&mut self, ui: &mut egui::Ui, project: &mut Project) -> bool {
        let mut pause = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Annotations");
            if ui.selectable_label(self.tool.is_none(), "Select").clicked() {
                self.tool = None;
            }
            for kind in [Kind::Text, Kind::Rectangle, Kind::Ellipse, Kind::Arrow] {
                if ui
                    .selectable_label(self.tool == Some(kind), kind.label())
                    .clicked()
                {
                    self.tool = Some(kind);
                    pause = true;
                }
            }
            egui::ComboBox::from_id_salt("annotation-selection")
                .selected_text(
                    self.selected
                        .and_then(|i| project.annotations.get(i))
                        .map(|a| a.kind.label())
                        .unwrap_or("No selection"),
                )
                .show_ui(ui, |ui| {
                    for (i, a) in project.annotations.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.selected == Some(i),
                                format!("{}: {}", i + 1, a.kind.label()),
                            )
                            .clicked()
                        {
                            self.selected = Some(i);
                            self.tool = None;
                            pause = true;
                        }
                    }
                });
            if ui
                .add_enabled(self.selected.is_some(), egui::Button::new("Delete"))
                .clicked()
            {
                self.delete(project);
            }
        });
        if let Some(a) = self.selected.and_then(|i| project.annotations.get_mut(i)) {
            ui.horizontal_wrapped(|ui| {
                ui.label("Color");
                ui.color_edit_button_srgba_unmultiplied(&mut a.color);
                if a.kind == Kind::Text {
                    ui.label("Font size");
                    ui.add(
                        egui::DragValue::new(&mut a.font_size)
                            .range(6.0..=300.0)
                            .speed(1.0),
                    );
                } else {
                    ui.label("Thickness");
                    ui.add(
                        egui::DragValue::new(&mut a.thickness)
                            .range(1.0..=60.0)
                            .speed(0.5),
                    );
                }
                ui.label("Sizes in video pixels");
            });
            if a.kind == Kind::Text {
                ui.label("Text content");
                let response = ui.add(
                    egui::TextEdit::multiline(&mut a.text)
                        .id(egui::Id::new(("annotation-text", self.selected)))
                        .desired_rows(2)
                        .desired_width(ui.available_width()),
                );
                pause |= response.has_focus();
            }
        }
        ui.small(match self.tool {
            Some(Kind::Text)=>"Click the video to add text. Edit its content below the tools.",
            Some(_)=>"Drag on the video to draw. Esc cancels.",
            None=>"Click to select; drag to move; drag a handle to resize. Delete removes the selection.",
        });
        pause
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
        if ui.is_enabled() && !ui.ctx().text_edit_focused() {
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.delete(project);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cancel(project);
            }
        }
        if response.drag_started()
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
                            handles(a)
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
        if response.clicked()
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
                    if a.kind == Kind::Arrow {
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
        for a in project
            .annotations
            .iter()
            .filter(|a| a.visible_at(time, project.video.duration))
        {
            paint(&painter, a, map);
        }
        if let Some(a) = self
            .selected
            .and_then(|i| project.annotations.get(i))
            .filter(|a| a.visible_at(time, project.video.duration))
        {
            let (min, max) = a.bounds();
            painter.rect_stroke(
                Rect::from_two_pos(map.screen(min), map.screen(max)),
                0.0,
                Stroke::new(1.0, Color32::LIGHT_BLUE),
                egui::StrokeKind::Outside,
            );
            for p in handles(a) {
                painter.rect_filled(
                    Rect::from_center_size(map.screen(p), Vec2::splat(8.0)),
                    1.0,
                    Color32::LIGHT_BLUE,
                );
            }
        }
        if self.tool.is_some() && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        pause
    }
}

fn pick(project: &Project, p: [f32; 2], tolerance: f32, time: f64) -> Option<usize> {
    project
        .annotations
        .iter()
        .rposition(|a| a.visible_at(time, project.video.duration) && a.hit(p, tolerance))
}
fn handles(a: &Annotation) -> Vec<[f32; 2]> {
    if a.kind == Kind::Arrow {
        return vec![a.a, a.b];
    }
    let (min, max) = a.bounds();
    vec![min, [max[0], min[1]], max, [min[0], max[1]]]
}
fn valid(a: &Annotation, scale: f32) -> bool {
    let (min, max) = a.bounds();
    if a.kind == Kind::Arrow {
        ((max[0] - min[0]).hypot(max[1] - min[1])) * scale >= 3.0
    } else {
        (max[0] - min[0]) * scale >= 3.0 && (max[1] - min[1]) * scale >= 3.0
    }
}
fn paint(p: &egui::Painter, a: &Annotation, map: Mapping) {
    video_annotations::render::paint(p, a, map.rect, map.extent);
}

#[cfg(test)]
mod tests {
    use super::*;

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
                assert!(editor.toolbar(ui, &mut project));
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
