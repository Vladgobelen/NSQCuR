use crate::app::Addon;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::path::PathBuf;

const CONFIG_URL: &str =
    "https://raw.githubusercontent.com/Vladgobelen/NSQCu/refs/heads/main/addons.json";

#[derive(Deserialize)]
struct RawAddon {
    name: String,
    download_url: String,
    target_path: String,
}

pub fn load_online_config(client: &Client) -> Result<Vec<Addon>> {
    let response = client
        .get(CONFIG_URL)
        .send()
        .context("Failed to download config")?;

    let raw_addons: Vec<RawAddon> = response.json().context("Failed to parse config")?;

    Ok(raw_addons
        .into_iter()
        .map(|raw| Addon {
            name: raw.name,
            link: raw.download_url,
            target_path: PathBuf::from(raw.target_path)
                .to_string_lossy()
                .replace('/', std::path::MAIN_SEPARATOR.to_string().as_str()),
            installed: false,
        })
        .collect())
}
