//! Video-space data, independent of the GUI and decoder. Editing arrives in step 3.

#[derive(Debug, Clone)]
pub struct Annotation {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub shape: Shape,
}

#[derive(Debug, Clone)]
pub enum Shape {
    Text { position: [f32; 2], text: String },
    Rectangle { origin: [f32; 2], size: [f32; 2] },
    Ellipse { center: [f32; 2], radii: [f32; 2] },
    Arrow { from: [f32; 2], to: [f32; 2] },
}
