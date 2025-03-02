use crate::app::{Addon, AddonState};
use anyhow::Result;
use fs_extra::{dir::CopyOptions, move_items};
use reqwest::blocking::Client;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tempfile;
use zip::ZipArchive;

pub fn check_addon_installed(addon: &Addon) -> bool {
    get_install_path(addon).exists()
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
    // 1. Скачивание архива
    let temp_dir = tempfile::tempdir()?;
    let download_path = temp_dir.path().join("archive.zip");
    download_file(client, &addon.link, &download_path, state.clone())?;

    // 2. Распаковка
    let extract_dir = tempfile::tempdir()?;
    extract_zip(&download_path, extract_dir.path())?;

    // 3. Анализ содержимого
    let entries: Vec<_> = fs::read_dir(extract_dir.path())?
        .filter_map(|e| e.ok())
        .collect();

    match entries.len() {
        // 4. Один элемент - переименовываем
        1 => {
            let source = entries[0].path();
            if source.is_dir() {
                move_renamed(&source, &install_path)?;
            } else {
                fs::create_dir_all(&install_path.parent().unwrap())?;
                fs::rename(&source, &install_path)?;
            }
        }
        // 5. Несколько элементов - перемещаем всё
        _ => {
            fs::create_dir_all(&install_path)?;
            move_all_contents(extract_dir.path(), &install_path)?;
        }
    }

    Ok(check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    install_path: PathBuf,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    fs::create_dir_all(install_path.parent().unwrap())?;
    download_file(client, &addon.link, &install_path, state)?;
    Ok(check_addon_installed(addon))
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let path = get_install_path(addon);
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(!check_addon_installed(addon))
}

// Вспомогательные функции
fn get_install_path(addon: &Addon) -> PathBuf {
    Path::new(&addon.target_path).join(&addon.name)
}

fn move_renamed(source: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::rename(source, dest)?;
    Ok(())
}

fn move_all_contents(source: &Path, dest: &Path) -> Result<()> {
    let options = CopyOptions::new().overwrite(true);
    let items: Vec<_> = fs::read_dir(source)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    move_items(&items, dest, &options)?;
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
