use crate::modules::addon_manager;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Clone)]
pub struct Addon {
    pub name: String,
    pub link: String,
    pub target_path: String,
    pub installed: bool,
}

pub struct AddonState {
    pub progress: f32,
}

pub struct App {
    addons: Vec<Addon>,
    state: Arc<Mutex<AddonState>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Загрузка реальных данных из хранилища
        let addons = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            addons,
            state: Arc::new(Mutex::new(AddonState { progress: 0.0 })),
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.addons);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Интерфейс для работы с реальными аддонами
            for addon in &mut self.addons {
                ui.horizontal(|ui| {
                    ui.label(&addon.name);

                    if addon.installed {
                        if ui.button("Uninstall").clicked() {
                            let result = addon_manager::uninstall_addon(addon);
                            addon.installed = !result.unwrap_or(false);
                        }
                    } else {
                        if ui.button("Install").clicked() {
                            let client = reqwest::blocking::Client::new();
                            let result =
                                addon_manager::install_addon(&client, addon, self.state.clone());
                            addon.installed = result.unwrap_or(false);
                        }
                    }

                    ui.add(egui::ProgressBar::new(self.state.lock().unwrap().progress));
                });
            }
        });
    }
}
