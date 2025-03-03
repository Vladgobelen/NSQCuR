#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use app::App;
use egui::IconData;

fn main() -> eframe::Result<()> {
    simplelog::CombinedLogger::init(vec![simplelog::WriteLogger::new(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        std::fs::File::create("updater.log").unwrap(),
    )])
    .unwrap();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_min_inner_size([400.0, 600.0])
            .with_icon(load_icon().expect("Failed to load icon")),
        ..Default::default()
    };

    eframe::run_native(
        "Night Watch Updater",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[allow(dead_code)]
fn load_icon() -> Option<IconData> {
    let icon_bytes = include_bytes!("../resources/emblem.ico");
    let image = image::load_from_memory(icon_bytes).ok()?.to_rgba8();

    // Получаем размеры до перемещения
    let (width, height) = (image.width(), image.height());

    // Теперь перемещаем image
    let rgba = image.into_raw();

    Some(IconData {
        rgba,
        width,
        height,
    })
}
