use crate::app::Addon;
use anyhow::Result;
use indexmap::IndexMap;
use log::info;
use reqwest::blocking::Client;
use serde::{de, Deserialize};
use std::path::PathBuf;

#[derive(Deserialize)]
struct AddonConfig {
    link: String,
    description: String,
    #[serde(deserialize_with = "normalize_path")]
    target_path: String,
}

fn normalize_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: de::Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    Ok(path.replace("/", std::path::MAIN_SEPARATOR.to_string().as_str()))
}

pub fn load_addons_config_blocking(client: &Client) -> Result<IndexMap<String, Addon>> {
    let response = client
        .get("https://raw.githubusercontent.com/Vladgobelen/NSQCu/refs/heads/main/addons.json")
        .header("User-Agent", "NightWatchUpdater/1.0")
        .send()?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("HTTP Error: {}", response.status()));
    }

    let text = response.text()?;

    #[derive(Deserialize)]
    struct Config {
        addons: IndexMap<String, AddonConfig>,
    }

    let config: Config = serde_json::from_str(&text)?;

    info!("Loaded {} addons", config.addons.len());

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
    info!("Directory structure check skipped (dynamic handling)");
    Ok(())
}

pub fn base_dir() -> PathBuf {
    std::env::current_dir().expect("Failed to get current directory")
}
