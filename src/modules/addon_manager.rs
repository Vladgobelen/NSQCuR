use crate::app::{Addon, AddonState};
use crate::config;
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions;
use log::{error, info, warn};
use std::{
    fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::tempdir;
use ureq::Agent;
use zip_extensions::zip_extract;

pub fn check_addon_installed(addon: &Addon) -> bool {
    let base_dir = config::base_dir();

    if addon.is_zip {
        let target_dir = base_dir.join(&addon.target_path).join(&addon.name);
        target_dir.exists()
    } else {
        let file_name = addon
            .link
            .split('/')
            .last()
            .unwrap_or(&addon.name)
            .split('?')
            .next()
            .unwrap_or(&addon.name);

        let file_path = base_dir.join(&addon.target_path).join(file_name);
        file_path.exists()
    }
}

pub fn install_addon(client: &Agent, addon: &Addon, state: Arc<Mutex<AddonState>>) -> Result<bool> {
    if addon.is_zip {
        handle_zip_install(client, addon, state)
    } else {
        handle_file_install(client, addon, state)
    }
}

fn handle_zip_install(
    client: &Agent,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("🚀 Starting ZIP install: {}", addon.name);

    let temp_dir = tempdir().context("🔴 Failed to create temp dir")?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));

    info!("📂 Temp dir: {}", temp_dir.path().display());
    info!("📥 ZIP path: {}", download_path.display());

    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;

    zip_extract(&download_path, &extract_dir)
        .map_err(|e| anyhow::anyhow!("🔧 Failed to extract ZIP: {}", e))?;

    let entries: Vec<PathBuf> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .collect();

    if entries.is_empty() {
        return Err(anyhow::anyhow!("📭 Empty ZIP archive"));
    }

    let (source_dir, should_create_subdir) = match entries.as_slice() {
        [single_entry] if single_entry.is_dir() => (single_entry.clone(), true),
        _ => (extract_dir.clone(), false),
    };

    let base_dir = config::base_dir();
    let target_dir = base_dir.join(&addon.target_path);
    let final_target = if should_create_subdir {
        target_dir.join(&addon.name)
    } else {
        target_dir
    };

    fs::create_dir_all(&final_target)?;
    copy_all_contents(&source_dir, &final_target)?;

    info!("✅ Successfully installed: {}", addon.name);
    Ok(check_addon_installed(addon))
}

fn handle_file_install(
    client: &Agent,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("📥 Installing file: {}", addon.name);

    let file_name = addon
        .link
        .split('/')
        .last()
        .unwrap_or(&addon.name)
        .split('?')
        .next()
        .unwrap_or(&addon.name)
        .trim()
        .to_string();

    info!("File name parsed: '{}'", file_name);

    let target_path = config::base_dir().join(&addon.target_path).join(&file_name);

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).context(format!(
            "🔴 Failed to create directory: {}",
            parent.display()
        ))?;
    }

    download_file(client, &addon.link, &target_path, state)?;

    info!("✅ File installed: {}", target_path.display());
    Ok(target_path.exists())
}

fn download_file(
    client: &Agent,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("⏬ Downloading: {}", url);

    if !url.starts_with("https://") {
        return Err(anyhow::anyhow!("Invalid URL protocol: {}", url));
    }

    let mut attempts = 0;
    let max_attempts = 3;
    let total_size;

    let response = loop {
        let result = client
            .get(url)
            .set("User-Agent", "NightWatchUpdater/1.0")
            .timeout(Duration::from_secs(30))
            .call();

        match result {
            Ok(res) => {
                total_size = res
                    .header("Content-Length")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                break res;
            }
            Err(e) => {
                error!("Network error (attempt {}): {}", attempts + 1, e);
                if attempts >= max_attempts {
                    return Err(e.into());
                }
                attempts += 1;
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    };

    let mut reader = response.into_reader();
    let mut file = File::create(path).context("🔴 Failed to create temp file")?;
    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])?;
        downloaded += bytes_read as u64;
        state.lock().unwrap().progress = downloaded as f32 / total_size as f32;
    }

    if total_size > 0 && downloaded != total_size {
        return Err(anyhow::anyhow!(
            "📭 File corrupted: expected {} bytes, got {}",
            total_size,
            downloaded
        ));
    }

    file.sync_all()?;
    info!(
        "✅ Downloaded: {} ({:.2} MB)",
        url,
        downloaded as f64 / 1024.0 / 1024.0
    );
    Ok(())
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!("📁 Copying: [{}] → [{}]", source.display(), dest.display());

    fs::create_dir_all(dest)?;
    let options = CopyOptions::new().overwrite(true).content_only(true);

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = dest.join(entry.file_name());

        if entry_path.is_dir() {
            fs_extra::dir::copy(&entry_path, &target_path, &options)?;
        } else {
            fs::copy(&entry_path, &target_path)?;
        }
    }

    Ok(())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("Starting uninstall: {}", addon.name);

    let base_dir = config::base_dir();
    let mut success = true;

    if addon.is_zip {
        let dir_path = base_dir.join(&addon.target_path).join(&addon.name);
        info!("🗑 Attempting to delete directory: {}", dir_path.display());
        if dir_path.exists() {
            if let Err(e) = fs::remove_dir_all(&dir_path) {
                error!("Directory deletion error: {}", e);
                success = false;
            }
        } else {
            warn!("Directory not found: {}", dir_path.display());
            success = false;
        }
    } else {
        let file_name = addon
            .link
            .split('/')
            .last()
            .unwrap_or(&addon.name)
            .split('?')
            .next()
            .unwrap_or(&addon.name);

        let file_path = base_dir.join(&addon.target_path).join(file_name);
        info!("🗑 Attempting to delete file: {}", file_path.display());

        match fs::remove_file(&file_path) {
            Ok(_) => info!("Deleted successfully"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!("File not found: {}", file_path.display())
            }
            Err(e) => {
                error!("Deletion error: {}", e);
                success = false;
            }
        }
    }

    Ok(success && !check_addon_installed(addon))
}
