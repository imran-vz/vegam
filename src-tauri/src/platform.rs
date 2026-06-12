use std::io;

/// Read a file from the local filesystem.
///
/// Desktop-only. The former Android `content://` URI handling is documented in
/// `docs/mobile/2026-06-12-mobile-implementation-knowledge.md`.
pub async fn read_file(_app: &tauri::AppHandle, path: &str) -> io::Result<Vec<u8>> {
    log::info!("Desktop: reading file: {}", path);

    tokio::fs::read(path).await
}
