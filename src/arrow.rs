//! Native port of ArrowGallery method 4, including its quadratic SVG curves.
use crate::annotations::Annotation;
use eframe::egui::{Color32, Mesh, Pos2, Vec2, epaint::Vertex};

pub fn outline(a: &Annotation) -> Vec<[f32; 2]> {
    let d = Vec2::new(a.b[0] - a.a[0], a.b[1] - a.a[1]);
    let length = d.length();
    let unit = d.normalized();
    let normal = Vec2::new(-unit.y, unit.x);
    let sx = length / 223.5; // M8 40 to the quadratic tip at x=231.5.
    let sy = sx * (a.thickness / 4.0).sqrt();
    let mut points = vec![[8.0, 40.0], [193.0, 34.0], [177.0, 15.0]];
    let mut q = |from: [f32; 2], control: [f32; 2], to: [f32; 2], steps: usize| {
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            points.push(std::array::from_fn(|j| {
                (1.0 - t).powi(2) * from[j] + 2.0 * (1.0 - t) * t * control[j] + t * t * to[j]
            }));
        }
    };
    q([177.0, 15.0], [176.0, 13.0], [179.0, 15.0], 8);
    points.push([230.0, 38.0]);
    for i in 1..=16 {
        let t = i as f32 / 16.0;
        points.push([230.0 + 6.0 * t * (1.0 - t), 38.0 + 4.0 * t]);
    }
    points.push([179.0, 65.0]);
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        points.push([
            179.0 * (1.0 - t).powi(2) + 352.0 * (1.0 - t) * t + 177.0 * t * t,
            65.0 * (1.0 - t).powi(2) + 134.0 * (1.0 - t) * t + 65.0 * t * t,
        ]);
    }
    points.push([193.0, 46.0]);
    points
        .into_iter()
        .map(|p| {
            let v = Vec2::new(a.a[0], a.a[1])
                + unit * ((p[0] - 8.0) * sx)
                + normal * ((p[1] - 40.0) * sy);
            [v.x, v.y]
        })
        .collect()
}

fn cross(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    let u = b - a;
    let v = c - a;
    u.x * v.y - u.y * v.x
}

/// Ear clipping supports the concave neck; a single mesh prevents darker overlaps
/// during fades. Only the outside boundary gets an anti-aliasing fringe.
pub fn mesh(points: &[Pos2], color: Color32, feather: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let n = points.len();
    let vertex = |pos, color| Vertex {
        pos,
        uv: eframe::egui::epaint::WHITE_UV,
        color,
    };
    mesh.vertices
        .extend(points.iter().map(|&p| vertex(p, color)));
    let mut remaining: Vec<usize> = (0..n).collect();
    while remaining.len() > 2 {
        let mut ear = None;
        for k in 0..remaining.len() {
            let a = remaining[(k + remaining.len() - 1) % remaining.len()];
            let b = remaining[k];
            let c = remaining[(k + 1) % remaining.len()];
            if cross(points[a], points[b], points[c]) <= 0.0 {
                continue;
            }
            let contains = remaining.iter().any(|&j| {
                j != a
                    && j != b
                    && j != c
                    && cross(points[a], points[b], points[j]) >= 0.0
                    && cross(points[b], points[c], points[j]) >= 0.0
                    && cross(points[c], points[a], points[j]) >= 0.0
            });
            if !contains {
                ear = Some((k, a, b, c));
                break;
            }
        }
        let Some((k, a, b, c)) = ear else {
            break;
        };
        mesh.indices.extend([a as u32, b as u32, c as u32]);
        remaining.remove(k);
    }
    for i in 0..n {
        let prev = (points[i] - points[(i + n - 1) % n]).normalized();
        let next = (points[(i + 1) % n] - points[i]).normalized();
        let n1 = Vec2::new(prev.y, -prev.x);
        let n2 = Vec2::new(next.y, -next.x);
        let bisector = (n1 + n2).normalized();
        let offset = bisector * (feather / bisector.dot(n1).max(0.25));
        mesh.vertices
            .push(vertex(points[i] + offset, Color32::TRANSPARENT));
    }
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.indices.extend([
            i as u32,
            (n + i) as u32,
            j as u32,
            j as u32,
            (n + i) as u32,
            (n + j) as u32,
        ]);
    }
    mesh
}

pub fn hit(a: &Annotation, p: [f32; 2], tolerance: f32) -> bool {
    if a.a == a.b {
        return false;
    }
    let points = outline(a);
    let p = Pos2::new(p[0], p[1]);
    let mut inside = false;
    for i in 0..points.len() {
        let v = Pos2::new(points[i][0], points[i][1]);
        let w = points[(i + 1) % points.len()];
        let w = Pos2::new(w[0], w[1]);
        let d = w - v;
        let t = ((p - v).dot(d) / d.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
        if p.distance(v + d * t) <= tolerance {
            return true;
        }
        if (v.y > p.y) != (w.y > p.y) && p.x < (w.x - v.x) * (p.y - v.y) / (w.y - v.y) + v.x {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::Kind;
    #[test]
    fn gallery_path_triangulates_completely_and_preserves_endpoints() {
        for (a, b) in [
            ([10.0, 40.0], [240.0, 40.0]),
            ([240.0, 40.0], [10.0, 40.0]),
            ([50.0, 20.0], [50.0, 250.0]),
        ] {
            let annotation = Annotation::new(Kind::Arrow, a, b, 2.0);
            let points = outline(&annotation);
            assert_eq!(points[0], a);
            assert!(
                points
                    .iter()
                    .any(|p| (p[0] - b[0]).hypot(p[1] - b[1]) < 0.001)
            );
            let points: Vec<_> = points.into_iter().map(|p| Pos2::new(p[0], p[1])).collect();
            let mesh = mesh(&points, Color32::WHITE, 1.0);
            assert_eq!(
                mesh.indices.len(),
                3 * (points.len() - 2) + 6 * points.len()
            );
            let polygon_area: f32 = (0..points.len())
                .map(|i| cross(Pos2::ZERO, points[i], points[(i + 1) % points.len()]))
                .sum();
            let triangles_area: f32 = mesh.indices[..3 * (points.len() - 2)]
                .as_chunks::<3>()
                .0
                .iter()
                .map(|ids| {
                    cross(
                        points[ids[0] as usize],
                        points[ids[1] as usize],
                        points[ids[2] as usize],
                    )
                })
                .sum();
            assert!((polygon_area - triangles_area).abs() < 0.1);
        }
    }
}
