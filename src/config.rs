use crate::app::Addon;
use anyhow::{Context, Result};
use indexmap::IndexMap;
use log::{error, info};
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
    let path = String::deserialize(deserializer)?
        .trim()
        .replace("/", std::path::MAIN_SEPARATOR.to_string().as_str());

    if path.is_empty() {
        return Err(de::Error::custom("Target path cannot be empty"));
    }

    Ok(path)
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
        .call()
        .context("Network request failed")?;

    if response.status() != 200 {
        return Err(anyhow::anyhow!(
            "HTTP Error: {} - {}",
            response.status(),
            response.into_string()?
        ));
    }

    let text = response
        .into_string()
        .context("Invalid response encoding")?;
    info!("Raw JSON response: {}", text);

    #[derive(Deserialize)]
    struct Config {
        addons: IndexMap<String, AddonConfig>,
    }

    let config: Config = serde_json::from_str(&text).context("JSON parse error")?;

    for (name, cfg) in &config.addons {
        if cfg.link.is_empty() {
            return Err(anyhow::anyhow!("Addon {} has empty link", name));
        }
    }

    Ok(config
        .addons
        .into_iter()
        .map(|(name, cfg)| (name.clone(), Addon::from(cfg)))
        .collect())
}

pub fn check_game_directory() -> Result<()> {
    let base_dir = base_dir();
    if !base_dir.exists() {
        return Err(anyhow::anyhow!("Base directory not found: {:?}", base_dir));
    }
    Ok(())
}

pub fn base_dir() -> PathBuf {
    std::env::current_dir().expect("Failed to get current directory")
}
