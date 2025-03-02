#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use app::App;
use eframe::egui;
use egui::{IconData, ViewportBuilder};

fn main() -> Result<(), eframe::Error> {
    #[cfg(debug_assertions)]
    env_logger::init();

    let icon = load_icon().expect("Failed to load icon");

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "Установщик аддонов Ночной стражи",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

fn load_icon() -> Option<IconData> {
    let (icon_rgba, icon_width, icon_height) = {
        let icon = include_bytes!("../resources/emblem.ico");
        let image = image::load_from_memory(icon).ok()?.into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };

    Some(IconData {
        rgba: icon_rgba,
        width: icon_width,
        height: icon_height,
    })
}
