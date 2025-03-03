use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use reqwest::blocking::Client;
use simplelog::*;
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
    main_path.exists()
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
    info!("Начало установки ZIP: {}", addon.name);

    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));

    info!("Скачивание: {} -> {}", addon.link, download_path.display());
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;

    info!("Распаковка: {}", download_path.display());
    extract_zip(&download_path, &extract_dir)?;

    let install_base = Path::new(&addon.target_path);
    let dir_entries: Vec<fs::DirEntry> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
        .collect();

    match dir_entries.len() {
        0 => {
            info!("Копирование содержимого в: {}", install_base.display());
            copy_all_contents(&extract_dir, install_base)?
        }
        1 => {
            let source_dir = dir_entries[0].path();
            let install_path = install_base.join(&addon.name);
            info!(
                "Копирование из {} в {}",
                source_dir.display(),
                install_path.display()
            );
            copy_all_contents(&source_dir, &install_path)?
        }
        _ => {
            for dir_entry in dir_entries {
                let source_dir = dir_entry.path();
                let dir_name = dir_entry.file_name();
                let install_path = install_base.join(dir_name);
                info!(
                    "Копирование компонента: {} -> {}",
                    source_dir.display(),
                    install_path.display()
                );
                copy_all_contents(&source_dir, &install_path)?;
            }
        }
    }

    info!("Успешная установка: {}", addon.name);
    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!(
        "Начало копирования: {} -> {}",
        source.display(),
        dest.display()
    );

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

    info!(
        "Копирование завершено: {} файлов",
        fs::read_dir(source)?.count()
    );
    Ok(())
}

fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("Начало загрузки: {}", url);

    let mut response = client
        .get(url)
        .header("User-Agent", "NightWatchUpdater/1.0")
        .send()?;

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

    info!(
        "Загрузка завершена: {} ({:.2} MB)",
        url,
        downloaded as f64 / 1024.0 / 1024.0
    );
    Ok(())
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    info!("Распаковка архива: {}", zip_path.display());

    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(target_dir)?;

    info!(
        "Успешно распакован: {} ({} файлов)",
        zip_path.display(),
        archive.len()
    );
    Ok(())
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Установка файла: {}", addon.name);

    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(&addon.name);
    download_file(client, &addon.link, &download_path, state)?;

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    fs::create_dir_all(install_path.parent().unwrap())?;
    fs::copy(&download_path, &install_path)?;

    info!("Файл установлен: {}", install_path.display());
    Ok(install_path.exists())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("Начало удаления: {}", addon.name);

    let main_path = Path::new(&addon.target_path).join(&addon.name);
    let mut success = true;

    if main_path.exists() {
        info!("Удаление основной папки: {}", main_path.display());
        if let Err(e) = fs::remove_dir_all(&main_path) {
            error!("Ошибка удаления: {}", e);
            success = false;
        }
    }

    let install_base = Path::new(&addon.target_path);
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                info!("Удаление компонента: {}", name);
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    error!("Ошибка удаления {}: {}", name, e);
                    success = false;
                }
            }
        }
    }

    if success {
        info!("Успешное удаление: {}", addon.name);
    } else {
        warn!("Частичное удаление: {}", addon.name);
    }
    Ok(success && !check_addon_installed(addon))
}
