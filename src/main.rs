use eframe::egui;
use reqwest::blocking::get;
use std::fs::{self, create_dir_all, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

// Структура аддона
#[derive(Clone)]
struct Addon {
    name: String,
    installed: bool,
    url: String,
    install_path: PathBuf,
    temp_dir: PathBuf,
}

impl Addon {
    fn new(name: &str, url: &str, install_path: &str) -> Self {
        let temp_dir = PathBuf::from("temp").join(name);
        Self {
            name: name.to_string(),
            installed: false,
            url: url.to_string(),
            install_path: PathBuf::from(install_path),
            temp_dir,
        }
    }

    fn install(&mut self) -> Result<(), String> {
        log::info!("[{}] Starting installation", self.name);

        // 1. Скачивание
        log::info!("[{}] Downloading from {}", self.name, self.url);
        let response = get(&self.url).map_err(|e| format!("Download failed: {}", e))?;

        let content = response
            .bytes()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // 2. Создание временной директории
        if !self.temp_dir.exists() {
            log::info!(
                "[{}] Creating temp directory: {:?}",
                self.name,
                self.temp_dir
            );
            create_dir_all(&self.temp_dir)
                .map_err(|e| format!("Failed to create temp dir: {}", e))?;
        }

        // 3. Сохранение ZIP-архива
        let zip_path = self.temp_dir.join("addon.zip");
        let mut file =
            File::create(&zip_path).map_err(|e| format!("Failed to create zip file: {}", e))?;

        file.write_all(&content)
            .map_err(|e| format!("Failed to write zip file: {}", e))?;

        // 4. Распаковка
        log::info!("[{}] Extracting files", self.name);
        let zip_file = File::open(&zip_path).map_err(|e| format!("Failed to open zip: {}", e))?;

        let mut archive =
            ZipArchive::new(zip_file).map_err(|e| format!("Invalid zip archive: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read file {}: {}", i, e))?;

            let out_path = self.temp_dir.join(file.name());

            if file.name().ends_with('/') {
                create_dir_all(&out_path).map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                let mut out_file =
                    File::create(&out_path).map_err(|e| format!("Failed to create file: {}", e))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|e| format!("Failed to write file: {}", e))?;
            }
        }

        // 5. Перемещение файлов
        log::info!("[{}] Moving to {}", self.name, self.install_path.display());
        if !self.install_path.exists() {
            create_dir_all(&self.install_path)
                .map_err(|e| format!("Failed to create install dir: {}", e))?;
        }

        for entry in
            fs::read_dir(&self.temp_dir).map_err(|e| format!("Failed to read temp dir: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Invalid dir entry: {}", e))?;
            if entry.file_name() != "addon.zip" {
                let dest = self.install_path.join(entry.file_name());
                fs::rename(entry.path(), dest)
                    .map_err(|e| format!("Failed to move file: {}", e))?;
            }
        }

        // 6. Очистка
        log::info!("[{}] Cleaning temp files", self.name);
        fs::remove_dir_all(&self.temp_dir).map_err(|e| format!("Failed to clean temp: {}", e))?;

        self.installed = true;
        Ok(())
    }

    fn uninstall(&mut self) -> Result<(), String> {
        log::info!("[{}] Uninstalling", self.name);
        if self.install_path.exists() {
            fs::remove_dir_all(&self.install_path)
                .map_err(|e| format!("Uninstall failed: {}", e))?;
        }
        self.installed = false;
        Ok(())
    }
}

struct MyApp {
    addons: Vec<Addon>,
}

impl MyApp {
    fn new() -> Self {
        let addons = vec![
            Addon::new(
                "DBM",
                "https://github.com/.../DBM.zip", // Реальный URL
                "Interface/AddOns/DBM",
            ),
            Addon::new(
                "Skada",
                "https://github.com/.../Skada.zip",
                "Interface/AddOns/Skada",
            ),
            // ... другие аддоны
        ];

        // Проверка существующих установок
        let mut checked_addons = Vec::new();
        for mut addon in addons {
            addon.installed = addon.install_path.exists();
            checked_addons.push(addon);
        }

        Self {
            addons: checked_addons,
        }
    }

    fn apply_changes(&mut self) {
        log::info!("Applying changes...");

        for addon in &mut self.addons {
            if addon.installed && !addon.install_path.exists() {
                if let Err(e) = addon.install() {
                    log::error!("Installation error: {}", e);
                }
            } else if !addon.installed && addon.install_path.exists() {
                if let Err(e) = addon.uninstall() {
                    log::error!("Uninstall error: {}", e);
                }
            }
        }

        log::info!("Operation completed!");
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Nightwatch Updater");

            // Список аддонов с галочками
            for addon in &mut self.addons {
                ui.horizontal(|ui| {
                    let changed = ui.checkbox(&mut addon.installed, &addon.name).changed();
                    if changed {
                        log::info!("Checkbox '{}' changed to {}", addon.name, addon.installed);
                    }
                });
            }

            // Кнопка применения
            if ui.button("Apply Changes").clicked() {
                log::info!("Apply button clicked");
                self.apply_changes();
            }
        });
    }
}

fn main() {
    // Настройка логов
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}",
                chrono::Local::now().format("%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();

    // Запуск приложения
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Nightwatch Updater",
        options,
        Box::new(|_cc| Box::new(MyApp::new())),
    );
}
