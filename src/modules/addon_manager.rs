use crate::app::{Addon, AddonState};
use anyhow::Result;
use reqwest::blocking::Client;
use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use zip::ZipArchive;

pub fn check_addon_installed(addon: &Addon) -> bool {
    let path = if addon.delete_path.is_empty() {
        Path::new(&addon.target_path)
    } else {
        Path::new(&addon.delete_path)
    };

    let exists = path.exists();
    let correct_type = match addon.addon_type {
        0 | 2 => path.is_dir(),
        1 => path.is_file(),
        _ => false,
    };

    exists && correct_type
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    match addon.addon_type {
        0 | 2 => handle_zip_install(client, addon, state),
        1 => handle_file_install(client, addon, state),
        _ => anyhow::bail!("Unsupported addon type: {}", addon.addon_type),
    }
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let target_path = Path::new(&addon.target_path);
    let source_path = Path::new(&addon.source_path);

    let mut response = client.get(&addon.link).send()?;
    let mut buffer = Vec::new();
    response.read_to_end(&mut buffer)?;
    let mut reader = Cursor::new(buffer);
    let total_size = buffer.len() as u64;

    if target_path.exists() {
        fs::remove_dir_all(target_path)?;
    }
    fs::create_dir_all(target_path)?;

    let mut archive = ZipArchive::new(&mut reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_path = file.mangled_name();

        let relative_path = match source_path.to_str() {
            Some("") => file_path.clone(),
            _ => file_path
                .strip_prefix(source_path)
                .unwrap_or_else(|_| file_path.as_path())
                .to_path_buf(),
        };

        let outpath = target_path.join(relative_path);

        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }

        let pos = reader.position();
        state.lock().unwrap().progress = pos as f32 / total_size as f32;
    }

    Ok(check_addon_installed(addon))
}

fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let target_path = Path::new(&addon.target_path);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut response = client.get(&addon.link).send()?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(target_path)?;

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

    Ok(check_addon_installed(addon))
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let path = if addon.delete_path.is_empty() {
        Path::new(&addon.target_path)
    } else {
        Path::new(&addon.delete_path)
    };

    if path.exists() {
        match addon.addon_type {
            0 | 2 => fs::remove_dir_all(path)?,
            1 => fs::remove_file(path)?,
            _ => {}
        }
    }

    Ok(!check_addon_installed(addon))
}
