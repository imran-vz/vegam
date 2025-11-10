use anyhow::Result;
use iroh_blobs::ticket::BlobTicket;
use iroh_blobs::BlobFormat;
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

use crate::iroh::ticket_codec::{decrypt_ticket, encrypt_ticket};
use crate::iroh::Iroh;
use crate::state::{TransferDirection, TransferInfo, TransferStatus};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlobTicketInfo {
    pub ticket: String,
    pub file_name: String,
    pub file_size: u64,
    pub transfer_id: String,
}

/// Add file bytes to blob store and create transfer ticket
pub async fn create_send_ticket(
    iroh: &Iroh,
    file_data: Vec<u8>,
    file_path: String,
) -> Result<BlobTicketInfo> {
    info!(
        "Creating send ticket from bytes, original path: {}",
        file_path
    );

    let file_size = file_data.len() as u64;

    // Extract file name from path or use default
    let file_name = PathBuf::from(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    // Import bytes into blob store using Blobs API
    let tag_info = iroh.blobs.add_bytes(file_data).await?;
    let hash = tag_info.hash;

    info!("File imported with hash: {}", hash);

    // Create ticket with node address info
    let addr = iroh.node_addr.clone();

    info!("Creating ticket with node addr: {}", addr.id);
    info!(
        "Relay URLs in ticket: {:?}",
        addr.relay_urls().collect::<Vec<_>>()
    );

    // BlobTicket now takes EndpointAddr directly
    let ticket = BlobTicket::new(addr, hash, BlobFormat::Raw);
    let ticket_str = ticket.to_string();

    let transfer_id = Uuid::new_v4().to_string();

    // Encode filename and size in ticket format: filename|size|blob_ticket
    let enhanced_ticket = format!("{}|{}|{}", file_name, file_size, ticket_str);

    // Encrypt the ticket using AES-256-GCM with node ID as key derivation
    let node_id = iroh.node_addr.id.to_string();
    let encrypted_ticket = encrypt_ticket(&enhanced_ticket, &node_id)?;

    Ok(BlobTicketInfo {
        ticket: encrypted_ticket,
        file_name,
        file_size,
        transfer_id,
    })
}

/// Parse enhanced ticket format: filename|size|blob_ticket
/// Returns (filename, size, BlobTicket)
/// Decrypts the ticket using AES-256-GCM with the receiver's node ID
pub fn parse_enhanced_ticket(ticket_str: &str, node_id: &str) -> Result<(String, u64, BlobTicket)> {
    // Decrypt the ticket using the receiver's node ID
    let decrypted = decrypt_ticket(ticket_str, node_id)?;

    let parts: Vec<&str> = decrypted.splitn(3, '|').collect();

    if parts.len() == 3 {
        // Enhanced format with metadata
        let filename = parts[0].to_string();
        let size = parts[1].parse::<u64>()?;
        let ticket: BlobTicket = parts[2].parse()?;
        Ok((filename, size, ticket))
    } else {
        // Legacy format without metadata (shouldn't happen with encryption)
        let ticket: BlobTicket = decrypted.parse()?;
        Ok(("received_file".to_string(), 0, ticket))
    }
}

// Blob provider is now handled automatically by the Router pattern
// No need for manual start_blob_provider function

/// Download a file from a ticket with proper streaming
pub async fn receive_file<F>(
    iroh: &Iroh,
    ticket_str: String,
    output_path: PathBuf,
    progress_callback: F,
) -> Result<TransferInfo>
where
    F: Fn(String, u64, u64) + Send + 'static,
{
    use iroh_blobs::api::downloader::DownloadProgressItem;
    use n0_future::StreamExt;

    info!("Receiving file from ticket");

    // Get receiver's node ID for decryption
    let receiver_node_id = iroh.node_addr.id.to_string();

    // Parse and decrypt the ticket to get file size
    let (_filename, file_size, ticket) = parse_enhanced_ticket(&ticket_str, &receiver_node_id)?;
    let hash = ticket.hash();
    let sender_addr = ticket.addr().clone();

    let transfer_id = Uuid::new_v4().to_string();
    let file_name = output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    info!("Downloading from sender: {}", sender_addr.id);
    info!("Sender relay: {:?}", sender_addr.relay_urls().next());
    info!("Requesting hash: {}", hash);

    // Emit initial progress (0, file_size) if file size is known
    if file_size > 0 {
        progress_callback(transfer_id.clone(), 0, file_size);
    }

    // Download blob using downloader API with progress tracking
    let download = iroh.downloader.download(hash, Some(sender_addr.id));
    let mut stream = download.stream().await?;

    // Track bytes downloaded during network transfer
    let mut bytes_downloaded: u64 = 0;

    // Iterate through progress events
    while let Some(item) = stream.next().await {
        match item {
            DownloadProgressItem::Progress(bytes) => {
                bytes_downloaded = bytes;
                // Report download progress
                let total = if file_size > 0 {
                    file_size
                } else {
                    bytes_downloaded
                };
                progress_callback(transfer_id.clone(), bytes_downloaded, total);
            }
            DownloadProgressItem::Error(e) => {
                return Err(e);
            }
            _ => {}
        }
    }

    info!("Download complete, {} bytes received", bytes_downloaded);

    // Now blob is in store, read it and write to file
    let mut reader = iroh.blobs.reader(hash);
    let mut file_data = Vec::new();
    tokio::io::copy(&mut reader, &mut file_data).await?;
    tokio::fs::write(&output_path, &file_data).await?;

    let actual_file_size = file_data.len() as u64;
    info!("File written to disk, {} bytes", actual_file_size);

    // Call progress callback with final status
    progress_callback(transfer_id.clone(), actual_file_size, actual_file_size);

    Ok(TransferInfo {
        id: transfer_id,
        file_name,
        file_size: actual_file_size,
        bytes_transferred: actual_file_size,
        status: TransferStatus::Completed,
        error: None,
        direction: TransferDirection::Receive,
        speed_bps: 0,
    })
}
