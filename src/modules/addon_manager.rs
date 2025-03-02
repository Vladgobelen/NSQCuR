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
    let install_path = get_install_path(addon);
    if addon.link.ends_with(".zip") {
        // Для ZIP-аддонов проверяем наличие любого файла .toc
        fs::read_dir(install_path)
            .map(|entries| {
                entries
                    .filter(|e| {
                        e.as_ref()
                            .unwrap()
                            .path()
                            .extension()
                            .map_or(false, |ext| ext == "toc")
                    })
                    .next()
                    .is_some()
            })
            .unwrap_or(false)
    } else {
        // Для одиночных файлов проверяем существование
        install_path.exists()
    }
}

fn get_install_path(addon: &Addon) -> PathBuf {
    Path::new(&addon.target_path).to_path_buf()
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
    let download_path = Path::new(TEMP_DIR).join(format!("{}.zip", addon.name));
    println!("[DEBUG] Скачиваем архив: {:?}", download_path);
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = Path::new(TEMP_DIR).join(&addon.name);
    println!("[DEBUG] Распаковываем в: {:?}", extract_dir);
    extract_zip(&download_path, &extract_dir)?;

    let install_path = get_install_path(addon);
    let entries: Vec<_> = fs::read_dir(&extract_dir)?.collect();

    // Если есть ровно одна директория - используем её содержимое
    if entries.len() == 1 && entries[0].as_ref().unwrap().file_type()?.is_dir() {
        let root_folder = entries[0].as_ref().unwrap().path();
        move_contents(&root_folder, &install_path)?;
    } else {
        move_contents(&extract_dir, &install_path)?;
    }

    Ok(check_addon_installed(addon))
}

fn move_contents(source: &Path, dest: &Path) -> Result<()> {
    let options = DirCopyOptions::new().overwrite(true).content_only(true);

    if dest.exists() {
        fs::remove_dir_all(dest).context(format!("Ошибка удаления: {:?}", dest))?;
    }
    fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")))?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        if entry_path.is_dir() {
            fs_extra::dir::copy(&entry_path, &target, &options)?;
        } else {
            fs::copy(&entry_path, &target)?;
        }
        println!("[DEBUG] Копирование: {:?} -> {:?}", entry_path, target);
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
    let install_path = get_install_path(addon).join(&addon.name);
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
