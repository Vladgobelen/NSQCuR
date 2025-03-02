use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::{dir::CopyOptions as DirCopyOptions, file::CopyOptions as FileCopyOptions};
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

    // 3. Логирование структуры
    println!("[DEBUG] Содержимое архива:");
    log_directory_structure(&extract_dir)?;

    // 4. Перенос файлов
    let install_path = get_install_path(addon);
    move_contents(&extract_dir, &install_path)
        .with_context(|| format!("Ошибка переноса в {:?}", install_path))?;

    Ok(check_addon_installed(addon))
}

fn log_directory_structure(path: &Path) -> Result<()> {
    fn log_recursive(path: &Path, depth: usize) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            println!(
                "{}- {}",
                " ".repeat(depth * 2),
                entry_path.file_name().unwrap().to_string_lossy()
            );
            if entry_path.is_dir() {
                log_recursive(&entry_path, depth + 1)?;
            }
        }
        Ok(())
    }
    log_recursive(path, 0)
}

fn move_contents(source: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    let dir_options = DirCopyOptions::new().overwrite(true);
    let file_options = FileCopyOptions::new().overwrite(true);

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        if entry_path.is_dir() {
            fs_extra::dir::copy(entry_path, &target, &dir_options)?;
        } else {
            fs_extra::file::copy(entry_path, &target, &file_options)?;
        }
        println!("[DEBUG] Перенос: {:?} -> {:?}", entry_path, target);
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
