#![cfg_attr((debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod modules;

use anyhow::Result;
use app::App;
use eframe::egui::{IconData, ViewportBuilder};
use std::sync::Arc;

fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let icon_result = load_icon();
    let mut viewport_builder = ViewportBuilder::default().with_inner_size([400.0, 600.0]);

    if let Ok(icon) = icon_result {
        viewport_builder = viewport_builder.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport: viewport_builder,
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

    let (width, height) = (image.width(), image.height());
    let rgba = image.to_rgba8().into_raw();

    Ok(IconData {
        rgba,
        width,
        height,
    })
}
