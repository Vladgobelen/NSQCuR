#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use app::App;
use eframe::egui;
use egui::{IconData, ViewportBuilder};
use simplelog::{CombinedLogger, LevelFilter, WriteLogger}; // Убрали Config

fn main() -> Result<(), eframe::Error> {
    CombinedLogger::init(vec![WriteLogger::new(
        LevelFilter::Info,
        simplelog::Config::default(),
        std::fs::File::create("updater.log").unwrap(),
    )])
    .unwrap();

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
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

    // Сначала получаем размеры до перемещения
    let width = image.width();
    let height = image.height();
    let rgba = image.into_raw();

    Some(IconData {
        rgba,
        width,
        height,
    })
}
