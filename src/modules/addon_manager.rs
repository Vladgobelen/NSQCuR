fn handle_zip_install(
    client: &Client,
    addon: &Addon,
    install_path: PathBuf,
    state: Arc<Mutex<AddonState>>,
) -> Result<bool> {
    println!("[DEBUG] Starting ZIP install for: {}", addon.name);

    let temp_dir = tempfile::tempdir()?;
    let download_path = temp_dir.path().join("archive.zip");
    println!("[DEBUG] Downloading to: {:?}", download_path);
    download_file(client, &addon.link, &download_path, state.clone())?;

    let extract_dir = tempfile::tempdir()?;
    println!("[DEBUG] Extracting to: {:?}", extract_dir.path());
    extract_zip(&download_path, extract_dir.path())?;

    // Анализ содержимого архива
    let entries: Vec<_> = fs::read_dir(extract_dir.path())?
        .filter_map(|e| e.ok())
        .collect();

    println!("[DEBUG] Archive contents ({} items):", entries.len());
    for entry in &entries {
        println!("  - {:?}", entry.path());
    }

    match entries.len() {
        // Одна папка/файл - переносим содержимое
        1 => {
            let source = entries[0].path();
            println!("[DEBUG] Single item found: {:?}", source);

            if source.is_dir() {
                println!(
                    "[DEBUG] Moving directory contents from: {:?} to: {:?}",
                    source, install_path
                );

                // Удаляем целевую директорию если существует
                if install_path.exists() {
                    fs::remove_dir_all(&install_path)?;
                }
                fs::create_dir_all(&install_path)?;

                // Копируем содержимое папки
                for entry in fs::read_dir(&source)? {
                    let entry = entry?;
                    let target = install_path.join(entry.file_name());
                    println!("  [DEBUG] Moving: {:?} -> {:?}", entry.path(), target);

                    if entry.path().is_dir() {
                        fs_extra::dir::copy(entry.path(), &target, &CopyOptions::new())?;
                    } else {
                        fs::rename(entry.path(), target)?;
                    }
                }
            } else {
                println!("[DEBUG] Moving single file to: {:?}", install_path);
                fs::create_dir_all(install_path.parent().unwrap())?;
                fs::rename(&source, &install_path)?;
            }
        }

        // Несколько элементов - переносим всё
        _ => {
            println!("[DEBUG] Moving all contents to: {:?}", install_path);
            fs::create_dir_all(&install_path)?;

            for entry in entries {
                let entry_path = entry.path();
                let target = install_path.join(entry.file_name());
                println!("  [DEBUG] Moving: {:?} -> {:?}", entry_path, target);

                if entry_path.is_dir() {
                    fs_extra::dir::copy(entry_path, &target, &CopyOptions::new())?;
                } else {
                    fs::rename(entry_path, target)?;
                }
            }
        }
    }

    println!(
        "[DEBUG] Install completed. Path exists: {}",
        check_addon_installed(addon)
    );

    Ok(check_addon_installed(addon))
}
