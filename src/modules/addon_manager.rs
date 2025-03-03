use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{error, info, warn};
use reqwest::blocking::Client;
use std::{
    fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use zip::ZipArchive;

pub fn check_addon_installed(addon: &Addon) -> bool {
    let target_dir = Path::new(&addon.target_path);
    let entries = match fs::read_dir(target_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };

    entries.filter_map(|e| e.ok()).any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        name.starts_with(&addon.name) || name.contains(&addon.name)
    })
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
    info!("🚀 Starting ZIP install: {}", addon.name);

    // Скачивание файла
    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())?;

    // Проверка целостности архива
    let file = File::open(&download_path).context("❌ Failed to open ZIP file")?;
    let mut archive = match ZipArchive::new(file) {
        Ok(ar) => ar,
        Err(e) => {
            error!("💀 Invalid ZIP archive: {}", e);
            return Err(anyhow::anyhow!("Invalid ZIP archive"));
        }
    };

    // Распаковка
    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;
    archive
        .extract(&extract_dir)
        .context("🔧 Failed to extract ZIP")?;

    // Анализ содержимого архива
    let entries: Vec<PathBuf> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .collect();

    if entries.is_empty() {
        return Err(anyhow::anyhow!("📭 Empty ZIP archive"));
    }

    // Определение стратегии копирования
    let (source_dir, should_create_subdir) = match entries.as_slice() {
        // Если в архиве одна директория - используем её
        [single_entry] if single_entry.is_dir() => (single_entry.clone(), true),

        // Если несколько элементов - копируем всё содержимое
        _ => (extract_dir.clone(), false),
    };

    // Подготовка путей
    let target_dir = Path::new(&addon.target_path);
    let final_target = if should_create_subdir {
        target_dir.join(&addon.name)
    } else {
        target_dir.to_path_buf()
    };

    // Копирование
    fs::create_dir_all(&final_target)?;
    copy_all_contents(&source_dir, &final_target)?;

    info!("✅ Successfully installed: {}", addon.name);
    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!("📁 Copying: [{}] -> [{}]", source.display(), dest.display());

    if dest.exists() {
        fs::remove_dir_all(dest).context("🚮 Failed to clean target directory")?;
    }

    let options = DirCopyOptions::new().overwrite(true).content_only(true);

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = dest.join(entry.file_name());

        if entry_path.is_dir() {
            fs_extra::dir::copy(&entry_path, &target_path, &options)?;
        } else {
            fs::copy(&entry_path, &target_path)?;
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
    info!("⏬ Downloading: {}", url);

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
        "📥 Downloaded: {} ({:.2} MB)",
        url,
        downloaded as f64 / 1024.0 / 1024.0
    );
    Ok(())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("Starting uninstall: {}", addon.name);

    let main_path = Path::new(&addon.target_path).join(&addon.name);
    let mut success = true;

    if main_path.exists() {
        info!("Deleting main directory: {}", main_path.display());
        if let Err(e) = fs::remove_dir_all(&main_path) {
            error!("Deletion error: {}", e);
            success = false;
        }
    }

    let install_base = Path::new(&addon.target_path);
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                info!("Deleting component: {}", name);
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    error!("Component deletion error: {} - {}", name, e);
                    success = false;
                }
            }
        }
    }

    if success {
        info!("Uninstall successful: {}", addon.name);
    } else {
        warn!("Partial uninstall: {}", addon.name);
    }
    Ok(success && !check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Installing file: {}", addon.name);

    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(&addon.name);
    download_file(client, &addon.link, &download_path, state)?;

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    fs::create_dir_all(install_path.parent().unwrap())?;
    fs::copy(&download_path, &install_path)?;

    info!("File installed: {}", install_path.display());
    Ok(install_path.exists())
}
