//! Shared annotation painter, plus an offline rasterizer for egui's triangle meshes.
use crate::annotations::{Annotation, Effect, Kind};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};

/// Time-based rendering shared by the editor, fullscreen, MP4 and GIF export.
pub fn paint_at(
    p: &egui::Painter,
    source: &Annotation,
    viewport: Rect,
    extent: [f32; 2],
    time: f64,
) {
    let a = source.evaluated(time);
    if a.color[3] == 0 && a.kind != Kind::Spotlight {
        return;
    }
    paint(p, &a, viewport, extent);
    if let Effect::Orbit { color, period } = a.effects.effect
        && a.kind.supports_orbit()
    {
        let phase = ((time - a.start_seconds).max(0.0) / period).fract() as f32;
        let point = orbit_point(&a, phase);
        let center = viewport.min
            + Vec2::new(
                point[0] / extent[0] * viewport.width(),
                point[1] / extent[1] * viewport.height(),
            );
        p.circle_filled(
            center,
            (a.thickness * 1.5).max(4.0) * viewport.width() / extent[0],
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], a.color[3]),
        );
    }
}

fn orbit_point(a: &Annotation, phase: f32) -> [f32; 2] {
    if a.kind == Kind::Line {
        // Travel to the other endpoint and back, without jumping at loop end.
        let t = 1.0 - (2.0 * phase - 1.0).abs();
        return std::array::from_fn(|i| a.a[i] + (a.b[i] - a.a[i]) * t);
    }
    let (min, max) = a.bounds();
    if a.kind == Kind::Ellipse {
        let angle = phase * std::f32::consts::TAU;
        let radii = Vec2::new((max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5);
        // egui paints ellipse strokes outside the path. Offset along the
        // ellipse normal (not its radial direction) to follow the stroke center.
        let normal = Vec2::new(
            angle.cos() / radii.x.max(f32::EPSILON),
            angle.sin() / radii.y.max(f32::EPSILON),
        )
        .normalized();
        let offset = normal * (a.thickness * 0.5);
        [
            (min[0] + max[0]) * 0.5 + angle.cos() * radii.x + offset.x,
            (min[1] + max[1]) * 0.5 + angle.sin() * radii.y + offset.y,
        ]
    } else {
        // Rectangle strokes are inside the bounds. Keep the centerline inside
        // even when the stroke is wider than the shape.
        let inset = [
            (a.thickness * 0.5).min((max[0] - min[0]) * 0.5),
            (a.thickness * 0.5).min((max[1] - min[1]) * 0.5),
        ];
        let min = [min[0] + inset[0], min[1] + inset[1]];
        let max = [max[0] - inset[0], max[1] - inset[1]];
        let w = max[0] - min[0];
        let h = max[1] - min[1];
        let d = phase * 2.0 * (w + h);
        if d < w {
            [min[0] + d, min[1]]
        } else if d < w + h {
            [max[0], min[1] + d - w]
        } else if d < 2.0 * w + h {
            [max[0] - (d - w - h), max[1]]
        } else {
            [min[0], max[1] - (d - 2.0 * w - h)]
        }
    }
}

pub fn paint(p: &egui::Painter, a: &Annotation, viewport: Rect, extent: [f32; 2]) {
    let scale = viewport.width() / extent[0];
    let screen = |point: [f32; 2]| {
        viewport.min
            + Vec2::new(
                point[0] / extent[0] * viewport.width(),
                point[1] / extent[1] * viewport.height(),
            )
    };
    let color = Color32::from_rgba_unmultiplied(a.color[0], a.color[1], a.color[2], a.color[3]);
    let stroke = Stroke::new(a.thickness * scale, color);
    let (min, max) = a.bounds();
    let rect = Rect::from_two_pos(screen(min), screen(max));
    match a.kind {
        Kind::Spotlight => {
            let hole = rect.intersect(viewport);
            let shade = Color32::from_black_alpha((a.dimming * 255.0).round() as u8);
            // Four non-overlapping panels leave the selected region untouched.
            for panel in [
                Rect::from_min_max(viewport.min, Pos2::new(viewport.max.x, hole.min.y)),
                Rect::from_min_max(Pos2::new(viewport.min.x, hole.max.y), viewport.max),
                Rect::from_min_max(
                    Pos2::new(viewport.min.x, hole.min.y),
                    Pos2::new(hole.min.x, hole.max.y),
                ),
                Rect::from_min_max(
                    Pos2::new(hole.max.x, hole.min.y),
                    Pos2::new(viewport.max.x, hole.max.y),
                ),
            ] {
                if panel.is_positive() {
                    p.rect_filled(panel, 0.0, shade);
                }
            }
        }
        Kind::Line => {
            p.line_segment([screen(a.a), screen(a.b)], stroke);
        }
        Kind::Rectangle => {
            p.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        }
        Kind::Ellipse => {
            p.add(egui::Shape::ellipse_stroke(
                rect.center(),
                rect.size() * 0.5,
                stroke,
            ));
        }
        Kind::Arrow => {
            if screen(a.a).distance(screen(a.b)) > 0.1 {
                let points: Vec<_> = crate::arrow::outline(a).into_iter().map(screen).collect();
                p.add(crate::arrow::mesh(
                    &points,
                    color,
                    1.0 / p.ctx().pixels_per_point(),
                ));
            }
        }
        Kind::Text => {
            let galley = p.layout(
                a.text.clone(),
                egui::FontId::proportional(a.font_size * scale),
                color,
                rect.width().max(1.0),
            );
            p.with_clip_rect(rect.intersect(viewport))
                .galley(rect.min, galley, color);
        }
    }
}

/// Rasterizes the very same shapes and bundled font atlas used by the preview.
pub fn rasterize(
    a: &Annotation,
    size: [u32; 2],
    cancel: &AtomicBool,
) -> Result<image::RgbaImage, String> {
    rasterize_scene(
        std::slice::from_ref(a),
        size,
        a.start_seconds,
        a.end_seconds,
        cancel,
    )
}

pub fn rasterize_scene(
    annotations: &[Annotation],
    size: [u32; 2],
    time: f64,
    duration: f64,
    cancel: &AtomicBool,
) -> Result<image::RgbaImage, String> {
    let [width, height] = size;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 9_000_000 {
        return Err("Export supports up to 9 megapixels.".into());
    }
    let ctx = egui::Context::default();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(width as f32, height as f32));
    let input = egui::RawInput {
        screen_rect: Some(rect),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        for a in annotations.iter().filter(|a| a.visible_at(time, duration)) {
            paint_at(
                &ui.painter().with_clip_rect(rect),
                a,
                rect,
                [width as f32, height as f32],
                time,
            );
        }
    });
    let deltas = output
        .textures_delta
        .set
        .remove(&egui::TextureId::default());
    output.textures_delta.clear();
    let deltas = deltas.ok_or("Missing font atlas during export")?;
    let mut atlas = egui::ColorImage::filled([1, 1], Color32::TRANSPARENT);
    for delta in deltas {
        let egui::ImageData::Color(image) = &delta.image;
        if let Some([x, y]) = delta.pos {
            for row in 0..image.size[1] {
                let start = (y + row) * atlas.size[0] + x;
                atlas.pixels[start..start + image.size[0]]
                    .copy_from_slice(&image.pixels[row * image.size[0]..(row + 1) * image.size[0]]);
            }
        } else {
            atlas = (**image).clone();
        }
    }
    let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
    let mut pixels = vec![[0.0f32; 4]; width as usize * height as usize];
    for clipped in meshes {
        let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive else {
            return Err("Unsupported paint callback".into());
        };
        if mesh.texture_id != egui::TextureId::default() {
            return Err("Unsupported paint texture".into());
        }
        let clip = clipped.clip_rect.intersect(rect);
        for ids in mesh.indices.as_chunks::<3>().0 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Export canceled".into());
            }
            let mut v = [
                mesh.vertices[ids[0] as usize],
                mesh.vertices[ids[1] as usize],
                mesh.vertices[ids[2] as usize],
            ];
            let mut area = edge(v[0].pos, v[1].pos, v[2].pos);
            if area.abs() < 1e-8 {
                continue;
            }
            if area < 0.0 {
                v.swap(1, 2);
                area = -area;
            }
            let x0 = v
                .iter()
                .map(|v| v.pos.x)
                .fold(f32::INFINITY, f32::min)
                .max(clip.left())
                .floor()
                .max(0.0) as u32;
            let x1 = v
                .iter()
                .map(|v| v.pos.x)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(clip.right())
                .ceil()
                .min(width as f32) as u32;
            let y0 = v
                .iter()
                .map(|v| v.pos.y)
                .fold(f32::INFINITY, f32::min)
                .max(clip.top())
                .floor()
                .max(0.0) as u32;
            let y1 = v
                .iter()
                .map(|v| v.pos.y)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(clip.bottom())
                .ceil()
                .min(height as f32) as u32;
            for y in y0..y1 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Export canceled".into());
                }
                for x in x0..x1 {
                    let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                    if !clip.contains(p) {
                        continue;
                    }
                    let e = [
                        edge(v[1].pos, v[2].pos, p),
                        edge(v[2].pos, v[0].pos, p),
                        edge(v[0].pos, v[1].pos, p),
                    ];
                    // Top-left coverage avoids blending shared triangle edges twice.
                    if !(0..3).all(|i| {
                        e[i] > 0.0
                            || (e[i] == 0.0 && top_left(v[(i + 1) % 3].pos, v[(i + 2) % 3].pos))
                    }) {
                        continue;
                    }
                    let weights = e.map(|e| e / area);
                    let uv = v[0].uv.to_vec2() * weights[0]
                        + v[1].uv.to_vec2() * weights[1]
                        + v[2].uv.to_vec2() * weights[2];
                    let texture = sample(&atlas, uv);
                    let mut color = [0.0; 4];
                    for c in 0..4 {
                        color[c] = (0..3)
                            .map(|i| v[i].color.to_array()[c] as f32 / 255.0 * weights[i])
                            .sum::<f32>()
                            * texture[c];
                    }
                    let dst = &mut pixels[(y * width + x) as usize];
                    for c in 0..4 {
                        dst[c] = color[c] + dst[c] * (1.0 - color[3]);
                    }
                }
            }
        }
    }
    let mut image = image::RgbaImage::new(width, height);
    for (out, p) in image.pixels_mut().zip(pixels) {
        *out = image::Rgba(if p[3] > 0.0 {
            [
                (p[0] / p[3] * 255.0).round() as u8,
                (p[1] / p[3] * 255.0).round() as u8,
                (p[2] / p[3] * 255.0).round() as u8,
                (p[3] * 255.0).round() as u8,
            ]
        } else {
            [0; 4]
        });
    }
    Ok(image)
}
fn edge(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
fn top_left(a: Pos2, b: Pos2) -> bool {
    b.y < a.y || (b.y == a.y && b.x > a.x)
}
fn sample(image: &egui::ColorImage, uv: Vec2) -> [f32; 4] {
    let x = (uv.x * image.size[0] as f32 - 0.5).clamp(0.0, (image.size[0] - 1) as f32);
    let y = (uv.y * image.size[1] as f32 - 0.5).clamp(0.0, (image.size[1] - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(image.size[0] - 1);
    let y1 = (y0 + 1).min(image.size[1] - 1);
    let fx = x.fract();
    let fy = y.fract();
    std::array::from_fn(|c| {
        [
            (x0, y0, (1.0 - fx) * (1.0 - fy)),
            (x1, y0, fx * (1.0 - fy)),
            (x0, y1, (1.0 - fx) * fy),
            (x1, y1, fx * fy),
        ]
        .iter()
        .map(|&(x, y, w)| image.pixels[y * image.size[0] + x].to_array()[c] as f32 / 255.0 * w)
        .sum()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gallery_four_arrow_has_a_filled_shaft_and_connected_head() {
        let cancel = AtomicBool::new(false);
        let mut a = Annotation::new(Kind::Arrow, [20.0, 50.0], [140.0, 50.0], 2.0);
        a.color = [255, 0, 0, 255];
        let image = rasterize_scene(&[a.clone()], [160, 100], 1.0, 2.0, &cancel).unwrap();
        assert_eq!(image.get_pixel(80, 50)[3], 255, "Central shaft is filled");
        assert_eq!(image.get_pixel(80, 58)[3], 0);
        assert!(image.get_pixel(120, 54)[3] > 200);
        assert!(image.get_pixel(120, 46)[3] > 200);
        assert!(a.hit([120.0, 54.0], 1.0), "Wing can be selected");
        assert!(a.hit([80.0, 50.0], 1.0), "Interior can be selected");
        assert!(!a.hit([80.0, 80.0], 1.0));
        a.b = a.a;
        assert!(!a.hit([100.0, 80.0], 1.0));
    }
    #[test]
    fn gallery_four_arrow_keeps_proportions_and_uniform_fade() {
        let cancel = AtomicBool::new(false);
        let mut a = Annotation::new(Kind::Arrow, [20.0, 100.0], [720.0, 100.0], 2.0);
        a.effects.fade_in = 2.0;
        let image =
            rasterize_scene(std::slice::from_ref(&a), [760, 220], 1.0, 2.0, &cancel).unwrap();
        assert_eq!(image.get_pixel(670, 115)[3], 128);
        assert_eq!(image.get_pixel(670, 100)[3], 128);
        assert_eq!(image.get_pixel(350, 100)[3], 128);
        assert_eq!(
            image.get_pixel(560, 140)[3],
            0,
            "Concave neck is not filled"
        );
        assert!(a.hit([670.0, 115.0], 1.0));
    }
    #[test]
    fn spotlight_preserves_inner_pixels_and_fades_outer_dimming() {
        let cancel = AtomicBool::new(false);
        let mut a = Annotation::new(Kind::Spotlight, [40.0, 30.0], [120.0, 70.0], 4.0);
        a.dimming = 0.8;
        a.effects.fade_in = 1.0;
        a.effects.fade_out = 1.0;
        for (time, alpha) in [(0.0, 0), (0.5, 102), (2.0, 204), (3.5, 102), (4.0, 0)] {
            let image = rasterize_scene(&[a.clone()], [160, 100], time, 4.0, &cancel).unwrap();
            assert_eq!(image.get_pixel(80, 50).0, [0; 4]);
            for (x, y) in [(80, 10), (80, 90), (10, 50), (150, 50), (10, 10)] {
                assert_eq!(image.get_pixel(x, y).0, [0, 0, 0, alpha]);
            }
        }
        a.dimming = 0.0;
        assert!(
            rasterize_scene(&[a], [160, 100], 2.0, 4.0, &cancel)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
    }

    #[test]
    fn line_ball_returns_without_jumping_and_stroke_has_no_arrowhead() {
        let cancel = AtomicBool::new(false);
        let mut a = Annotation::new(Kind::Line, [20.0, 50.0], [120.0, 50.0], 4.0);
        a.color = [255, 0, 0, 255];
        assert_eq!(orbit_point(&a, 0.0), a.a);
        assert_eq!(orbit_point(&a, 0.5), a.b);
        assert_eq!(orbit_point(&a, 0.25), [70.0, 50.0]);
        assert_eq!(orbit_point(&a, 0.75), [70.0, 50.0]);
        assert_eq!(orbit_point(&a, 1.0), a.a);
        let plain = rasterize_scene(&[a.clone()], [160, 100], 0.0, 4.0, &cancel).unwrap();
        assert_eq!(plain.get_pixel(70, 50).0, [255, 0, 0, 255]);
        assert_eq!(plain.get_pixel(108, 55)[3], 0);
        a.effects.effect = Effect::Orbit {
            color: [0, 0, 255],
            period: 2.0,
        };
        let ball = rasterize_scene(&[a], [160, 100], 0.5, 4.0, &cancel).unwrap();
        assert_eq!(ball.get_pixel(70, 50).0, [0, 0, 255, 255]);
    }
    #[test]
    fn orbit_follows_stroke_center_for_rectangles_and_ellipses() {
        let mut a = Annotation::new(Kind::Rectangle, [20.0, 20.0], [100.0, 70.0], 2.0);
        a.thickness = 10.0;
        assert_eq!(orbit_point(&a, 0.0), [25.0, 25.0]);
        assert_eq!(orbit_point(&a, 0.5), [95.0, 65.0]);
        a.thickness = 200.0;
        assert_eq!(orbit_point(&a, 0.3), [60.0, 45.0]);

        a.kind = Kind::Ellipse;
        a.thickness = 10.0;
        assert_eq!(orbit_point(&a, 0.0), [105.0, 45.0]);
        let bottom = orbit_point(&a, 0.25);
        assert!((bottom[0] - 60.0).abs() < 0.001);
        assert!((bottom[1] - 75.0).abs() < 0.001);
        let angle = std::f32::consts::FRAC_PI_4;
        let point = orbit_point(&a, 0.125);
        let path = Vec2::new(60.0 + 40.0 * angle.cos(), 45.0 + 25.0 * angle.sin());
        let offset = Vec2::new(point[0], point[1]) - path;
        let tangent = Vec2::new(-40.0 * angle.sin(), 25.0 * angle.cos());
        assert!((offset.length() - 5.0).abs() < 0.001);
        assert!(offset.dot(tangent).abs() < 0.001);
    }
    #[test]
    fn animated_scene_handles_fades_glow_and_colored_orbiting_ball() {
        let cancel = AtomicBool::new(false);
        assert!(
            rasterize_scene(&[], [160, 100], 0.0, 2.0, &cancel)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
        let mut a = Annotation::new(Kind::Rectangle, [20.0, 20.0], [80.0, 70.0], 2.0);
        a.effects.fade_in = 1.0;
        let first = rasterize_scene(&[a.clone()], [160, 100], 0.0, 2.0, &cancel).unwrap();
        assert!(first.pixels().all(|p| p[3] == 0));
        let middle = rasterize_scene(&[a.clone()], [160, 100], 1.0, 2.0, &cancel).unwrap();
        assert_eq!(middle.get_pixel(50, 21)[3], 255);
        assert_eq!(middle.get_pixel(25, 21)[3], 255);
        a.effects.effect = Effect::Glow {
            color: [50, 100, 240],
            pulse_hz: 0.5,
        };
        let pulsed = rasterize_scene(&[a.clone()], [160, 100], 1.0, 2.0, &cancel).unwrap();
        assert_eq!(pulsed.get_pixel(60, 21)[3], middle.get_pixel(60, 21)[3]);
        assert_ne!(
            pulsed.get_pixel(60, 21).0[..3],
            middle.get_pixel(60, 21).0[..3]
        );
        assert_eq!(
            pulsed.get_pixel(30, 12)[3],
            0,
            "Color pulse must not add a halo"
        );
        a.effects = Default::default();
        a.effects.effect = Effect::Orbit {
            color: [0, 255, 0],
            period: 1.0,
        };
        let orbit = rasterize_scene(&[a.clone()], [160, 100], 0.0, 2.0, &cancel).unwrap();
        assert_eq!(orbit.get_pixel(20, 20).0, [0, 255, 0, 255]);
        assert_eq!(
            orbit.get_pixel(50, 21)[3],
            255,
            "Contour stays fully visible"
        );
        assert_ne!(
            rasterize_scene(&[a.clone()], [160, 100], 0.0, 2.0, &cancel).unwrap(),
            rasterize_scene(&[a], [160, 100], 0.5, 2.0, &cancel).unwrap()
        );
    }
    #[test]
    fn all_shapes_render_and_respect_transparency_and_text_clipping() {
        let cancel = AtomicBool::new(false);
        for kind in [Kind::Text, Kind::Rectangle, Kind::Ellipse, Kind::Arrow] {
            let mut a = Annotation::new(kind, [20.0, 20.0], [100.0, 70.0], 1.0);
            a.text = "Hello\nclipped text".into();
            a.color = [255, 0, 0, 128];
            let image = rasterize(&a, [160, 100], &cancel).unwrap();
            assert!(image.pixels().any(|p| p[3] > 80), "{kind:?} must render");
            assert_eq!(image.get_pixel(0, 0)[3], 0);
            if kind == Kind::Text {
                assert_eq!(image.get_pixel(30, 75)[3], 0);
            }
            if kind == Kind::Rectangle {
                assert_eq!(image.get_pixel(50, 50)[3], 0);
                let pixel = image.get_pixel(50, 21);
                assert!(
                    pixel[0] > 250 && (120..=135).contains(&pixel[3]),
                    "{pixel:?}"
                );
            }
            a.color[3] = 0;
            assert!(
                rasterize(&a, [160, 100], &cancel)
                    .unwrap()
                    .pixels()
                    .all(|p| p[3] == 0)
            );
        }
    }
    #[test]
    fn rasterization_can_be_canceled() {
        let a = Annotation::new(Kind::Rectangle, [20.0, 20.0], [100.0, 70.0], 1.0);
        assert!(
            rasterize(&a, [160, 100], &AtomicBool::new(true))
                .unwrap_err()
                .contains("canceled")
        );
    }
}
