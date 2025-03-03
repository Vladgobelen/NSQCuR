#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use app::App;
use eframe::egui;
use egui::{IconData, ViewportBuilder};
use std::path::Path;

fn main() -> Result<(), eframe::Error> {
    // Проверка запуска из корня игры
    if !Path::new("WoW.exe").exists() {
        eprintln!("Запустите программу из корневой директории игры!");
        std::process::exit(1);
    }

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_icon(load_icon().expect("Не удалось загрузить иконку")),
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
