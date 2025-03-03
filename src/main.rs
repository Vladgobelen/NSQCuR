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
    let image = match image::load_from_memory(icon_bytes) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            log::error!("Failed to load icon: {}", e);
            return None;
        }
    };

    log::info!("Icon dimensions: {}x{}", image.width(), image.height());
    Some(IconData {
        rgba: image.into_raw(),
        width: image.width(),
        height: image.height(),
    })
}
