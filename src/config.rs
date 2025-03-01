use crate::app::Addon;
use anyhow::Result;
use indexmap::IndexMap;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct AddonConfig {
    link: String,
    description: String,
    #[serde(rename = "type")]
    addon_type: u8,
    source_path: String,
    target_path: String,
    delete_path: String,
}

pub fn load_addons_config_blocking(client: &Client) -> Result<IndexMap<String, Addon>> {
    let response = client
        .get("https://raw.githubusercontent.com/Vladgobelen/NSQCu/refs/heads/main/addons.json")
        .send()?;
    let text = response.text()?;

    #[derive(Deserialize)]
    struct Config {
        addons: IndexMap<String, AddonConfig>,
    }

    let config: Config = serde_json::from_str(&text)?;

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
                    addon_type: cfg.addon_type,
                    source_path: cfg.source_path,
                    target_path: cfg.target_path,
                    delete_path: cfg.delete_path,
                },
            )
        })
        .collect())
}

pub fn check_game_directory() -> Result<()> {
    let required_dirs = ["Interface/AddOns", "Data", "Fonts"];
    for dir in required_dirs {
        let path = Path::new(dir);
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }
    }
    Ok(())
}
