use crate::app::Addon;
use anyhow::Result;
use indexmap::IndexMap;
use log::info;
use serde::{de, Deserialize};
use std::path::PathBuf;
use ureq::Agent;

#[derive(Deserialize)]
struct AddonConfig {
    name: String,
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

impl From<AddonConfig> for Addon {
    fn from(cfg: AddonConfig) -> Self {
        let is_zip = cfg.link.to_lowercase().ends_with(".zip");
        Addon {
            name: cfg.name,
            link: cfg.link,
            description: cfg.description,
            target_path: cfg.target_path,
            is_zip,
        }
    }
}

pub fn load_addons_config_blocking(client: &Agent) -> Result<IndexMap<String, Addon>> {
    let response = client
        .get("https://raw.githubusercontent.com/Vladgobelen/NSQCu/main/addons.json")
        .set("User-Agent", "NightWatchUpdater/1.0")
        .call()?;

    if response.status() != 200 {
        return Err(anyhow::anyhow!(
            "HTTP Error: {} - {}",
            response.status(),
            response.into_string()?
        ));
    }

    let text = response.into_string()?;

    #[derive(Deserialize)]
    struct Config {
        addons: IndexMap<String, AddonConfig>,
    }

    let config: Config = serde_json::from_str(&text)?;

    Ok(config
        .addons
        .into_iter()
        .map(|(name, cfg)| (name, Addon::from(cfg)))
        .collect())
}

pub fn check_game_directory() -> Result<()> {
    Ok(())
}

pub fn base_dir() -> PathBuf {
    let path = std::env::current_dir().expect("Failed to get current directory");
    info!("Base directory: {}", path.display());
    path
}
