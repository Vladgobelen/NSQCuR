use crate::app::{Addon, AddonState};
use anyhow::Result;
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{debug, error, info, warn};
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
        debug!("[{}] Found in main path: {:?}", addon.name, main_path);
        return true;
    }

    let install_base = Path::new(&addon.target_path);
    match fs::read_dir(install_base) {
        Ok(entries) => entries.filter_map(|e| e.ok()).any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let found = name.starts_with(&addon.name) || name.contains(&addon.name);
            if found {
                debug!("[{}] Found component: {:?}", addon.name, e.path());
            }
            found
        }),
        Err(e) => {
            error!("[{}] Directory error: {}", addon.name, e);
            false
        }
    }
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("[{}] Starting installation", addon.name);
    if addon.link.ends_with(".zip") {
        handle_zip_install(client, addon, state)
    } else {
        handle_file_install(client, addon, state)
    }
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("[{}] File installation started", addon.name);
    let temp_dir = tempdir().inspect_err(|e| {
        error!("[{}] Tempdir error: {}", addon.name, e);
        e
    })?;

    debug!("[{}] Temp dir: {:?}", addon.name, temp_dir.path());
    let download_path = temp_dir.path().join(&addon.name);

    // Скачивание
    download_file(client, &addon.link, &download_path, state)
        .inspect_err(|e| error!("[{}] Download failed: {}", addon.name, e))?;

    if !download_path.exists() {
        error!("[{}] File not found after download", addon.name);
        return Err(anyhow::anyhow!("File missing"));
    }

    // Установка
    let install_path = Path::new(&addon.target_path).join(&addon.name);
    debug!("[{}] Install path: {:?}", addon.name, install_path);

    if let Some(parent) = install_path.parent() {
        fs::create_dir_all(parent)
            .inspect_err(|e| error!("[{}] Create parent error: {}", addon.name, e))?;
    }

    fs::copy(&download_path, &install_path)
        .inspect_err(|e| error!("[{}] Copy error: {}", addon.name, e))?;

    let exists = install_path.exists();
    if exists {
        info!("[{}] Successfully installed", addon.name);
    } else {
        error!("[{}] Installation failed", addon.name);
    }

    Ok(exists)
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("[{}] ZIP installation started", addon.name);
    let temp_dir = tempdir().inspect_err(|e| {
        error!("[{}] Tempdir error: {}", addon.name, e);
        e
    })?;

    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)
        .inspect_err(|e| error!("[{}] Extract dir error: {}", addon.name, e))?;

    extract_zip(&download_path, &extract_dir)
        .inspect_err(|e| error!("[{}] Extract error: {}", addon.name, e))?;

    let install_base = std::fs::canonicalize(Path::new(&addon.target_path))
        .inspect_err(|e| error!("[{}] Canonicalize error: {}", addon.name, e))?;

    // Проверка структуры архива
    let entries: Vec<_> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let valid = !name.starts_with("__") && !name.contains("macos");
            if !valid {
                debug!("[{}] Filtered out: {:?}", addon.name, e.path());
            }
            valid
        })
        .collect();

    match entries.len() {
        0 => {
            warn!("[{}] Archive is empty!", addon.name);
            copy_all_contents(&extract_dir, &install_base)
        }
        1 => {
            let source_dir = entries[0].path();
            let install_path = install_base.join(&addon.name);
            copy_all_contents(&source_dir, &install_path)
        }
        _ => {
            for entry in entries {
                let source_dir = entry.path();
                let dir_name = entry.file_name();
                let install_path = install_base.join(dir_name);
                copy_all_contents(&source_dir, &install_path)?;
            }
            Ok(())
        }
    }?;

    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    info!("Copying from {:?} to {:?}", source, dest);

    if !source.exists() {
        return Err(anyhow::anyhow!("Source path not found"));
    }

    if dest.exists() {
        fs::remove_dir_all(dest).or_else(|_| fs::remove_file(dest))?;
    }

    fs_extra::dir::copy(
        source,
        dest,
        DirCopyOptions::new().overwrite(true).content_only(true),
    )?;

    Ok(())
}

fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    info!("Downloading {} to {:?}", url, path);
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

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    info!("[{}] Uninstalling...", addon.name);
    let mut success = true;
    let paths_to_delete = vec![
        Path::new(&addon.target_path).join(&addon.name),
        Path::new(&addon.target_path).join(format!("{}.zip", addon.name)),
    ];

    for path in paths_to_delete {
        if path.exists() {
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };

            if let Err(e) = result {
                error!("[{}] Delete error: {} ({:?})", addon.name, e, path);
                success = false;
            }
        }
    }

    // Удаление остаточных файлов
    if let Ok(entries) = fs::read_dir(&addon.target_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.to_string_lossy().contains(&addon.name) {
                let result = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };

                if let Err(e) = result {
                    error!("[{}] Cleanup error: {} ({:?})", addon.name, e, path);
                }
            }
        }
    }

    Ok(success && !check_addon_installed(addon))
}
