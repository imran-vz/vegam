mod iroh;
mod state;

use iroh::transfer::BlobTicketInfo;
use state::{AppState, PeerInfo, TransferInfo};
use std::path::PathBuf;
use tauri::State;
use tracing::info;

#[tauri::command]
async fn init_node(state: State<'_, AppState>) -> Result<String, String> {
    info!("Initializing Iroh node");

    let endpoint = iroh::node::initialize_endpoint()
        .await
        .map_err(|e| format!("Failed to initialize node: {}", e))?;

    let node_id = iroh::node::get_node_id(&endpoint);
    state.set_endpoint(endpoint).await;

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
    file_path: String,
) -> Result<BlobTicketInfo, String> {
    info!("Sending file: {}", file_path);

    let endpoint = state
        .get_endpoint()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let path = PathBuf::from(file_path);
    let ticket_info = iroh::transfer::create_send_ticket(&endpoint, path)
        .await
        .map_err(|e| format!("Failed to create ticket: {}", e))?;

    // Add transfer to state
    let transfer = TransferInfo {
        id: ticket_info.transfer_id.clone(),
        file_name: ticket_info.file_name.clone(),
        file_size: ticket_info.file_size,
        bytes_transferred: 0,
        status: state::TransferStatus::Pending,
    };
    state.add_transfer(transfer).await;

    Ok(ticket_info)
}

#[tauri::command]
async fn receive_file(
    state: State<'_, AppState>,
    ticket: String,
    output_path: String,
) -> Result<TransferInfo, String> {
    info!("Receiving file to: {}", output_path);

    let endpoint = state
        .get_endpoint()
        .await
        .map_err(|e| format!("Node not initialized: {}", e))?;

    let path = PathBuf::from(output_path);
    let transfer = iroh::transfer::receive_file(&endpoint, ticket, path)
        .await
        .map_err(|e| format!("Failed to receive file: {}", e))?;

    state.add_transfer(transfer.clone()).await;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            init_node,
            get_node_id,
            send_file,
            receive_file,
            get_transfer_status,
            list_peers,
            get_device_name,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
