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
        debug!("Addon {} found in main path: {:?}", addon.name, main_path);
        return true;
    }

    let install_base = Path::new(&addon.target_path);
    match fs::read_dir(install_base) {
        Ok(entries) => entries.filter_map(|e| e.ok()).any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let found = name.starts_with(&addon.name) || name.contains(&addon.name);
            if found {
                debug!("Found addon component: {:?}", e.path());
            }
            found
        }),
        Err(e) => {
            error!("Error reading directory: {}", e);
            false
        }
    }
}

pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Starting installation of {}", addon.name);
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
    info!("Processing file installation for {}", addon.name);
    let temp_dir = tempdir().inspect_err(|e| error!("Tempdir error: {}", e))?;
    let download_path = temp_dir.path().join(&addon.name);

    download_file(client, &addon.link, &download_path, state)?;

    let install_path = Path::new(&addon.target_path).join(&addon.name);
    fs::create_dir_all(install_path.parent().unwrap())
        .inspect_err(|e| error!("Create parent dir error: {}", e))?;

    fs::copy(&download_path, &install_path).inspect_err(|e| error!("File copy error: {}", e))?;

    Ok(install_path.exists())
}

fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    info!("Processing ZIP archive for {}", addon.name);
    let temp_dir = tempdir().inspect_err(|e| error!("Tempdir error: {}", e))?;
    debug!("Created temp dir: {:?}", temp_dir.path());

    let download_path = temp_dir.path().join(format!("{}.zip", addon.name));
    info!("Downloading to: {:?}", download_path);
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = temp_dir.path().join("extracted");
    fs::create_dir_all(&extract_dir).inspect_err(|e| error!("Extract dir error: {}", e))?;
    info!("Extracting to: {:?}", extract_dir);
    extract_zip(&download_path, &extract_dir)?;

    if let Ok(entries) = fs::read_dir(&extract_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            debug!("Extracted item: {:?}", entry.path());
        }
    }

    let install_base = std::fs::canonicalize(Path::new(&addon.target_path))
        .inspect_err(|e| error!("Canonicalize error: {}", e))?;

    let test_file = install_base.join(".tmp_write_test");
    fs::write(&test_file, "test").inspect_err(|e| error!("Write test failed: {}", e))?;
    fs::remove_file(&test_file).inspect_err(|e| error!("Cleanup failed: {}", e))?;

    let entries: Vec<_> = fs::read_dir(&extract_dir)
        .inspect_err(|e| error!("Read dir error: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let valid = !name.starts_with("__") && !name.is_empty();
            if !valid {
                debug!("Filtered out: {:?}", e.path());
            }
            valid
        })
        .collect();

    info!("Filtered archive content:");
    for entry in &entries {
        info!("- {:?}", entry.path());
    }

    match entries.len() {
        0 => {
            warn!("Empty archive after filtering!");
            copy_all_contents(&extract_dir, &install_base)
        }
        1 => {
            let source_dir = entries[0].path();
            let install_path = install_base.join(&addon.name);
            info!(
                "Copying single directory: {:?} -> {:?}",
                source_dir, install_path
            );
            copy_all_contents(&source_dir, &install_path)
        }
        _ => {
            info!("Copying multiple directories");
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
    info!("Copying contents from {:?} to {:?}", source, dest);
    if !source.exists() {
        error!("Source directory does not exist: {:?}", source);
        return Err(anyhow::anyhow!("Source directory not found"));
    }

    if dest.exists() {
        warn!("Removing existing directory: {:?}", dest);
        fs::remove_dir_all(dest).inspect_err(|e| error!("Remove dir error: {}", e))?;
    }

    fs::create_dir_all(dest).inspect_err(|e| error!("Create dir error: {}", e))?;

    let options = DirCopyOptions::new().overwrite(true).content_only(true);

    for entry in fs::read_dir(source).inspect_err(|e| error!("Read dir error: {}", e))? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        debug!("Copying: {:?} -> {:?}", entry_path, target);

        if entry_path.is_dir() {
            fs_extra::dir::copy(&entry_path, &target, &options)
                .inspect_err(|e| error!("Directory copy error: {}", e))?;
        } else {
            fs::copy(&entry_path, &target).inspect_err(|e| error!("File copy error: {}", e))?;
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
    info!("Downloading {} to {:?}", url, path);
    let mut response = client
        .get(url)
        .send()
        .inspect_err(|e| error!("Request failed: {}", e))?;

    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(path).inspect_err(|e| error!("File create error: {}", e))?;

    let mut downloaded = 0;
    let mut buf = [0u8; 8192];

    while let Ok(bytes_read) = response.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buf[..bytes_read])
            .inspect_err(|e| error!("Write error: {}", e))?;
        downloaded += bytes_read as u64;
        state.lock().unwrap().progress = downloaded as f32 / total_size as f32;
    }
    info!("Download completed: {}", url);
    Ok(())
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    info!("Extracting ZIP: {:?}", zip_path);
    let file = File::open(zip_path).inspect_err(|e| error!("Open ZIP error: {}", e))?;
    let mut archive = ZipArchive::new(file).inspect_err(|e| error!("ZIP archive error: {}", e))?;
    archive
        .extract(target_dir)
        .inspect_err(|e| error!("Extraction error: {}", e))?;
    Ok(())
}

pub fn uninstall_addon(addon: &Addon) -> Result<bool> {
    let main_path = Path::new(&addon.target_path).join(&addon.name);
    let mut success = true;

    if main_path.exists() {
        if let Err(e) = fs::remove_dir_all(&main_path) {
            error!("Error deleting main folder: {}", e);
            success = false;
        }
    }

    let install_base = Path::new(&addon.target_path);
    if let Ok(entries) = fs::read_dir(install_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&addon.name) {
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    error!("Error deleting component {}: {}", name, e);
                    success = false;
                }
            }
        }
    }

    Ok(success && !check_addon_installed(addon))
}
