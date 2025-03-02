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
    let install_path = get_install_path(addon);
    install_path.exists()
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let install_path = get_install_path(addon);

    if addon.link.ends_with(".zip") {
        handle_zip_install(client, addon, install_path, state)
    } else {
        handle_file_install(client, addon, install_path, state)
    }
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    install_path: PathBuf,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let temp_dir = tempfile::tempdir()?;
    let download_path = temp_dir.path().join("download.zip");

    // Скачивание архива
    download_file(client, &addon.link, &download_path, state.clone())?;

    // Распаковка
    let extract_temp_dir = tempfile::tempdir()?;
    extract_zip(&download_path, extract_temp_dir.path())?;

    // Копирование в целевую директорию
    let copy_options = CopyOptions::new()
        .overwrite(true)
        .content_only(false)
        .copy_inside(true);

    if install_path.exists() {
        fs::remove_dir_all(&install_path)?;
    }

    copy(extract_temp_dir.path(), &install_path, &copy_options)?;

    Ok(check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    install_path: PathBuf,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    download_file(client, &addon.link, &install_path, state)?;
    Ok(check_addon_installed(addon))
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let install_path = get_install_path(addon);

    if install_path.exists() {
        if install_path.is_dir() {
            fs::remove_dir_all(&install_path)?;
        } else {
            fs::remove_file(&install_path)?;
        }
    }

    Ok(!check_addon_installed(addon))
}

// Вспомогательные функции
fn get_install_path(addon: &Addon) -> PathBuf {
    Path::new(&addon.target_path).join(&addon.name)
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
