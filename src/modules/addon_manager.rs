use crate::app::{Addon, AddonState};
use anyhow::Result;
use fs_extra::dir::CopyOptions as DirCopyOptions;
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
    let install_path = get_install_path(addon);
    if addon.link.ends_with(".zip") {
        fs::read_dir(&install_path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| e.path().extension().is_some_and(|ext| ext == "toc"))
            })
            .unwrap_or(false)
    } else {
        install_path.exists()
    }
}

fn get_install_path(addon: &Addon) -> PathBuf {
    Path::new(&addon.target_path).join(&addon.name)
}

pub fn install_addon(client: &Client, addon: &Addon, state: Arc<Mutex<AddonState>>) -> Result<()> {
    let temp_dir = tempdir()?;
    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));

    println!("[DEBUG] Скачиваем архив: {:?}", download_path);
    download_file(client, &addon.link, &download_path, state.clone())?;

    extract_zip(&download_path, temp_dir.path())?;

    let root = find_archive_root(temp_dir.path());
    let install_path = get_install_path(addon);

    if install_path.exists() {
        if install_path.is_dir() {
            fs::remove_dir_all(&install_path)?;
        } else {
            fs::remove_file(&install_path)?;
        }
    }

    move_contents(&root, &install_path)?;
    Ok(())
}

fn find_archive_root(temp_path: &Path) -> PathBuf {
    let mut root = temp_path.to_path_buf();
    let entries: Vec<_> = fs::read_dir(temp_path)
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    if entries.len() == 1 && entries[0].path().is_dir() {
        root = entries[0].path();
    }
    root
}

fn move_contents(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs_extra::dir::copy(from, to, &DirCopyOptions::new().overwrite(true))?;
    } else {
        fs::copy(from, to)?;
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
    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file = File::create(path)?;
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

pub fn uninstall_addon(addon: &Addon) -> Result<()> {
    let path = get_install_path(addon);
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }

    // Проверяем, что аддон действительно удален
    if check_addon_installed(addon) {
        println!("[DEBUG] Аддон {} не был полностью удален", addon.name);
    } else {
        println!("[DEBUG] Аддон {} успешно удален", addon.name);
    }

    Ok(())
}
