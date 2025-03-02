use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{debug, error, info, warn};
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
    debug!("[{}] Checking installation status", addon.name);
    let install_path = Path::new(&addon.target_path).join(&addon.name);

    if install_path.exists() {
        debug!(
            "[{}] Main installation path exists: {:?}",
            addon.name, install_path
        );
        return true;
    }

    // Дополнительные проверки
    if let Ok(entries) = fs::read_dir(&addon.target_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |name| name.contains(&addon.name))
            {
                debug!("[{}] Found related file: {:?}", addon.name, path);
                return true;
            }
        }
    }
    false
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("[{}] Starting installation process", addon.name);
    let result = if addon.link.ends_with(".zip") {
        handle_zip_install(client, addon, state)
    } else {
        handle_file_install(client, addon, state)
    };

    result.inspect_err(|e| error!("[{}] Installation failed: {}", addon.name, e))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("[{}] Handling single file installation", addon.name);
    let temp_dir =
        tempdir().with_context(|| format!("[{}] Failed to create temp directory", addon.name))?;

    let download_path = temp_dir.path().join(&addon.name);
    debug!(
        "[{}] Temporary download path: {:?}",
        addon.name, download_path
    );

    // Загрузка файла
    download_file(client, &addon.link, &download_path, state.clone())
        .with_context(|| format!("[{}] Download failed", addon.name))?;

    // Проверка скачанного файла
    if !download_path.exists() {
        return Err(anyhow::anyhow!(
            "[{}] File not found after download",
            addon.name
        ));
    }

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    debug!(
        "[{}] Target installation path: {:?}",
        addon.name, install_path
    );

    // Создание родительских директорий
    if let Some(parent) = install_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("[{}] Failed to create directories", addon.name))?;
    }

    // Копирование файла
    fs::copy(&download_path, &install_path)
        .with_context(|| format!("[{}] File copy failed", addon.name))?;

    debug!("[{}] Installation completed successfully", addon.name);
    Ok(install_path.exists())
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("[{}] Handling ZIP archive installation", addon.name);
    let temp_dir =
        tempdir().with_context(|| format!("[{}] Failed to create temp directory", addon.name))?;

    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())
        .with_context(|| format!("[{}] ZIP download failed", addon.name))?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)
        .with_context(|| format!("[{}] Failed to create extract directory", addon.name))?;

    // Распаковка архива
    extract_zip(&download_path, &extract_dir)
        .with_context(|| format!("[{}] ZIP extraction failed", addon.name))?;

    let install_base = std::fs::canonicalize(Path::new(&addon.target_path))
        .with_context(|| format!("[{}] Invalid target path", addon.name))?;

    // Обработка содержимого архива
    let entries: Vec<_> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let valid = !name.starts_with("__") && !name.contains("macosx");
            if !valid {
                debug!(
                    "[{}] Filtered out archive entry: {:?}",
                    addon.name,
                    e.path()
                );
            }
            valid
        })
        .collect();

    match entries.len() {
        0 => Err(anyhow::anyhow!(
            "[{}] Archive contains no valid files",
            addon.name
        )),
        1 => {
            let source_dir = entries[0].path();
            let install_path = install_base.join(&addon.name);
            copy_all_contents(&source_dir, &install_path)
                .with_context(|| format!("[{}] Failed to copy single directory", addon.name))
        }
        _ => {
            for entry in entries {
                let source_dir = entry.path();
                let dir_name = entry.file_name();
                let install_path = install_base.join(dir_name);
                copy_all_contents(&source_dir, &install_path)
                    .with_context(|| format!("[{}] Multi-directory copy failed", addon.name))?;
            }
            Ok(())
        }
    }?;

    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    debug!("Copying contents from {:?} to {:?}", source, dest);
    if !source.exists() {
        return Err(anyhow::anyhow!("Source directory does not exist"));
    }

    let options = DirCopyOptions::new().overwrite(true).content_only(true);

    fs_extra::dir::copy(source, dest, &options)
        .map_err(|e| anyhow::anyhow!("Copy failed: {}", e))?;

    debug!("Copied {} items", fs::read_dir(dest)?.count());
    Ok(())
}

fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("Downloading {} to {:?}", url, path);
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to send request to {}", url))?;

    let total_size = response.content_length().unwrap_or(0);
    debug!("Content length: {} bytes", total_size);

    let mut file =
        File::create(path).with_context(|| format!("Failed to create file {:?}", path))?;

    let mut downloaded = 0u64;
    let mut buffer = [0u8; 8192];

    while let Ok(bytes_read) = response.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .with_context(|| "Failed to write to file")?;

        downloaded += bytes_read as u64;
        state
            .lock()
            .map(|mut s| s.progress = downloaded as f32 / total_size as f32)
            .unwrap_or_else(|e| error!("Lock error: {}", e));
    }

    debug!("Download completed: {} bytes written", downloaded);
    Ok(())
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    info!("Extracting ZIP archive: {:?}", zip_path);
    let file =
        File::open(zip_path).with_context(|| format!("Failed to open ZIP file {:?}", zip_path))?;

    let mut archive = ZipArchive::new(file).with_context(|| "Invalid ZIP archive format")?;

    archive
        .extract(target_dir)
        .with_context(|| "Failed to extract ZIP contents")?;

    debug!("Extracted {} files", archive.len());
    Ok(())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("[{}] Starting uninstallation", addon.name);
    let mut success = true;

    let targets = [
        PathBuf::from(&addon.target_path).join(&addon.name),
        PathBuf::from(&addon.target_path).join(format!("{}.zip", addon.name)),
    ];

    for path in &targets {
        if path.exists() {
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };

            if let Err(e) = result {
                error!("[{}] Failed to delete {:?}: {}", addon.name, path, e);
                success = false;
            }
        }
    }

    // Дополнительная очистка
    if let Ok(entries) = fs::read_dir(&addon.target_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.to_string_lossy().contains(&addon.name) {
                if let Err(e) = fs::remove_dir_all(&path).or_else(|_| fs::remove_file(&path)) {
                    error!("[{}] Cleanup failed for {:?}: {}", addon.name, path, e);
                }
            }
        }
    }

    Ok(success && !check_addon_installed(addon))
}
