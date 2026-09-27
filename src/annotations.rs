//! Annotation geometry in oriented video pixels, independent of UI size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub effects: Effects,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Effects {
    pub fade_in: f64,
    pub fade_out: f64,
    /// Linear displacement from the stored geometry over the active interval.
    pub movement: [f32; 2],
    pub glow: f32,
    /// Zero disables the traveling outline; otherwise seconds per revolution.
    pub outline_period: f64,
}

impl Annotation {
    pub fn animated(&self) -> bool {
        self.effects.fade_in > 0.0
            || self.effects.fade_out > 0.0
            || self.effects.movement != [0.0; 2]
            || self.effects.outline_period > 0.0
    }

    /// Pure evaluation: seeking and exporting never change the saved geometry.
    pub fn evaluated(&self, time: f64) -> Self {
        let mut a = self.clone();
        let lifetime = (self.end_seconds - self.start_seconds).max(f64::EPSILON);
        let elapsed = (time - self.start_seconds).clamp(0.0, lifetime);
        let progress = (elapsed / lifetime) as f32;
        for axis in 0..2 {
            a.a[axis] += self.effects.movement[axis] * progress;
            a.b[axis] += self.effects.movement[axis] * progress;
        }
        let fade_in = if self.effects.fade_in > 0.0 {
            (elapsed / self.effects.fade_in).min(1.0)
        } else {
            1.0
        };
        let fade_out = if self.effects.fade_out > 0.0 {
            ((lifetime - elapsed) / self.effects.fade_out).min(1.0)
        } else {
            1.0
        };
        a.color[3] = (self.color[3] as f64 * fade_in.min(fade_out)).round() as u8;
        a
    }
    /// Half-open intervals, except the final video endpoint remains visible.
    pub fn visible_at(&self, time: f64, duration: f64) -> bool {
        time.is_finite()
            && time >= self.start_seconds
            && (time < self.end_seconds || (time == duration && self.end_seconds == duration))
    }

    pub fn at_playhead(mut self, time: f64, duration: f64) -> Self {
        let gap = Self::minimum_duration(duration);
        self.start_seconds =
            if time.is_finite() { time } else { 0.0 }.clamp(0.0, (duration - gap).max(0.0));
        self.end_seconds = (self.start_seconds + 5.0).min(duration);
        self
    }

    pub fn minimum_duration(duration: f64) -> f64 {
        duration.clamp(0.0, 0.001)
    }

    pub fn set_start(&mut self, time: f64, duration: f64) {
        if time.is_finite() {
            self.start_seconds = time.clamp(
                0.0,
                (self.end_seconds - Self::minimum_duration(duration)).max(0.0),
            );
        }
    }

    pub fn set_end(&mut self, time: f64, duration: f64) {
        if time.is_finite() {
            self.end_seconds = time.clamp(
                (self.start_seconds + Self::minimum_duration(duration)).min(duration),
                duration,
            );
        }
    }

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
            effects: Effects::default(),
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
    fn effects_evaluate_deterministically_without_mutating_geometry() {
        let mut a = Annotation::new(Kind::Text, [10.0, 20.0], [50.0, 60.0], 4.0);
        a.start_seconds = 1.0;
        a.effects.fade_in = 1.0;
        a.effects.fade_out = 1.0;
        a.effects.movement = [60.0, -12.0];
        assert_eq!(a.evaluated(1.0).color[3], 0);
        assert_eq!(a.evaluated(1.5).color[3], 128);
        assert_eq!(a.evaluated(2.5).a, [40.0, 14.0]);
        assert_eq!(a.evaluated(2.5).color[3], 255);
        assert_eq!(a.evaluated(3.5).color[3], 128);
        assert_eq!(a.evaluated(4.0).color[3], 0);
        assert_eq!(a.a, [10.0, 20.0]);
        a.effects.fade_in = 6.0;
        a.effects.fade_out = 6.0;
        assert_eq!(a.evaluated(2.5).color[3], 64);
    }
    #[test]
    fn intervals_use_start_inclusive_end_exclusive_and_include_video_end() {
        let mut a = Annotation::new(Kind::Text, [0.0; 2], [10.0; 2], 10.0).at_playhead(2.0, 10.0);
        assert!(!a.visible_at(1.999, 10.0));
        assert!(a.visible_at(2.0, 10.0));
        assert!(a.visible_at(6.999, 10.0));
        assert!(!a.visible_at(7.0, 10.0));
        a.set_end(10.0, 10.0);
        assert!(a.visible_at(10.0, 10.0));
        assert!(!a.visible_at(f64::NAN, 10.0));
    }
    #[test]
    fn timing_edits_are_bounded_and_cannot_invert_an_interval() {
        let mut a = Annotation::new(Kind::Text, [0.0; 2], [10.0; 2], 10.0).at_playhead(9.0, 10.0);
        assert_eq!((a.start_seconds, a.end_seconds), (9.0, 10.0));
        a.set_start(50.0, 10.0);
        assert!(a.start_seconds < a.end_seconds);
        a.set_start(-1.0, 10.0);
        assert_eq!(a.start_seconds, 0.0);
        a.set_end(-1.0, 10.0);
        assert_eq!(a.end_seconds, 0.001);
        a.set_end(f64::NAN, 10.0);
        assert_eq!(a.end_seconds, 0.001);
        let eof = a.clone().at_playhead(10.0, 10.0);
        assert!(eof.start_seconds < eof.end_seconds);
        let short = a.at_playhead(0.0005, 0.0005);
        assert_eq!((short.start_seconds, short.end_seconds), (0.0, 0.0005));
    }
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
