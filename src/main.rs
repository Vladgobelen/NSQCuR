#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use app::App;
use egui::IconData;
use log::error;

fn main() -> eframe::Result<()> {
    std::panic::set_hook(Box::new(|panic_info| {
        error!("CRITICAL PANIC: {:?}", panic_info);
    }));

    let log_file = std::fs::File::create("debug.log").unwrap();
    simplelog::CombinedLogger::init(vec![simplelog::WriteLogger::new(
        simplelog::LevelFilter::Trace,
        simplelog::Config::default(),
        log_file,
    )])
    .unwrap();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_icon(load_icon().expect("Failed to load icon")),
        ..Default::default()
    };

    eframe::run_native(
        "Night Watch Updater",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

fn load_icon() -> Option<IconData> {
    let icon_bytes = include_bytes!("../resources/emblem.ico");
    let image = image::load_from_memory(icon_bytes).ok()?.to_rgba8();
    let (width, height) = (image.width(), image.height());
    let rgba = image.into_raw();

    Some(IconData {
        rgba,
        width,
        height,
    })
}
