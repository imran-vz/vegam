use anyhow::Result;
use iroh::blobs::store::Store;
use iroh::blobs::{BlobFormat, Hash, HashAndFormat, Tag};
use iroh::net::endpoint::Endpoint;
use iroh_blobs::provider::Ticket;
use iroh_blobs::util::SetTagOption;
use std::path::{Path, PathBuf};
use tracing::{info, error};
use uuid::Uuid;

use crate::state::{TransferInfo, TransferStatus};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlobTicketInfo {
    pub ticket: String,
    pub file_name: String,
    pub file_size: u64,
    pub transfer_id: String,
}

/// Add a file to the blob store and create a transfer ticket
pub async fn create_send_ticket(
    endpoint: &Endpoint,
    file_path: PathBuf,
) -> Result<BlobTicketInfo> {
    info!("Creating send ticket for file: {:?}", file_path);

    // Get file metadata
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?
        .to_string();

    let metadata = tokio::fs::metadata(&file_path).await?;
    let file_size = metadata.len();

    // Create in-memory blob store
    let db = iroh::blobs::store::mem::Store::new();

    // Import file into blob store
    let import_outcome = db
        .import_file(
            file_path.clone(),
            iroh_blobs::store::ImportMode::Copy,
            BlobFormat::Raw,
            Default::default(),
        )
        .await?;

    let hash = import_outcome.hash;
    info!("File imported with hash: {}", hash);

    // Create ticket with node address info
    let addr = crate::iroh::node::get_node_addr(endpoint);
    let ticket = Ticket::new(addr, hash, BlobFormat::Raw)?;
    let ticket_str = ticket.to_string();

    let transfer_id = Uuid::new_v4().to_string();

    Ok(BlobTicketInfo {
        ticket: ticket_str,
        file_name,
        file_size,
        transfer_id,
    })
}

/// Parse a ticket string and extract metadata
pub fn parse_ticket(ticket_str: &str) -> Result<Ticket> {
    let ticket: Ticket = ticket_str.parse()?;
    Ok(ticket)
}

/// Download a file from a ticket
pub async fn receive_file(
    endpoint: &Endpoint,
    ticket_str: String,
    output_path: PathBuf,
) -> Result<TransferInfo> {
    info!("Receiving file from ticket");

    // Parse ticket
    let ticket = parse_ticket(&ticket_str)?;
    let hash = ticket.hash();

    // Extract sender's node address
    let sender_addr = ticket.node_addr().clone();
    info!("Connecting to sender: {}", sender_addr.node_id);

    // Create temporary store for download
    let db = iroh::blobs::store::mem::Store::new();

    // Download blob
    info!("Starting download for hash: {}", hash);

    // Connect to sender
    let connection = endpoint.connect(sender_addr, iroh::blobs::protocol::ALPN).await?;

    // Request blob
    let request = iroh_blobs::protocol::GetRequest {
        hash,
        ranges: vec![],
    };

    // TODO: Implement actual blob transfer protocol
    // This is a simplified version - full implementation needs:
    // 1. Proper protocol handshake
    // 2. Streaming download with progress tracking
    // 3. Hash verification
    // 4. File writing to output_path

    let transfer_id = Uuid::new_v4().to_string();
    let file_name = output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(TransferInfo {
        id: transfer_id,
        file_name,
        file_size: 0, // Will be updated during transfer
        bytes_transferred: 0,
        status: TransferStatus::InProgress,
    })
}

/// Get transfer progress (placeholder for now)
pub async fn get_transfer_progress(transfer_id: &str) -> Result<u64> {
    // TODO: Implement actual progress tracking
    // This will require maintaining transfer state and monitoring
    // the blob download progress
    Ok(0)
}
