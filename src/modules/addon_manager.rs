use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use log::{debug, error, info, trace, warn};
use reqwest::blocking::Client;
use std::{
    fs,
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use zip::ZipArchive;

pub fn check_addon_installed(addon: &Addon) -> bool {
    trace!("Checking installation status for: {}", addon.name);
    let main_path = Path::new(&addon.target_path).join(&addon.name);

    if main_path.exists() {
        debug!("Main path exists: {:?}", main_path);
        return true;
    }

    let install_base = Path::new(&addon.target_path);
    match fs::read_dir(install_base) {
        Ok(entries) => {
            let found = entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(&addon.name) || name.contains(&addon.name)
            });
            if found {
                debug!("Found partial installation for: {}", addon.name);
            }
            found
        }
        Err(e) => {
            warn!("Error reading directory: {:?} - {}", install_base, e);
            false
        }
    }
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Starting installation of: {}", addon.name);

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
    info!("Processing ZIP archive for: {}", addon.name);
    let temp_dir = tempdir().context("Failed to create temp directory")?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));

    info!("Downloading to: {:?}", download_path);
    download_file(client, &addon.link, &download_path, state.clone())
        .context("Failed to download file")?;

    let extract_dir = temp_dir.path().join("extracted");
    info!("Extracting to: {:?}", extract_dir);
    fs::create_dir_all(&extract_dir)?;
    extract_zip(&download_path, &extract_dir).context("Failed to extract ZIP archive")?;

    let install_base = Path::new(&addon.target_path);
    let dir_entries: Vec<fs::DirEntry> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
        .collect();

    match dir_entries.len() {
        0 => {
            info!("Copying root contents to: {:?}", install_base);
            copy_all_contents(&extract_dir, install_base).context("Root contents copy failed")?;
        }
        1 => {
            let source_dir = dir_entries[0].path();
            let install_path = install_base.join(&addon.name);
            info!("Copying single directory to: {:?}", install_path);
            copy_all_contents(&source_dir, &install_path)
                .context("Single directory copy failed")?;
        }
        _ => {
            info!("Copying multiple directories to: {:?}", install_base);
            for dir_entry in dir_entries {
                let source_dir = dir_entry.path();
                let dir_name = dir_entry.file_name();
                let install_path = install_base.join(dir_name);
                info!("Copying {:?} to {:?}", source_dir, install_path);
                copy_all_contents(&source_dir, &install_path)
                    .context("Multi-directory copy failed")?;
            }
        }
    }

    let result = check_addon_installed(addon);
    info!("Installation result for {}: {}", addon.name, result);
    Ok(result)
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!("Copying contents from {:?} to {:?}", source, dest);

    if dest.exists() {
        info!("Removing existing directory: {:?}", dest);
        fs::remove_dir_all(dest).context(format!("Failed to remove {:?}", dest))?;
    }

    fs::create_dir_all(dest).context(format!("Failed to create directory {:?}", dest))?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        if entry_path.is_dir() {
            debug!("Copying directory: {:?} -> {:?}", entry_path, target);
            copy_all_contents(&entry_path, &target)?;
        } else {
            debug!("Copying file: {:?} -> {:?}", entry_path, target);
            fs::copy(&entry_path, &target)
                .context(format!("Failed to copy {:?} to {:?}", entry_path, target))?;
        }
    }

    info!("Copy completed successfully");
    Ok(())
}

fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("Starting download: {} -> {:?}", url, path);
    let mut response = client.get(url).send()?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(path)?;

    let mut downloaded = 0;
    let mut buf = [0u8; 8192];

    while let Ok(bytes_read) = response.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buf[..bytes_read])?;
        downloaded += bytes_read as u64;
        let progress = downloaded as f32 / total_size as f32;
        state.lock().unwrap().progress = progress;
        debug!("Download progress: {:.2}%", progress * 100.0);
    }

    info!("Download completed: {} bytes", downloaded);
    Ok(())
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    info!("Extracting ZIP: {:?} -> {:?}", zip_path, target_dir);
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(target_dir)?;
    info!("Extraction completed successfully");
    Ok(())
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Handling single file installation for: {}", addon.name);
    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(&addon.name);

    download_file(client, &addon.link, &download_path, state).context("File download failed")?;

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    info!("Installing file to: {:?}", install_path);
    fs::create_dir_all(install_path.parent().unwrap())?;
    fs::copy(&download_path, &install_path)?;

    let result = install_path.exists();
    info!("File installation result: {}", result);
    Ok(result)
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("Starting uninstall for: {}", addon.name);
    let main_path = Path::new(&addon.target_path).join(&addon.name);
    let mut success = true;

    if main_path.exists() {
        info!("Removing main directory: {:?}", main_path);
        if let Err(e) = fs::remove_dir_all(&main_path) {
            error!("Error deleting main folder: {}", e);
            success = false;
        }
    }

    let install_base = Path::new(&addon.target_path);
    info!("Checking for additional components in: {:?}", install_base);
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                info!("Removing component: {:?}", entry.path());
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    error!("Error deleting component {}: {}", name, e);
                    success = false;
                }
            }
        }
    }

    let result = success && !check_addon_installed(addon);
    info!("Uninstall result for {}: {}", addon.name, result);
    Ok(result)
}
