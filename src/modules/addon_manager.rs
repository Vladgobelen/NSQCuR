use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
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

    // Нормализуем пути и удаляем служебные директории
    let entries: Vec<_> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            // Игнорируем служебные папки, например, __MACOSX
            !name.starts_with("__") && !name.is_empty()
        })
        .collect();

    let install_base = Path::new(&addon.target_path);

    // Если в архиве только одна директория с названием аддона — копируем её
    let has_single_addon_dir = entries.iter().any(|e| {
        e.file_name().to_string_lossy().eq_ignore_ascii_case(&addon.name)
    });

    if has_single_addon_dir {
        let source_dir = extract_dir.join(&addon.name);
        let install_path = install_base.join(&addon.name);
        copy_all_contents(&source_dir, &install_path)?;
    } else {
        // Копируем все содержимое напрямую
        copy_all_contents(&extract_dir, install_base)?;
    }

    Ok(check_addon_installed(addon))
}


fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
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
