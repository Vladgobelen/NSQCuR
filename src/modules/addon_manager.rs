use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use reqwest::blocking::Client;
use std::{
    fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zip::ZipArchive;

const TEMP_DIR: &str = "tmp_debug";

pub fn check_addon_installed(addon: &Addon) -> bool {
    get_install_path(addon).exists()
}

fn get_install_path(addon: &Addon) -> PathBuf {
    Path::new(&addon.target_path).join(&addon.name)
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    fs::create_dir_all(TEMP_DIR)?;

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
    // 1. Скачивание
    let download_path = Path::new(TEMP_DIR).join(format!("{}.zip", addon.name));
    println!("[DEBUG] Скачиваем архив: {:?}", download_path);
    download_file(client, &addon.link, &download_path, state.clone())?;

    // 2. Распаковка
    let extract_dir = Path::new(TEMP_DIR).join(&addon.name);
    println!("[DEBUG] Распаковываем в: {:?}", extract_dir);
    extract_zip(&download_path, &extract_dir)?;

    // 3. Определяем корневую папку архива
    let archive_root = find_archive_root(&extract_dir)?;
    println!("[DEBUG] Корень архива: {:?}", archive_root);

    // 4. Перенос
    let install_path = get_install_path(addon);
    move_archive_root(&archive_root, &install_path)?;

    Ok(check_addon_installed(addon))
}

fn find_archive_root(extract_dir: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = fs::read_dir(extract_dir)?.filter_map(|e| e.ok()).collect();

    // Если есть ровно одна директория - используем её как корень
    if entries.len() == 1 && entries[0].path().is_dir() {
        Ok(entries[0].path())
    } else {
        Ok(extract_dir.to_path_buf())
    }
}

fn move_archive_root(source: &Path, dest: &Path) -> Result<()> {
    let options = DirCopyOptions::new()
        .overwrite(true)
        .copy_inside(true)
        .content_only(false);

    // Удаляем старую версию, если существует
    if dest.exists() {
        fs::remove_dir_all(dest)
            .with_context(|| format!("Не удалось удалить старую версию: {:?}", dest))?;
    }

    // Создаем родительскую директорию для dest
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    // Копируем всю папку (а не только её содержимое)
    fs_extra::dir::copy(
        source,
        dest.parent().unwrap_or_else(|| Path::new("")),
        &options,
    )?;

    println!(
        "[DEBUG] Перенос завершен: {:?} -> {:}",
        source,
        dest.display()
    );
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
    let install_path = get_install_path(addon);
    fs::create_dir_all(install_path.parent().unwrap())?;
    download_file(client, &addon.link, &install_path, state)?;
    Ok(install_path.exists())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let path = get_install_path(addon);
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(!check_addon_installed(addon))
}
