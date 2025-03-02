use crate::app::{Addon, AddonState};
use anyhow::{Context, Result};
use fs_extra::dir::CopyOptions as DirCopyOptions;
use reqwest::blocking::Client;
use std::{
    fs,
    fs::File,
    io::{copy as io_copy, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zip::ZipArchive;

const TEMP_DIR: &str = "tmp_debug";

// Упрощенная проверка установки аддона
pub fn check_addon_installed(addon: &Addon) -> bool {
    let install_path = get_install_path(addon);

    if addon.link.ends_with(".zip") {
        // Используем более идиоматичный подход с .any()
        fs::read_dir(install_path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| e.path().extension().map_or(false, |ext| ext == "toc"))
            })
            .unwrap_or(false)
    } else {
        install_path.exists()
    }
}

// Возвращаем PathBuf напрямую
fn get_install_path(addon: &Addon) -> PathBuf {
    PathBuf::from(&addon.target_path)
}

// Основная логика установки
pub fn install_addon(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    fs::create_dir_all(TEMP_DIR).context("Не удалось создать временную директорию")?;

    if addon.link.ends_with(".zip") {
        handle_zip_install(client, addon, state)
    } else {
        handle_file_install(client, addon, state)
    }
}

// Установка из ZIP-архива
fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let download_path = Path::new(TEMP_DIR).join(format!("{}.zip", addon.name));
    download_file(client, &addon.link, &download_path, state.clone())
        .context("Ошибка загрузки архива")?;

    let extract_dir = Path::new(TEMP_DIR).join(&addon.name);
    extract_zip(&download_path, &extract_dir).context("Ошибка распаковки архива")?;

    let install_path = get_install_path(addon);
    let entries: Vec<_> = fs::read_dir(&extract_dir)
        .context("Не удалось прочитать распакованные файлы")?
        .collect::<Result<Vec<_>, _>>()?;

    // Обработка структуры архива
    let source_dir = if let [entry] = entries.as_slice() {
        if entry.file_type()?.is_dir() {
            entry.path()
        } else {
            extract_dir.clone()
        }
    } else {
        extract_dir.clone()
    };

    move_contents(&source_dir, &install_path).context("Ошибка копирования файлов")?;

    Ok(check_addon_installed(addon))
}

// Копирование содержимого с улучшенной обработкой ошибок
fn move_contents(source: &Path, dest: &Path) -> Result<()> {
    let options = DirCopyOptions::new().overwrite(true).content_only(true);

    // Очистка целевой директории
    if dest.exists() {
        fs::remove_dir_all(dest).context(format!("Не удалось удалить: {}", dest.display()))?;
    }

    fs::create_dir_all(dest)
        .context(format!("Не удалось создать директорию: {}", dest.display()))?;

    // Копирование с использованием fs_extra для директорий
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dest.join(entry.file_name());

        if entry_path.is_dir() {
            fs_extra::dir::copy(&entry_path, &target, &options).context(format!(
                "Ошибка копирования директории: {}",
                entry_path.display()
            ))?;
        } else {
            fs::copy(&entry_path, &target).context(format!(
                "Ошибка копирования файла: {}",
                entry_path.display()
            ))?;
        }
    }

    Ok(())
}

// Загрузка файла с улучшенным управлением прогрессом
fn download_file(
    client: &Client,
    url: &str,
    path: &Path,
    state: Arc<Mutex<AddonState>>,
) -> Result<()> {
    let mut response = client.get(url).send()?;
    let total_size = response.content_length().unwrap_or(1);
    let mut file = File::create(path)?;

    let mut buffer = Vec::with_capacity(total_size as usize);
    response.read_to_end(&mut buffer)?;
    file.write_all(&buffer)?;

    // Обновление прогресса
    let mut state = state.lock().unwrap();
    state.progress = 1.0;

    Ok(())
}

// Распаковка архива
fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(target_dir)?;
    Ok(())
}

// Установка одиночного файла
fn handle_file_install(
    client: &Client,
    addon: &Addon,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    let install_path = get_install_path(addon);
    fs::create_dir_all(install_path.parent().unwrap_or(Path::new(".")))
        .context("Не удалось создать целевую директорию")?;

    download_file(client, &addon.link, &install_path, state)?;
    Ok(install_path.exists())
}

// Удаление аддона
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
