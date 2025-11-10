mod iroh;
mod platform;
mod state;

use iroh::transfer::BlobTicketInfo;
use state::{AppState, PeerInfo, TransferDirection, TransferInfo, TransferStatus};
use std::path::PathBuf;
use tauri::{Emitter, Manager, State};
use tauri_plugin_log::{log, Target, TargetKind};
use tracing::info;

#[tauri::command]
async fn init_node(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<String, String> {
    info!("Initializing Iroh node with gossip protocol");

    // Get data directory for persistent blob store
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to get data directory: {}", e))?
        .join("iroh");

    // Initialize Iroh with Router, Blobs, and Gossip
    let iroh = crate::iroh::Iroh::new(data_dir.clone())
        .await
        .map_err(|e| format!("Failed to initialize Iroh: {}", e))?;

    let node_id = iroh.node_addr.id.to_string();

    // Extract gossip receiver and sender for peer discovery
    let receiver = iroh
        .gossip
        .take_receiver()
        .await
        .map_err(|e| format!("Failed to get gossip receiver: {}", e))?;

    let sender = iroh.gossip.get_sender().await;

    // Spawn peer discovery task
    iroh::discovery::spawn_discovery_task(receiver, sender, node_id.clone(), app.clone());

    // Store iroh instance in state
    state.set_iroh(iroh).await;

    // Initialize debug instance if in debug mode
    #[cfg(debug_assertions)]
    {
        let debug_dir = data_dir.with_file_name("iroh-debug");
        let iroh_debug = crate::iroh::Iroh::new(debug_dir)
            .await
            .map_err(|e| format!("Failed to initialize debug Iroh: {}", e))?;

        let debug_receiver = iroh_debug
            .gossip
            .take_receiver()
            .await
            .map_err(|e| format!("Failed to get debug gossip receiver: {}", e))?;

        let debug_sender = iroh_debug.gossip.get_sender().await;
        let debug_node_id = iroh_debug.node_addr.id.to_string();

        iroh::discovery::spawn_discovery_task(
            debug_receiver,
            debug_sender,
            debug_node_id,
            app.clone(),
        );

        state.set_iroh_debug(iroh_debug).await;
    }

    info!(
        "Iroh node initialized successfully with node_id: {}",
        node_id
    );

    Ok(node_id)
}

#[tauri::command]
async fn get_node_id(state: State<'_, AppState>) -> Result<String, String> {
    let iroh = state
        .get_iroh()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    Ok(iroh.node_addr.id.to_string())
}

#[tauri::command]
async fn send_file(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    file_path: String,
) -> Result<BlobTicketInfo, String> {
    info!("Sending file: {}", file_path);

    let iroh = state
        .get_iroh()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    // Generate transfer ID upfront
    let transfer_id = uuid::Uuid::new_v4().to_string();

    // Emit initial pending status
    let initial_transfer = TransferInfo {
        id: transfer_id.clone(),
        file_name: std::path::PathBuf::from(&file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string(),
        file_size: 0,
        bytes_transferred: 0,
        status: TransferStatus::Pending,
        error: None,
        direction: TransferDirection::Send,
        speed_bps: 0,
    };
    state.add_transfer(initial_transfer.clone()).await;
    let _ = app.emit("transfer-update", &initial_transfer);

    // Read file using platform-specific handler (handles Android content URIs)
    let start_time = std::time::Instant::now();
    let file_data = platform::read_file(&app, &file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let file_size = file_data.len() as u64;
    let elapsed = start_time.elapsed().as_secs_f64();
    let speed_bps = if elapsed > 0.0 {
        (file_size as f64 / elapsed) as u64
    } else {
        0
    };

    // Emit reading complete status
    let reading_transfer = TransferInfo {
        id: transfer_id.clone(),
        file_name: initial_transfer.file_name.clone(),
        file_size,
        bytes_transferred: file_size,
        status: TransferStatus::InProgress,
        error: None,
        direction: TransferDirection::Send,
        speed_bps,
    };
    state.add_transfer(reading_transfer.clone()).await;
    let _ = app.emit("transfer-progress", &reading_transfer);

    let ticket_info = iroh::transfer::create_send_ticket(&iroh, file_data, file_path)
        .await
        .map_err(|e| format!("Failed to create ticket: {}", e))?;

    // Add final completed transfer to state
    let transfer = TransferInfo {
        id: transfer_id.clone(),
        file_name: ticket_info.file_name.clone(),
        file_size: ticket_info.file_size,
        bytes_transferred: ticket_info.file_size,
        status: TransferStatus::Completed,
        error: None,
        direction: TransferDirection::Send,
        speed_bps,
    };
    state.add_transfer(transfer.clone()).await;

    // Emit completed event
    let _ = app.emit("transfer-update", &transfer);

    // Return ticket info with transfer ID
    Ok(BlobTicketInfo {
        ticket: ticket_info.ticket,
        file_name: ticket_info.file_name,
        file_size: ticket_info.file_size,
        transfer_id,
    })
}

#[tauri::command]
async fn receive_file(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    ticket: String,
    output_path: String,
) -> Result<TransferInfo, String> {
    info!("Receiving file to: {}", output_path);

    let iroh = state
        .get_iroh()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    // Resolve to absolute path (handles relative paths from dialog)
    let path = if PathBuf::from(&output_path).is_absolute() {
        PathBuf::from(&output_path)
    } else {
        // Resolve relative to home directory for Downloads/ paths
        app.path()
            .resolve(&output_path, tauri::path::BaseDirectory::Home)
            .map_err(|e| format!("Failed to resolve path: {}", e))?
    };

    // Get node ID for ticket decryption
    let node_id = iroh.node_addr.id.to_string();

    // Parse and decrypt ticket to get file info for initial transfer
    let (filename, file_size, _) = iroh::transfer::parse_enhanced_ticket(&ticket, &node_id)
        .map_err(|e| format!("Invalid ticket: {}", e))?;

    let file_name = if filename != "received_file" {
        filename
    } else {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    };

    // Generate transfer ID and create initial transfer info
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let initial_transfer = TransferInfo {
        id: transfer_id.clone(),
        file_name: file_name.clone(),
        file_size,
        bytes_transferred: 0,
        status: TransferStatus::Pending,
        error: None,
        direction: TransferDirection::Receive,
        speed_bps: 0,
    };

    // Add to state and emit initial event
    state.add_transfer(initial_transfer.clone()).await;
    let _ = app.emit("transfer-update", &initial_transfer);

    // Clone necessary data before spawning to avoid lifetime issues
    let iroh_clone = iroh.clone();
    let transfers_arc = state.transfers.clone();

    // Spawn background task for download
    let app_clone = app.clone();
    let ticket_clone = ticket.clone();
    let transfer_id_clone = transfer_id.clone();
    let transfer_id_progress = transfer_id.clone();
    let file_name_clone = file_name.clone();
    let file_name_progress = file_name.clone();

    tokio::spawn(async move {
        // Create progress callback with 100ms throttling and speed tracking
        let app_progress = app_clone.clone();
        let last_emit = std::sync::Arc::new(std::sync::Mutex::new((
            std::time::Instant::now(),
            0u64, // last bytes transferred
        )));

        let progress_callback = move |_: String, bytes_transferred: u64, total_bytes: u64| {
            let mut last = last_emit.lock().unwrap();
            let now = std::time::Instant::now();

            // Only emit if 100ms has passed since last emit
            if now.duration_since(last.0).as_millis() >= 250 {
                let elapsed_secs = now.duration_since(last.0).as_secs_f64();
                let bytes_delta = bytes_transferred.saturating_sub(last.1);
                let speed_bps = if elapsed_secs > 0.0 {
                    (bytes_delta as f64 / elapsed_secs) as u64
                } else {
                    0
                };

                *last = (now, bytes_transferred);

                let progress = TransferInfo {
                    id: transfer_id_progress.clone(),
                    file_name: file_name_progress.clone(),
                    file_size: total_bytes,
                    bytes_transferred,
                    status: TransferStatus::InProgress,
                    error: None,
                    direction: TransferDirection::Receive,
                    speed_bps,
                };
                let _ = app_progress.emit("transfer-progress", &progress);
            }
        };

        // Attempt download
        let result =
            iroh::transfer::receive_file(&iroh_clone, ticket_clone, path, progress_callback).await;

        // Update final state based on result
        match result {
            Ok(mut transfer) => {
                // Use the original transfer_id
                transfer.id = transfer_id_clone.clone();
                let mut transfers = transfers_arc.write().await;
                transfers.insert(transfer.id.clone(), transfer.clone());
                drop(transfers);
                let _ = app_clone.emit("transfer-update", &transfer);
            }
            Err(e) => {
                let error_transfer = TransferInfo {
                    id: transfer_id_clone.clone(),
                    file_name: file_name_clone.clone(),
                    file_size,
                    bytes_transferred: 0,
                    status: TransferStatus::Failed,
                    error: Some(e.to_string()),
                    direction: TransferDirection::Receive,
                    speed_bps: 0,
                };
                let mut transfers = transfers_arc.write().await;
                transfers.insert(error_transfer.id.clone(), error_transfer.clone());
                drop(transfers);
                let _ = app_clone.emit("transfer-update", &error_transfer);
            }
        }
    });

    // Return immediately with pending transfer info
    Ok(initial_transfer)
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
async fn parse_ticket_metadata(
    state: State<'_, AppState>,
    ticket: String,
) -> Result<TicketMetadata, String> {
    let iroh = state
        .get_iroh()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let node_id = iroh.node_addr.id.to_string();
    let (filename, size, _) = iroh::transfer::parse_enhanced_ticket(&ticket, &node_id)
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
    info!("Getting relay status");
    let iroh = state
        .get_iroh()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let relay_urls: Vec<_> = iroh.node_addr.relay_urls().collect();
    let relay_url = relay_urls.first();

    if relay_url.is_none() {
        info!("No relay connection established - check network and relay server accessibility");
    } else {
        info!("Relay connected: {:?}", relay_url);
    }

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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_barcode_scanner::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_android_fs::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .filter(|metadata| {
                    metadata.target().starts_with("vegam_lib")
                        || metadata.level() <= log::Level::Error
                })
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        );

    #[cfg(not(target_os = "android"))]
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .filter(|metadata| {
                    metadata.target().starts_with("vegam_lib")
                        || metadata.level() <= log::Level::Error
                })
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
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
