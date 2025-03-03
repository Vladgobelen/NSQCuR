use crate::{config, modules::addon_manager};
use egui::{CentralPanel, ProgressBar, ScrollArea};
use reqwest::blocking::Client;
use serde::Deserialize;
use simplelog::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Deserialize)]
pub struct Addon {
    pub name: String,
    pub link: String,
    pub description: String,
    pub target_path: String,
}

#[derive(Default)]
pub struct AddonState {
    pub target_state: Option<bool>,
    pub installing: bool,
    pub progress: f32,
}

pub struct App {
    pub addons: Vec<(Addon, Arc<Mutex<AddonState>>)>,
    pub client: Client,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        config::check_game_directory().unwrap_or_else(|e| panic!("{}", e));

        let client = Client::builder()
            .user_agent("NightWatchUpdater/1.0")
            .build()
            .expect("Failed to create HTTP client");

        let addons =
            config::load_addons_config_blocking(&client).expect("Failed to load addons config");

        let addons_with_state = addons
            .into_iter()
            .map(|(_, addon)| {
                let installed = addon_manager::check_addon_installed(&addon);
                (
                    addon,
                    Arc::new(Mutex::new(AddonState {
                        target_state: Some(installed),
                        installing: false,
                        progress: 0.0,
                    })),
                )
            })
            .collect();

        Self {
            addons: addons_with_state,
            client,
        }
    }

    fn toggle_addon(&mut self, index: usize) {
        let (addon, state) = self.addons[index].clone();
        let mut state_lock = state.lock().unwrap();

        if state_lock.installing {
            return;
        }

        let current_state = addon_manager::check_addon_installed(&addon);
        let desired_state = !current_state;

        info!(
            "Изменение состояния: {} -> {}",
            addon.name,
            if desired_state {
                "Установка"
            } else {
                "Удаление"
            }
        );

        state_lock.target_state = Some(desired_state);
        state_lock.installing = true;
        state_lock.progress = 0.0;
        drop(state_lock);

        let client = self.client.clone();
        std::thread::spawn(move || {
            let result = if desired_state {
                addon_manager::install_addon(&client, &addon, state.clone())
            } else {
                addon_manager::uninstall_addon(&addon)
            };

            let mut state = state.lock().unwrap();
            state.installing = false;

            let actual_state = addon_manager::check_addon_installed(&addon);
            state.target_state = Some(actual_state);

            if let Err(e) = result {
                error!("Ошибка операции: {} - {:?}", addon.name, e);
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.heading("Addon Manager");
            ui.separator();

            let mut indices_to_toggle = Vec::new();

            ScrollArea::vertical().show(ui, |ui| {
                for (i, (addon, state)) in self.addons.iter().enumerate() {
                    let mut state_lock = state.lock().unwrap();

                    let actual_state = addon_manager::check_addon_installed(addon);
                    if state_lock.target_state != Some(actual_state) {
                        state_lock.target_state = Some(actual_state);
                    }

                    let mut current_state = state_lock.target_state.unwrap_or(false);

                    ui.horizontal(|ui| {
                        let enabled = !state_lock.installing;
                        let response =
                            ui.add_enabled_ui(enabled, |ui| ui.checkbox(&mut current_state, ""));

                        if response.inner.changed() && enabled {
                            indices_to_toggle.push(i);
                        }

                        ui.vertical(|ui| {
                            ui.heading(&addon.name);
                            ui.label(&addon.description);
                            if state_lock.installing {
                                ui.add(ProgressBar::new(state_lock.progress).show_percentage());
                            }
                        });
                    });
                    ui.separator();
                }
            });

            for index in indices_to_toggle {
                self.toggle_addon(index);
            }
        });

        ctx.request_repaint();
    }
}
