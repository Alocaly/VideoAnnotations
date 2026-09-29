//! Shared annotation painter, plus an offline rasterizer for egui's triangle meshes.
use crate::annotations::{Annotation, Kind};
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
    if a.color[3] == 0 {
        return;
    }
    if a.effects.outline_period > 0.0 && matches!(a.kind, Kind::Rectangle | Kind::Ellipse) {
        let mut dim = a.clone();
        dim.color[3] = (a.color[3] as f32 * 0.25) as u8;
        paint(p, &dim, viewport, extent);
        let phase = ((time - a.start_seconds).max(0.0) / a.effects.outline_period).fract() as f32;
        let (min, max) = a.bounds();
        let points = (0..=64)
            .map(|i| {
                let t = (phase + i as f32 / 256.0).fract();
                let point = if a.kind == Kind::Ellipse {
                    let angle = t * std::f32::consts::TAU;
                    [
                        (min[0] + max[0]) * 0.5 + angle.cos() * (max[0] - min[0]) * 0.5,
                        (min[1] + max[1]) * 0.5 + angle.sin() * (max[1] - min[1]) * 0.5,
                    ]
                } else {
                    let w = max[0] - min[0];
                    let h = max[1] - min[1];
                    let d = t * 2.0 * (w + h);
                    if d < w {
                        [min[0] + d, min[1]]
                    } else if d < w + h {
                        [max[0], min[1] + d - w]
                    } else if d < 2.0 * w + h {
                        [max[0] - (d - w - h), max[1]]
                    } else {
                        [min[0], max[1] - (d - 2.0 * w - h)]
                    }
                };
                viewport.min
                    + Vec2::new(
                        point[0] / extent[0] * viewport.width(),
                        point[1] / extent[1] * viewport.height(),
                    )
            })
            .collect();
        p.add(egui::Shape::line(
            points,
            Stroke::new(
                a.thickness * viewport.width() / extent[0],
                Color32::from_rgba_unmultiplied(a.color[0], a.color[1], a.color[2], a.color[3]),
            ),
        ));
    } else {
        paint(p, &a, viewport, extent);
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
            let from = screen(a.a);
            let to = screen(a.b);
            let d = to - from;
            p.line_segment([from, to], stroke);
            if d.length() > 0.1 {
                let unit = d.normalized();
                let head = (16.0 * scale).max(stroke.width * 3.0).min(d.length() * 0.4);
                let normal = Vec2::new(-unit.y, unit.x);
                p.line_segment([to, to - unit * head + normal * head * 0.5], stroke);
                p.line_segment([to, to - unit * head - normal * head * 0.5], stroke);
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
    fn animated_scene_handles_empty_frames_movement_fades_glow_and_outline() {
        let cancel = AtomicBool::new(false);
        assert!(
            rasterize_scene(&[], [160, 100], 0.0, 2.0, &cancel)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
        let mut a = Annotation::new(Kind::Rectangle, [20.0, 20.0], [80.0, 70.0], 2.0);
        a.effects.fade_in = 1.0;
        a.effects.movement = [40.0, 0.0];
        let first = rasterize_scene(&[a.clone()], [160, 100], 0.0, 2.0, &cancel).unwrap();
        assert!(first.pixels().all(|p| p[3] == 0));
        let middle = rasterize_scene(&[a.clone()], [160, 100], 1.0, 2.0, &cancel).unwrap();
        assert_eq!(middle.get_pixel(50, 21)[3], 255);
        assert_eq!(middle.get_pixel(25, 21)[3], 0);
        a.effects.glow = 30.0;
        let pulsed = rasterize_scene(&[a.clone()], [160, 100], 1.0, 2.0, &cancel).unwrap();
        assert_eq!(pulsed.get_pixel(60, 21)[3], middle.get_pixel(60, 21)[3]);
        assert_ne!(
            pulsed.get_pixel(60, 21).0[..3],
            middle.get_pixel(60, 21).0[..3]
        );
        assert_eq!(
            pulsed.get_pixel(30, 21)[3],
            0,
            "Color pulse must not add a halo"
        );
        a.effects = Default::default();
        a.effects.outline_period = 1.0;
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
