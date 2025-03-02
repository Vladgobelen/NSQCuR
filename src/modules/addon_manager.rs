use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
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
    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;
    extract_zip(&download_path, &extract_dir)?;

    let install_base = Path::new(&addon.target_path);
    let dir_entries: Vec<fs::DirEntry> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
        .collect();

    match dir_entries.len() {
        0 => copy_all_contents(&extract_dir, install_base)?,

        1 => {
            let source_dir = dir_entries[0].path();
            let install_path = install_base.join(&addon.name);
            copy_all_contents(&source_dir, &install_path)?;
        }

        _ => {
            for dir_entry in dir_entries {
                let source_dir = dir_entry.path();
                let dir_name = dir_entry.file_name();
                let install_path = install_base.join(dir_name);
                copy_all_contents(&source_dir, &install_path)?;
            }
        }
    }

    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        remove_dir_all::remove_dir_all(dest).context("Failed to remove existing directory")?;
    }
    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        if entry_path.is_dir() {
            copy_all_contents(&entry_path, &target)?;
        } else {
            fs::copy(&entry_path, &target)
                .context(format!("Failed to copy {:?} to {:?}", entry_path, target))?;
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
    Ok(())
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(target_dir)?;
    Ok(())
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(&addon.name);
    download_file(client, &addon.link, &download_path, state)?;

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    fs::create_dir_all(install_path.parent().unwrap())?;
    fs::copy(&download_path, &install_path)?;

    Ok(install_path.exists())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let main_path = Path::new(&addon.target_path).join(&addon.name);
    let mut success = true;

    if main_path.exists() {
        if let Err(e) = fs::remove_dir_all(&main_path) {
            eprintln!("Error deleting main folder: {}", e);
            success = false;
        }
    }

    let install_base = Path::new(&addon.target_path);
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    eprintln!("Error deleting component {}: {}", name, e);
                    success = false;
                }
            }
        }
    }

    Ok(success && !check_addon_installed(addon))
}
