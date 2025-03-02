use crate::{config, modules::addon_manager};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Addon {
    pub name: String,
    pub link: String,
    pub target_path: String,
    pub installed: bool,
}

#[derive(Default)]
pub struct AddonState {
    pub progress: f32,
}

pub struct App {
    addons: Vec<Addon>,
    state: Arc<Mutex<AddonState>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let client = reqwest::blocking::Client::new();
        let mut addons = match config::load_online_config(&client) {
            Ok(online) => online,
            Err(e) => {
                log::error!("Failed to load config: {}", e);
                Vec::new()
            }
        };

        if let Some(storage) = cc.storage {
            if let Some(local) = eframe::get_value(storage, eframe::APP_KEY) {
                merge_local_state(&mut addons, local);
            }
        }

        Self {
            addons,
            state: Arc::new(Mutex::new(AddonState { progress: 0.0 })),
        }
    }
}

fn merge_local_state(online: &mut [Addon], local: Vec<Addon>) {
    for local_addon in local {
        if let Some(online_addon) = online.iter_mut().find(|a| a.name == local_addon.name) {
            online_addon.installed = local_addon.installed;
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.addons);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            for addon in &mut self.addons {
                ui.horizontal(|ui| {
                    ui.label(&addon.name);

                    if addon.installed {
                        if ui.button("Uninstall").clicked() {
                            let result = addon_manager::uninstall_addon(addon);
                            addon.installed = !result.unwrap_or(false);
                        }
                    } else if ui.button("Install").clicked() {
                        let result = addon_manager::install_addon(
                            &reqwest::blocking::Client::new(),
                            addon,
                            self.state.clone(),
                        );
                        addon.installed = result.unwrap_or(false);
                    }

                    ui.add(egui::ProgressBar::new(self.state.lock().unwrap().progress));
                });
            }
        });
    }
}
