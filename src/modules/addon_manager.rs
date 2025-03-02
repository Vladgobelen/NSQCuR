use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{debug, error, info};
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
    let install_base = Path::new(&addon.target_path);

    let main_path = install_base.join(&addon.name);
    if main_path.exists() {
        return true;
    }

    install_base
        .read_dir()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|e| e.file_name().to_string_lossy().contains(&addon.name))
        })
        .unwrap_or(false)
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Installing {} from ZIP", addon.name);

    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;
    extract_zip(&download_path, &extract_dir)?;

    let install_base = Path::new(&addon.target_path)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&addon.target_path).to_path_buf());

    debug!("Install base directory: {}", install_base.display());

    if !install_base.exists() {
        fs::create_dir_all(&install_base).context("Failed to create target directory")?;
    }

    let dir_entries: Vec<_> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.is_dir()).unwrap_or(false))
        .collect();

    match dir_entries.len() {
        0 => {
            info!("Copying root contents to target");
            copy_all_contents(&extract_dir, &install_base)?;
        }
        1 => {
            let source_dir = dir_entries[0].path();
            let target_dir = install_base.join(&addon.name);
            info!(
                "Copying single folder: {} -> {}",
                source_dir.display(),
                target_dir.display()
            );
            copy_all_contents(&source_dir, &target_dir)?;
        }
        _ => {
            info!("Copying multiple folders directly to target");
            for entry in dir_entries {
                let source = entry.path();
                let target = install_base.join(entry.file_name());
                debug!("Copying: {} -> {}", source.display(), target.display());
                copy_all_contents(&source, &target)?;
            }
        }
    }

    Ok(check_addon_installed(addon))
}

fn copy_all_contents(source: &Path, dest: &Path) -> Result<()> {
    let source = source.canonicalize()?;
    let dest = dest.canonicalize()?;

    debug!("Copying from {} to {}", source.display(), dest.display());

    if dest.exists() {
        fs::remove_dir_all(&dest).context(format!("Failed to clean target: {}", dest.display()))?;
    }

    fs::create_dir_all(&dest)?;

    let options = DirCopyOptions::new()
        .overwrite(true)
        .content_only(true)
        .copy_inside(true);

    fs_extra::dir::copy(&source, &dest, &options)
        .map(|_| ())
        .context(format!(
            "Failed to copy contents from {} to {}",
            source.display(),
            dest.display()
        ))?;

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

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let base_path = Path::new(&addon.target_path);
    let mut success = true;

    // Удаление основной папки
    let main_path = base_path.join(&addon.name);
    if main_path.exists() {
        if let Err(e) = fs::remove_dir_all(&main_path) {
            error!("Failed to delete {}: {}", main_path.display(), e);
            success = false;
        }
    }

    // Удаление всех связанных папок
    if let Ok(entries) = fs::read_dir(base_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() && entry.file_name().to_string_lossy().contains(&addon.name) {
                if let Err(e) = fs::remove_dir_all(&path) {
                    error!("Failed to delete {}: {}", path.display(), e);
                    success = false;
                }
            }
        }
    }

    Ok(success && !check_addon_installed(addon))
}
