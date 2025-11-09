use std::io;

/// Read file with platform-specific handling
/// On Android, handles content:// URIs through tauri-plugin-android-fs
/// On desktop, uses standard file system
#[cfg(target_os = "android")]
pub async fn read_file(app: &tauri::AppHandle, path: &str) -> io::Result<Vec<u8>> {
    use std::io::Read;
    use tauri_plugin_android_fs::AndroidFsExt;
    use tauri_plugin_fs::FilePath;

    log::info!("Android: reading file: {}", path);

    let api = app.android_fs_async();

    // Convert path string to FileUri
    // For content URIs, parse as URL
    let file_path = if path.starts_with("content://") {
        let url = url::Url::parse(path)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        FilePath::Url(url)
    } else {
        FilePath::Path(path.into())
    };

    // Convert FilePath to FileUri (infallible conversion)
    let uri: tauri_plugin_android_fs::FileUri = file_path.into();

    // Open file for reading
    let mut file = api
        .open_file_readable(&uri)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    // Read to bytes
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    Ok(buffer)
}

#[cfg(not(target_os = "android"))]
pub async fn read_file(_app: &tauri::AppHandle, path: &str) -> io::Result<Vec<u8>> {
    log::info!("Desktop: reading file: {}", path);

    tokio::fs::read(path).await
}
