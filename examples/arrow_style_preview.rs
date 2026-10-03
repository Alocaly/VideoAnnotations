use std::sync::atomic::AtomicBool;
use video_annotations::{
    annotations::{Annotation, Kind},
    render::rasterize_scene,
};
fn main() {
    let mut annotations = Vec::new();
    for (a, b, thickness) in [
        ([30.0, 100.0], [390.0, 100.0], 4.0),
        ([30.0, 280.0], [390.0, 280.0], 12.0),
        ([30.0, 550.0], [720.0, 490.0], 4.0),
        ([40.0, 680.0], [180.0, 680.0], 12.0),
        ([420.0, 680.0], [280.0, 680.0], 4.0),
    ] {
        let mut arrow = Annotation::new(Kind::Arrow, a, b, 2.0);
        arrow.thickness = thickness;
        annotations.push(arrow);
    }
    let overlay =
        rasterize_scene(&annotations, [800, 740], 1.0, 2.0, &AtomicBool::new(false)).unwrap();
    let mut background = image::RgbaImage::from_pixel(800, 740, image::Rgba([35, 40, 48, 255]));
    image::imageops::overlay(&mut background, &overlay, 0, 0);
    background.save("target/arrow-style-preview.png").unwrap();
}
