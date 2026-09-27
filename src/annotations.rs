//! Annotation geometry in oriented video pixels, independent of UI size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Rectangle,
    Ellipse,
    Arrow,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Arrow => "Arrow",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub kind: Kind,
    /// Opposite box corners, or directed arrow endpoints.
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub text: String,
    pub color: [u8; 4],
    pub thickness: f32,
    pub font_size: f32,
}

impl Annotation {
    pub fn new(kind: Kind, a: [f32; 2], b: [f32; 2], duration: f64) -> Self {
        Self {
            kind,
            a,
            b,
            start_seconds: 0.0,
            end_seconds: duration,
            text: "Your text".into(),
            color: [255, 210, 60, 255],
            thickness: 4.0,
            font_size: 36.0,
        }
    }
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        (
            [self.a[0].min(self.b[0]), self.a[1].min(self.b[1])],
            [self.a[0].max(self.b[0]), self.a[1].max(self.b[1])],
        )
    }
    pub fn translate(&mut self, delta: [f32; 2], extent: [f32; 2]) {
        let (min, max) = self.bounds();
        for axis in 0..2 {
            let shift = delta[axis].clamp(-min[axis], (extent[axis] - max[axis]).max(-min[axis]));
            self.a[axis] += shift;
            self.b[axis] += shift;
        }
    }
    pub fn hit(&self, point: [f32; 2], tolerance: f32) -> bool {
        if self.kind == Kind::Arrow {
            return segment_distance(point, self.a, self.b) <= tolerance + self.thickness * 0.5;
        }
        let (min, max) = self.bounds();
        if self.kind == Kind::Ellipse {
            let rx = ((max[0] - min[0]) * 0.5).max(0.5) + tolerance;
            let ry = ((max[1] - min[1]) * 0.5).max(0.5) + tolerance;
            let x = (point[0] - (min[0] + max[0]) * 0.5) / rx;
            let y = (point[1] - (min[1] + max[1]) * 0.5) / ry;
            x * x + y * y <= 1.0
        } else {
            point[0] >= min[0] - tolerance
                && point[0] <= max[0] + tolerance
                && point[1] >= min[1] - tolerance
                && point[1] <= max[1] + tolerance
        }
    }
}

fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 0.0 {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    ((p[0] - a[0] - t * d[0]).powi(2) + (p[1] - a[1] - t * d[1]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_preserves_size_and_arrow_direction_at_edges() {
        let mut a = Annotation::new(Kind::Arrow, [90.0, 80.0], [30.0, 20.0], 5.0);
        a.translate([500.0, -500.0], [100.0, 100.0]);
        assert_eq!(a.a, [100.0, 60.0]);
        assert_eq!(a.b, [40.0, 0.0]);
    }
    #[test]
    fn hit_testing_uses_shape_geometry() {
        let mut a = Annotation::new(Kind::Ellipse, [0.0, 0.0], [100.0, 100.0], 5.0);
        assert!(a.hit([50.0, 50.0], 0.0));
        assert!(!a.hit([1.0, 1.0], 0.0));
        a.kind = Kind::Arrow;
        assert!(a.hit([50.0, 51.0], 2.0));
        assert!(!a.hit([50.0, 80.0], 2.0));
    }
}
