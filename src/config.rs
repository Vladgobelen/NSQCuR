use crate::app::Addon;
use anyhow::Result;
use indexmap::IndexMap;
use log::{error, info};
use reqwest::blocking::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct AddonConfig {
    link: String,
    description: String,
    target_path: String,
}

pub fn load_addons_config_blocking(client: &Client) -> Result<IndexMap<String, Addon>> {
    let response = client
        .get("https://raw.githubusercontent.com/Vladgobelen/NSQCu/refs/heads/main/addons.json")
        .header("User-Agent", "NightWatchUpdater/1.0")
        .send()?;

    if !response.status().is_success() {
        error!("Ошибка загрузки конфига: {}", response.status());
        return Err(anyhow::anyhow!(
            "HTTP Error: {} - {}",
            response.status(),
            response.text()?
        ));
    }

    let text = response.text()?;

    #[derive(Deserialize)]
    struct Config {
        addons: IndexMap<String, AddonConfig>,
    }

    let config: Config = serde_json::from_str(&text)?;

    info!("Успешно загружено {} аддонов", config.addons.len());
    Ok(config
        .addons
        .into_iter()
        .map(|(name, cfg)| {
            (
                name.clone(),
                Addon {
                    name,
                    link: cfg.link,
                    description: cfg.description,
                    target_path: cfg.target_path,
                },
            )
        })
        .collect())
}

pub fn check_game_directory() -> Result<()> {
    info!("Проверка структуры директорий");
    let required_dirs = ["Interface/AddOns", "Data", "Fonts"];
    for dir in required_dirs {
        let path = std::path::Path::new(dir);
        if !path.exists() {
            info!("Создание отсутствующей директории: {}", dir);
            std::fs::create_dir_all(path)?;
        }
    }
    info!("Проверка директорий завершена");
    Ok(())
}
