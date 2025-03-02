#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use anyhow::Result;
use app::App;
use eframe::egui;
use egui::{IconData, ViewportBuilder};
use log::error;

fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_icon(match load_icon() {
                Ok(icon) => Some(icon),
                Err(e) => {
                    error!("Failed to load icon: {}", e);
                    None
                }
            }),
        ..Default::default()
    };

    eframe::run_native(
        "Апдейтер Ночной Стражи",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

fn load_icon() -> Result<IconData> {
    let icon_bytes = include_bytes!("../resources/emblem.ico");
    let image = image::load_from_memory(icon_bytes)
        .map_err(|e| anyhow::anyhow!("Image load error: {}", e))?;
    let rgba8 = image.to_rgba8();
    Ok(IconData {
        rgba: rgba8.into_raw(),
        width: rgba8.width(),
        height: rgba8.height(),
    })
}
