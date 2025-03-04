// Требуется для Android-библиотеки
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    use eframe::Renderer;
    use winit::event_loop::EventLoopBuilder;

    std::env::set_var("RUST_BACKTRACE", "full");

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
        Box::new(|cc| Box::new(crate::app::App::new(cc))),
    )
    .unwrap();
}
