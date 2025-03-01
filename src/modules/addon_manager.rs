use crate::app::{Addon, AddonState};
use anyhow::Result;
use fs_extra::dir::{copy, CopyOptions};
use reqwest::blocking::Client;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile;
use zip::ZipArchive;

pub fn check_addon_installed(addon: &Addon) -> bool {
    let path = Path::new(&addon.target_path);
    let exists = path.exists();
    let correct_type = match addon.addon_type {
        0 | 2 => path.is_dir(),
        1 => path.is_file(),
        _ => false,
    };

    exists && correct_type
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    match addon.addon_type {
        0 | 2 => handle_zip_install(client, addon, state),
        1 => handle_file_install(client, addon, state),
        _ => anyhow::bail!("Unsupported addon type: {}", addon.addon_type),
    }
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let temp_dir = tempfile::tempdir()?;
    let download_path = temp_dir.path().join("download.zip");

    // Загрузка архива
    let mut response = client.get(&addon.link).send()?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(&download_path)?;

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

    // Распаковка во временную директорию
    let extract_temp_dir = tempfile::tempdir()?;
    let file = File::open(&download_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(extract_temp_dir.path())?;

    let source_path = Path::new(&addon.source_path);
    let source_dir = if addon.source_path.is_empty() {
        extract_temp_dir.path().to_path_buf()
    } else {
        extract_temp_dir.path().join(source_path)
    };

    let target_path = Path::new(&addon.target_path);

    // Удалить старую версию
    if target_path.exists() {
        if target_path.is_dir() {
            fs::remove_dir_all(target_path)?;
        } else {
            fs::remove_file(target_path)?;
        }
    }

    // Создать целевую директорию
    fs::create_dir_all(target_path)?;

    // Копировать содержимое
    let copy_options = CopyOptions::new()
        .overwrite(true)
        .content_only(addon.source_path.is_empty());

    if source_dir.is_dir() {
        copy(&source_dir, target_path, &copy_options)?;
    } else {
        anyhow::bail!("Source path is not a directory");
    }

    Ok(check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let target_path = Path::new(&addon.target_path);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut response = client.get(&addon.link).send()?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(target_path)?;

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

    Ok(check_addon_installed(addon))
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let path = Path::new(&addon.target_path);

    if path.exists() {
        match addon.addon_type {
            0 | 2 => fs::remove_dir_all(path)?,
            1 => fs::remove_file(path)?,
            _ => {}
        }
    }

    Ok(!check_addon_installed(addon))
}
