#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(target_os = "android", no_main)]

mod app;
mod config;
mod modules;

use app::App;
use egui::IconData;

#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    use eframe::{egui, Renderer};
    use winit::event_loop::EventLoopBuilder;

    std::env::set_var("RUST_BACKTRACE", "full");
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        std::fs::File::create("/sdcard/updater.log").unwrap(),
    )
    .unwrap();

    let options = eframe::NativeOptions {
        renderer: Renderer::Wgpu,
        event_loop_builder: Some(Box::new(|builder| {
            builder.with_android_app(app);
        })),
        ..Default::default()
    };

    eframe::run_native(
        "Night Watch Updater",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Box::new(App::new(cc))
        }),
    )
    .unwrap();
}

#[cfg(not(target_os = "android"))]
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
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(App::new(cc)))
        }),
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
