#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;

fn main() -> eframe::Result {
    let source = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "VideoAnnotations",
        options,
        Box::new(move |cc| Ok(Box::new(ui::VideoApp::new(cc, source)))),
    )
}
