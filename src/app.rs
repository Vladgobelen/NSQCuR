use eframe::egui::{self, CentralPanel, ProgressBar, ScrollArea};
use log::{error, info};
use serde::Deserialize;
use std::process::Command;
use std::sync::{Arc, Mutex};
use ureq::Agent;

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
    pub needs_update: bool,
}

pub struct App {
    pub addons: Vec<(Addon, Arc<Mutex<AddonState>>)>,
    pub client: Agent,
    pub game_available: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let game_available = crate::config::check_game_directory().is_ok();

        let client = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .build();

        let addons = crate::config::load_addons_config_blocking(&client)
            .expect("Failed to load addons config");

        let addons_with_state = addons
            .into_iter()
            .map(|(_, addon)| {
                let installed = crate::modules::addon_manager::check_addon_installed(&addon);
                let mut needs_update = false;

                if addon.name == "NSQC" && installed {
                    needs_update =
                        crate::modules::addon_manager::check_nsqc_update(&client).unwrap_or(false);
                }

                (
                    addon,
                    Arc::new(Mutex::new(AddonState {
                        target_state: Some(installed),
                        installing: false,
                        progress: 0.0,
                        needs_update,
                    })),
                )
            })
            .collect();

        Self {
            addons: addons_with_state,
            client,
            game_available,
        }
    }

    fn toggle_addon(&mut self, index: usize) {
        let (addon, state) = self.addons[index].clone();
        let client = self.client.clone();

        std::thread::spawn(move || {
            let mut state_lock = state.lock().unwrap();
            let current_state = crate::modules::addon_manager::check_addon_installed(&addon);
            let desired_state = !current_state;

            state_lock.installing = true;
            state_lock.target_state = Some(desired_state);
            state_lock.progress = 0.0;
            drop(state_lock);

            let result = if desired_state {
                crate::modules::addon_manager::install_addon(&client, &addon, state.clone())
            } else {
                crate::modules::addon_manager::uninstall_addon(&addon)
            };

            let mut state = state.lock().unwrap();
            state.installing = false;
            state.target_state = Some(crate::modules::addon_manager::check_addon_installed(&addon));

            if let Err(e) = result {
                error!("Operation failed: {} - {:?}", addon.name, e);
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            egui::TopBottomPanel::top("top_panel").show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if self.game_available {
                        if ui.button("🚀 Запустить игру").clicked() {
                            match launch_game() {
                                Ok(_) => info!("Game launched successfully"),
                                Err(e) => error!("Failed to launch game: {}", e),
                            }
                        }
                    } else {
                        ui.colored_label(
                            egui::Color32::RED,
                            "❌ Игра не найдена в текущей директории",
                        );
                    }
                });
            });

            ui.heading("Addon Manager");
            ui.separator();

            let mut indices_to_toggle = Vec::new();

            ScrollArea::vertical().show(ui, |ui| {
                for (i, (addon, state)) in self.addons.iter().enumerate() {
                    let state_lock = state.lock().unwrap();

                    ui.horizontal(|ui| {
                        if addon.name == "NSQC" && state_lock.needs_update {
                            ui.colored_label(egui::Color32::YELLOW, "⏫");
                        }

                        let enabled = !state_lock.installing;
                        let mut current_state = state_lock.target_state.unwrap_or(false);

                        let response =
                            ui.add_enabled_ui(enabled, |ui| ui.checkbox(&mut current_state, ""));

                        if response.inner.changed() {
                            indices_to_toggle.push(i);
                        }

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.heading(&addon.name);
                                if addon.name == "NSQC" && state_lock.needs_update {
                                    ui.colored_label(egui::Color32::GREEN, "(Доступно обновление)");
                                }
                            });
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
    }
}

fn launch_game() -> Result<(), std::io::Error> {
    let exe_path = crate::config::get_wow_path();

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        Command::new(exe_path)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new(exe_path).spawn()?;
    }

    Ok(())
}
