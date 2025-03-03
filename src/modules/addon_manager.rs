use crate::app::{Addon, AddonState};
use crate::config;
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{error, info, warn};
use reqwest::blocking::Client;
use std::{
    fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::tempdir;
use zip_extensions::zip_extract;

pub fn check_addon_installed(addon: &Addon) -> bool {
    let target_dir = config::base_dir().join(&addon.target_path);
    let entries = match fs::read_dir(target_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };

    entries.filter_map(|e| e.ok()).any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        name.starts_with(&addon.name) || name.contains(&addon.name)
    })
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    if addon.link.ends_with(".zip") {
        handle_zip_install(client, addon, state)
    } else {
        handle_file_install(client, addon, state)
    }
}

fn handle_zip_install(
    client: &Client,
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

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!("📁 Copying: [{}] -> [{}]", source.display(), dest.display());

    if dest.exists() {
        let mut attempts = 0;
        let max_attempts = 3;
        loop {
            match fs::remove_dir_all(dest) {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    if attempts >= max_attempts {
                        return Err(e).context("🚮 Failed to clean target directory");
                    }
                    warn!(
                        "Retrying delete... (attempt {}/{})",
                        attempts + 1,
                        max_attempts
                    );
                    std::thread::sleep(Duration::from_secs(1));
                    attempts += 1;
                }
                Err(e) => return Err(e).context("🚮 Failed to clean target directory"),
            }
        }
    }

    fs::create_dir_all(dest)?;
    let options = DirCopyOptions::new().overwrite(true).content_only(true);

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

fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("⏬ Downloading: {}", url);

    let mut attempts = 0;
    let max_attempts = 3;
    let mut response;
    let total_size;

    loop {
        let result = client
            .get(url)
            .header("User-Agent", "NightWatchUpdater/1.0")
            .timeout(Duration::from_secs(300))
            .send();

        match result {
            Ok(res) if res.status().is_success() => {
                total_size = res.content_length().unwrap_or(1);
                response = res;
                break;
            }
            Ok(res) => {
                let status = res.status();
                let body = res.text().unwrap_or_default();
                error!("HTTP Error {}: {}", status, body);
                if attempts >= max_attempts {
                    return Err(anyhow::anyhow!("HTTP Error {}: {}", status, body));
                }
            }
            Err(e) => {
                error!("Network error (attempt {}): {}", attempts + 1, e);
                if attempts >= max_attempts {
                    return Err(e.into());
                }
            }
        }

        attempts += 1;
        std::thread::sleep(Duration::from_secs(5));
    }

    let mut file = File::create(path).context("🔴 Failed to create temp file")?;
    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = response.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])?;
        downloaded += bytes_read as u64;
        state.lock().unwrap().progress = downloaded as f32 / total_size as f32;
    }

    let downloaded_size = fs::metadata(path)?.len();
    if downloaded_size != total_size {
        return Err(anyhow::anyhow!(
            "📭 File corrupted: expected {} bytes, got {}",
            total_size,
            downloaded_size
        ));
    }

    file.sync_all()?;
    info!(
        "✅ Downloaded: {} ({:.2} MB)",
        url,
        downloaded_size as f64 / 1024.0 / 1024.0
    );
    Ok(())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("Starting uninstall: {}", addon.name);

    let base_dir = config::base_dir();
    let main_path = base_dir.join(&addon.target_path).join(&addon.name);
    let mut success = true;

    if main_path.exists() {
        info!("Deleting main directory: {}", main_path.display());
        if let Err(e) = fs::remove_dir_all(&main_path) {
            error!("Deletion error: {}", e);
            success = false;
        }
    }

    let install_base = base_dir.join(&addon.target_path);
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                info!("Deleting component: {}", name);
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    error!("Component deletion error: {} - {}", name, e);
                    success = false;
                }
            }
        }
    }

    if success {
        info!("Uninstall successful: {}", addon.name);
    } else {
        warn!("Partial uninstall: {}", addon.name);
    }
    Ok(success && !check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Installing file: {}", addon.name);

    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(&addon.name);
    download_file(client, &addon.link, &download_path, state)?;

    let base_dir = config::base_dir();
    let install_path = base_dir.join(&addon.target_path).join(&addon.name);
    fs::create_dir_all(install_path.parent().unwrap())?;
    fs::copy(&download_path, &install_path)?;

    info!("File installed: {}", install_path.display());
    Ok(install_path.exists())
}
