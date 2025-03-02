use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::{copy, CopyOptions};
use reqwest::blocking::Client;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile;
use zip::ZipArchive;

pub fn check_addon_installed(addon: &Addon) -> bool {
    let install_path = Path::new(&addon.target_path).join(&addon.name);

    if !install_path.exists() {
        return false;
    }

    if install_path.is_dir() {
        fs::read_dir(&install_path)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    } else {
        true
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
    let install_dir = Path::new(&addon.target_path).join(&addon.name);
    let temp_dir = tempfile::tempdir().context("Failed to create temp dir")?;
    let download_path = temp_dir.path().join("download.zip");

    // Загрузка архива
    let mut response = client
        .get(&addon.link)
        .send()
        .context("Failed to download archive")?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(&download_path).context("Failed to create temp file")?;

    let mut downloaded = 0;
    let mut buf = [0u8; 8192];
    while let Ok(bytes_read) = response.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buf[..bytes_read])
            .context("Failed to write to temp file")?;
        downloaded += bytes_read as u64;
        state.lock().unwrap().progress = downloaded as f32 / total_size as f32;
    }

    // Распаковка
    let extract_temp_dir = tempfile::tempdir().context("Failed to create extract dir")?;
    let file = File::open(&download_path).context("Failed to open downloaded file")?;
    let mut archive = ZipArchive::new(file).context("Invalid ZIP archive")?;
    archive
        .extract(extract_temp_dir.path())
        .context("Failed to extract archive")?;

    // Удаление старой версии
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).context("Failed to remove old version")?;
    }

    // Копирование
    let copy_options = CopyOptions::new()
        .overwrite(true)
        .content_only(false)
        .copy_inside(true);

    copy(extract_temp_dir.path(), &install_dir, &copy_options).context("Failed to copy files")?;

    Ok(check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let install_path = Path::new(&addon.target_path).join(&addon.name);
    if let Some(parent) = install_path.parent() {
        fs::create_dir_all(parent).context("Failed to create parent dir")?;
    }

    let mut response = client
        .get(&addon.link)
        .send()
        .context("Failed to download file")?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(&install_path).context("Failed to create target file")?;

    let mut downloaded = 0;
    let mut buf = [0u8; 8192];

    while let Ok(bytes_read) = response.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buf[..bytes_read])
            .context("Failed to write to file")?;
        downloaded += bytes_read as u64;
        state.lock().unwrap().progress = downloaded as f32 / total_size as f32;
    }

    Ok(check_addon_installed(addon))
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let install_path = Path::new(&addon.target_path).join(&addon.name);

    if install_path.exists() {
        if install_path.is_dir() {
            fs::remove_dir_all(&install_path).context("Failed to remove directory")?;
        } else {
            fs::remove_file(&install_path).context("Failed to remove file")?;
        }
    }

    Ok(!check_addon_installed(addon))
}
