use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{error, info, warn};
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
    let main_path = Path::new(&addon.target_path).join(&addon.name);

    if main_path.exists() {
        return true;
    }

    let install_base = Path::new(&addon.target_path);
    match fs::read_dir(install_base) {
        Ok(entries) => entries.filter_map(|e| e.ok()).any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(&addon.name) || name.contains(&addon.name)
        }),
        Err(_) => false,
    }
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Installing addon: {}", addon.name);
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
    info!("Processing ZIP install for: {}", addon.name);
    let temp_dir = tempdir()?;
    info!("Created temp directory: {:?}", temp_dir.path());

    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())?;
    info!("Download completed to: {:?}", download_path);

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;
    info!("Created extraction directory: {:?}", extract_dir);

    extract_zip(&download_path, &extract_dir)?;
    info!("Extracted files to: {:?}", extract_dir);

    let install_base = Path::new(&addon.target_path);
    let dir_entries: Vec<fs::DirEntry> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
        .collect();

    match dir_entries.len() {
        0 => {
            info!("Copying root contents to {:?}", install_base);
            copy_all_contents(&extract_dir, install_base)?
        }
        1 => {
            let source_dir = dir_entries[0].path();
            let install_path = install_base.join(&addon.name);
            info!("Copying single directory to {:?}", install_path);
            copy_all_contents(&source_dir, &install_path)?
        }
        _ => {
            info!("Copying multiple directories to {:?}", install_base);
            for dir_entry in dir_entries {
                let source_dir = dir_entry.path();
                let dir_name = dir_entry.file_name();
                let install_path = install_base.join(dir_name);
                info!("Copying {:?} to {:?}", source_dir, install_path);
                copy_all_contents(&source_dir, &install_path)?;
            }
        }
    }

    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!("Copying contents from {:?} to {:?}", source, dest);
    if dest.exists() {
        fs::remove_dir_all(dest).context("Failed to remove existing directory")?;
    }
    fs::create_dir_all(dest)?;

    let options = DirCopyOptions::new().overwrite(true).content_only(true);

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        if entry_path.is_dir() {
            fs_extra::dir::copy(&entry_path, &target, &options)?;
        } else {
            fs::copy(&entry_path, &target)?;
        }
    }

    info!("Copy operation completed");
    Ok(())
}

fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("Starting download from: {} to {:?}", url, path);
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
        state.lock().unwrap().progress = downloaded as f32 / total_size as f32;
    }

    info!("Download finished successfully: {:?}", path);
    Ok(())
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    info!("Extracting ZIP: {:?} to {:?}", zip_path, target_dir);
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(target_dir)?;
    info!("ZIP extraction completed");
    Ok(())
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Processing file install for: {}", addon.name);
    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(&addon.name);
    download_file(client, &addon.link, &download_path, state)?;

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    info!("Copying file to {:?}", install_path);
    fs::create_dir_all(install_path.parent().unwrap())?;
    fs::copy(&download_path, &install_path)?;

    Ok(install_path.exists())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("Uninstalling addon: {}", addon.name);
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
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                info!("Removing related file/dir: {:?}", entry.path());
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    error!("Error deleting component {}: {}", name, e);
                    success = false;
                }
            }
        }
    }

    info!("Uninstall completed for: {}", addon.name);
    Ok(success && !check_addon_installed(addon))
}
