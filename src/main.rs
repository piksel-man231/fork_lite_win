#![cfg_attr(windows, windows_subsystem = "windows")]
mod app;
mod autostart;
mod core;

fn main() -> eframe::Result {
    let icon_png = image::load_from_memory(include_bytes!("../Assets/AppIcon.png"))
        .expect("logo")
        .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let icon = eframe::egui::IconData {
        rgba: icon_png.into_raw(),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([620.0, 460.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Fork Lite Win",
        options,
        Box::new(|cc| Ok(Box::new(app::ForkApp::new(cc)))),
    )
}
