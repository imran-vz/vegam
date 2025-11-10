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
    let blob_store = iroh_blobs::store::mem::MemStore::new();

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

    // Extract filename for early tracking
    let file_name = std::path::PathBuf::from(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    let transfer_id = uuid::Uuid::new_v4().to_string();

    // Create pending transfer
    let pending_transfer = TransferInfo {
        id: transfer_id.clone(),
        file_name: file_name.clone(),
        file_size: 0,
        bytes_transferred: 0,
        status: TransferStatus::Pending,
        error: None,
        direction: TransferDirection::Send,
    };
    state.add_transfer(pending_transfer.clone()).await;
    let _ = app.emit("transfer-update", &pending_transfer);

    // Read file using platform-specific handler (handles Android content URIs)
    let file_data = match platform::read_file(&app, &file_path).await {
        Ok(data) => {
            // Update to in-progress after successful read
            state
                .update_transfer_status(&transfer_id, TransferStatus::InProgress, None)
                .await;
            data
        }
        Err(e) => {
            let error_msg = format!("Failed to read file: {}", e);
            state
                .update_transfer_status(
                    &transfer_id,
                    TransferStatus::Failed,
                    Some(error_msg.clone()),
                )
                .await;
            if let Some(failed_transfer) = state.get_transfer(&transfer_id).await {
                let _ = app.emit("transfer-update", &failed_transfer);
            }
            return Err(error_msg);
        }
    };

    let ticket_info = match iroh::transfer::create_send_ticket_from_bytes(
        &endpoint,
        &blob_store,
        file_data,
        file_path,
    )
    .await
    {
        Ok(info) => {
            // Update status to completed
            state
                .update_transfer_status(&transfer_id, TransferStatus::Completed, None)
                .await;

            // Update with final file size and bytes transferred
            let file_size = info.file_size;
            state
                .update_transfer_progress(&transfer_id, file_size)
                .await;

            // Get updated transfer and emit
            if let Some(completed_transfer) = state.get_transfer(&transfer_id).await {
                let _ = app.emit("transfer-update", &completed_transfer);
            }

            // Return with our transfer_id
            BlobTicketInfo {
                transfer_id,
                ..info
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to create ticket: {}", e);
            state
                .update_transfer_status(
                    &transfer_id,
                    TransferStatus::Failed,
                    Some(error_msg.clone()),
                )
                .await;
            if let Some(failed_transfer) = state.get_transfer(&transfer_id).await {
                let _ = app.emit("transfer-update", &failed_transfer);
            }
            return Err(error_msg);
        }
    };

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

    // Parse ticket to get metadata for initial tracking
    let (filename, file_size, _) = iroh::transfer::parse_enhanced_ticket(&ticket)
        .map_err(|e| format!("Failed to parse ticket: {}", e))?;

    let transfer_id = uuid::Uuid::new_v4().to_string();

    // Create pending transfer
    let pending_transfer = TransferInfo {
        id: transfer_id.clone(),
        file_name: filename.clone(),
        file_size,
        bytes_transferred: 0,
        status: TransferStatus::Pending,
        error: None,
        direction: TransferDirection::Receive,
    };
    state.add_transfer(pending_transfer.clone()).await;
    let _ = app.emit("transfer-update", &pending_transfer);

    // Create progress callback using state tracking
    let transfers_arc = state.transfers.clone();
    let app_clone = app.clone();
    let transfer_id_clone = transfer_id.clone();
    let progress_callback = move |_: String, bytes_transferred: u64, _total_bytes: u64| {
        let transfers = transfers_arc.clone();
        let app_ref = app_clone.clone();
        let tid = transfer_id_clone.clone();
        tokio::spawn(async move {
            // Update progress directly
            let mut transfers_guard = transfers.write().await;
            if let Some(transfer) = transfers_guard.get_mut(&tid) {
                transfer.bytes_transferred = bytes_transferred;
                if bytes_transferred > 0 && transfer.status == TransferStatus::Pending {
                    transfer.status = TransferStatus::InProgress;
                }
                let updated = transfer.clone();
                drop(transfers_guard); // Release lock before emitting
                let _ = app_ref.emit("transfer-progress", &updated);
            }
        });
    };

    // Update status to in-progress before starting download
    state
        .update_transfer_status(&transfer_id, TransferStatus::InProgress, None)
        .await;
    if let Some(in_progress_transfer) = state.get_transfer(&transfer_id).await {
        let _ = app.emit("transfer-update", &in_progress_transfer);
    }

    // Attempt download
    let result =
        iroh::transfer::receive_file(&endpoint, &blob_store, ticket, path, progress_callback).await;

    match result {
        Ok(mut transfer) => {
            // Use our transfer_id and update final status
            transfer.id = transfer_id.clone();
            state
                .update_transfer_status(&transfer_id, TransferStatus::Completed, None)
                .await;
            state
                .update_transfer_progress(&transfer_id, transfer.file_size)
                .await;

            // Get updated transfer from state to ensure consistency
            if let Some(final_transfer) = state.get_transfer(&transfer_id).await {
                let _ = app.emit("transfer-update", &final_transfer);
                Ok(final_transfer)
            } else {
                Ok(transfer)
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to receive file: {}", e);
            state
                .update_transfer_status(
                    &transfer_id,
                    TransferStatus::Failed,
                    Some(error_msg.clone()),
                )
                .await;
            if let Some(failed_transfer) = state.get_transfer(&transfer_id).await {
                let _ = app.emit("transfer-update", &failed_transfer);
            }
            Err(error_msg)
        }
    }
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

#[derive(serde::Serialize)]
struct RelayStatus {
    connected: bool,
    relay_url: Option<String>,
}

#[tauri::command]
async fn get_relay_status(state: State<'_, AppState>) -> Result<RelayStatus, String> {
    let endpoint = state
        .get_endpoint()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let addr = endpoint.addr();
    let relay_url = addr.relay_urls().next();
    Ok(RelayStatus {
        connected: relay_url.is_some(),
        relay_url: relay_url.map(|u| u.to_string()),
    })
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
            get_relay_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
