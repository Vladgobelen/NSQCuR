#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use app::App;
use eframe::egui;
use egui::{IconData, ViewportBuilder};
use log::error; // Добавлено

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Добавлено

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_icon(load_icon().unwrap_or_else(|e| {
                error!("Failed to load icon: {}", e);
                None
            })),
        ..Default::default()
    };

    eframe::run_native(
        "Апдейтер Ночной Стражи",
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
