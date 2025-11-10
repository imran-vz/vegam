mod iroh;
mod platform;
mod state;

use iroh::transfer::BlobTicketInfo;
use state::{AppState, PeerInfo, TransferDirection, TransferInfo, TransferStatus};
use std::path::PathBuf;
use tauri::{Emitter, State};
use tauri_plugin_log::{log, Target, TargetKind};
use tracing::info;

#[tauri::command]
async fn init_node(state: State<'_, AppState>) -> Result<String, String> {
    info!("Initializing Iroh node");

    let endpoint = iroh::node::initialize_endpoint()
        .await
        .map_err(|e| format!("Failed to initialize node: {}", e))?;

    let node_id = iroh::node::get_node_id(&endpoint);

    // Create blob store
    let blob_store = iroh_blobs::store::mem::Store::new();

    // Start blob provider to serve blobs to peers
    iroh::transfer::start_blob_provider(endpoint.clone(), blob_store.clone());

    state.set_endpoint(endpoint).await;
    state.set_blob_store(blob_store).await;

    Ok(node_id)
}

#[tauri::command]
async fn get_node_id(state: State<'_, AppState>) -> Result<String, String> {
    let endpoint = state
        .get_endpoint()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    Ok(iroh::node::get_node_id(&endpoint))
}

#[tauri::command]
async fn send_file(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    file_path: String,
) -> Result<BlobTicketInfo, String> {
    info!("Sending file: {}", file_path);

    let endpoint = state
        .get_endpoint()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let blob_store = state
        .get_blob_store()
        .await
        .map_err(|e| format!("Blob store not initialized: {}", e))?;

    // Read file using platform-specific handler (handles Android content URIs)
    let file_data = platform::read_file(&app, &file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let ticket_info =
        iroh::transfer::create_send_ticket_from_bytes(&endpoint, &blob_store, file_data, file_path)
            .await
            .map_err(|e| format!("Failed to create ticket: {}", e))?;

    // Add transfer to state
    let transfer = TransferInfo {
        id: ticket_info.transfer_id.clone(),
        file_name: ticket_info.file_name.clone(),
        file_size: ticket_info.file_size,
        bytes_transferred: ticket_info.file_size, // Already "transferred" since we read it
        status: TransferStatus::Completed,
        error: None,
        direction: TransferDirection::Send,
    };
    state.add_transfer(transfer.clone()).await;

    // Emit event
    let _ = app.emit("transfer-update", &transfer);

    Ok(ticket_info)
}

#[tauri::command]
async fn receive_file(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    ticket: String,
    output_path: String,
) -> Result<TransferInfo, String> {
    info!("Receiving file to: {}", output_path);

    let endpoint = state
        .get_endpoint()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let blob_store = state
        .get_blob_store()
        .await
        .map_err(|e| format!("Blob store not initialized: {}", e))?;

    let path = PathBuf::from(&output_path);

    // Create progress callback
    let app_clone = app.clone();
    let progress_callback = move |transfer_id: String, bytes_transferred: u64, total_bytes: u64| {
        let progress = TransferInfo {
            id: transfer_id.clone(),
            file_name: String::new(), // Will be set in final transfer
            file_size: total_bytes,
            bytes_transferred,
            status: TransferStatus::InProgress,
            error: None,
            direction: TransferDirection::Receive,
        };
        let _ = app_clone.emit("transfer-progress", &progress);
    };

    // Attempt download
    let transfer =
        iroh::transfer::receive_file(&endpoint, &blob_store, ticket, path, progress_callback)
            .await
            .map_err(|e| format!("Failed to receive file: {}", e))?;

    // Add to state and emit event
    state.add_transfer(transfer.clone()).await;
    let _ = app.emit("transfer-update", &transfer);

    Ok(transfer)
}

#[tauri::command]
async fn get_transfer_status(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<Option<TransferInfo>, String> {
    Ok(state.get_transfer(&transfer_id).await)
}

#[tauri::command]
async fn list_peers(state: State<'_, AppState>) -> Result<Vec<PeerInfo>, String> {
    Ok(state.get_peers().await)
}

#[tauri::command]
fn get_device_name() -> String {
    iroh::discovery::get_device_name()
}

#[derive(serde::Serialize)]
struct TicketMetadata {
    filename: String,
    size: u64,
}

#[tauri::command]
fn parse_ticket_metadata(ticket: String) -> Result<TicketMetadata, String> {
    let (filename, size, _) = iroh::transfer::parse_enhanced_ticket(&ticket)
        .map_err(|e| format!("Failed to parse ticket: {}", e))?;
    Ok(TicketMetadata { filename, size })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    #[cfg(target_os = "android")]
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_android_fs::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        );

    #[cfg(not(target_os = "android"))]
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        );

    builder
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            init_node,
            get_node_id,
            send_file,
            receive_file,
            get_transfer_status,
            list_peers,
            get_device_name,
            parse_ticket_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
