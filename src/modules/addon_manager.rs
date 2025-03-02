use crate::app::{Addon, AddonState};
use anyhow::Result;
use fs_extra::dir::CopyOptions as DirCopyOptions;
use log::{debug, info};
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
    let install_path = Path::new(&addon.target_path).join(&addon.name);
    install_path.exists() && install_path.is_dir()
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    handle_zip_install(client, addon, state)
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Installing {}...", addon.name);

    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));

    // Download
    download_file(client, &addon.link, &download_path, state.clone())?;

    // Extract
    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir)?;
    extract_zip(&download_path, &extract_dir)?;

    // Prepare install path
    let install_base = Path::new(&addon.target_path)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&addon.target_path).to_path_buf());

    if !install_base.exists() {
        fs::create_dir_all(&install_base)?;
    }

    // Find root folder
    let dir_entries: Vec<_> = fs::read_dir(&extract_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    match dir_entries.len() {
        0 => copy_all_contents(&extract_dir, &install_base.join(&addon.name))?,
        1 => copy_all_contents(&dir_entries[0].path(), &install_base.join(&addon.name))?,
        _ => {
            for entry in dir_entries {
                let source = entry.path();
                let target = install_base.join(entry.file_name());
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
        fs::remove_dir_all(&dest)?;
    }

    fs::create_dir_all(&dest)?;

    let options = DirCopyOptions::new()
        .overwrite(true)
        .content_only(true)
        .copy_inside(true);

    fs_extra::dir::copy(&source, &dest, &options)?;

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
    let install_path = Path::new(&addon.target_path).join(&addon.name);

    if install_path.exists() {
        fs::remove_dir_all(&install_path)?;
    }

    Ok(!check_addon_installed(addon))
}
