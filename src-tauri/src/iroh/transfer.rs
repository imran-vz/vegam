use anyhow::Result;
use bytes::Bytes;
use iroh::base::ticket::BlobTicket;
use iroh::blobs::store::Store;
use iroh::blobs::BlobFormat;
use iroh::net::endpoint::Endpoint;
use iroh_blobs::store::Map;
use iroh_blobs::util::local_pool::LocalPool;
use std::path::PathBuf;
use tracing::{error, info};
use uuid::Uuid;

use crate::state::{TransferDirection, TransferInfo, TransferStatus};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlobTicketInfo {
    pub ticket: String,
    pub file_name: String,
    pub file_size: u64,
    pub transfer_id: String,
}

/// Add file bytes to blob store and create transfer ticket
pub async fn create_send_ticket_from_bytes(
    endpoint: &Endpoint,
    db: &iroh_blobs::store::mem::Store,
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

    // Import bytes into blob store
    let hash = db.import_bytes(file_data.into(), BlobFormat::Raw).await?;

    info!("File imported with hash: {:?}", hash);

    // Create ticket with node address info
    let addr = crate::iroh::node::get_node_addr(endpoint);
    let ticket = BlobTicket::new(addr, *hash.hash(), BlobFormat::Raw)?;
    let ticket_str = ticket.to_string();

    let transfer_id = Uuid::new_v4().to_string();

    // Encode filename and size in ticket format: filename|size|blob_ticket
    let enhanced_ticket = format!("{}|{}|{}", file_name, file_size, ticket_str);

    Ok(BlobTicketInfo {
        ticket: enhanced_ticket,
        file_name,
        file_size,
        transfer_id,
    })
}

/// Parse enhanced ticket format: filename|size|blob_ticket
/// Returns (filename, size, BlobTicket)
pub fn parse_enhanced_ticket(ticket_str: &str) -> Result<(String, u64, BlobTicket)> {
    let parts: Vec<&str> = ticket_str.splitn(3, '|').collect();

    if parts.len() == 3 {
        // Enhanced format with metadata
        let filename = parts[0].to_string();
        let size = parts[1].parse::<u64>()?;
        let ticket: BlobTicket = parts[2].parse()?;
        Ok((filename, size, ticket))
    } else {
        // Legacy format without metadata
        let ticket: BlobTicket = ticket_str.parse()?;
        Ok(("received_file".to_string(), 0, ticket))
    }
}

/// Parse a ticket string and extract metadata
pub fn parse_ticket(ticket_str: &str) -> Result<BlobTicket> {
    let (_filename, _size, ticket) = parse_enhanced_ticket(ticket_str)?;
    Ok(ticket)
}

/// Start blob provider to serve blobs to peers
pub fn start_blob_provider(endpoint: Endpoint, store: iroh_blobs::store::mem::Store) {
    tokio::spawn(async move {
        info!("Starting blob provider");
        let pool = LocalPool::single();
        let rt = pool.handle();
        loop {
            match endpoint.accept().await {
                Some(incoming) => {
                    let store = store.clone();
                    let rt = rt.clone();
                    tokio::spawn(async move {
                        match incoming.accept() {
                            Ok(connecting) => match connecting.await {
                                Ok(connection) => {
                                    info!("Accepted connection from peer");
                                    iroh_blobs::provider::handle_connection(
                                        connection,
                                        store,
                                        Default::default(),
                                        rt,
                                    )
                                    .await;
                                }
                                Err(e) => error!("Failed to await connection: {}", e),
                            },
                            Err(e) => error!("Failed to accept connection: {}", e),
                        }
                    });
                }
                None => {
                    info!("Endpoint closed");
                    break;
                }
            }
        }
    });
}

/// Download a file from a ticket with proper streaming
pub async fn receive_file<F>(
    endpoint: &Endpoint,
    db: &iroh_blobs::store::mem::Store,
    ticket_str: String,
    output_path: PathBuf,
    progress_callback: F,
) -> Result<TransferInfo>
where
    F: Fn(String, u64, u64) + Send + 'static,
{
    info!("Receiving file from ticket");

    // Parse ticket
    let ticket = parse_ticket(&ticket_str)?;
    let hash = ticket.hash();
    let sender_addr = ticket.node_addr().clone();

    let transfer_id = Uuid::new_v4().to_string();
    let file_name = output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    info!("Connecting to sender: {}", sender_addr.node_id);
    info!("Requesting hash: {}", hash);

    // Connect to sender
    let connection = endpoint
        .connect(sender_addr, iroh_blobs::protocol::ALPN)
        .await?;

    // Download blob directly
    let request = iroh_blobs::protocol::GetRequest::single(hash);
    let at_initial = iroh_blobs::get::fsm::start(connection, request);
    let at_connected = at_initial.next().await?;
    let connected_next = at_connected.next().await?;

    let at_start_root = match connected_next {
        iroh_blobs::get::fsm::ConnectedNext::StartRoot(s) => s,
        iroh_blobs::get::fsm::ConnectedNext::StartChild(_) => {
            anyhow::bail!("Unexpected child response");
        }
        iroh_blobs::get::fsm::ConnectedNext::Closing(_) => {
            anyhow::bail!("Connection closed unexpectedly");
        }
    };

    let at_blob_header = at_start_root.next();
    let (at_blob_content, _hash) = at_blob_header.next().await?;

    // Create file and write blob data with progress tracking
    let output_path_clone = output_path.clone();
    let file = iroh_io::File::create(move || std::fs::File::create(output_path_clone)).await?;

    // Manually create ProgressSliceWriter with 2-arg closure for AsyncSliceWriter compat
    let transfer_id_clone = transfer_id.clone();

    struct ProgressWrapper<W, F> {
        writer: W,
        callback: F,
    }

    impl<W: iroh_io::AsyncSliceWriter, F: FnMut(u64, usize)> iroh_io::AsyncSliceWriter
        for ProgressWrapper<W, F>
    {
        async fn write_bytes_at(&mut self, offset: u64, data: Bytes) -> std::io::Result<()> {
            (self.callback)(offset, data.len());
            self.writer.write_bytes_at(offset, data).await
        }

        async fn write_at(&mut self, offset: u64, data: &[u8]) -> std::io::Result<()> {
            (self.callback)(offset, data.len());
            self.writer.write_at(offset, data).await
        }

        async fn sync(&mut self) -> std::io::Result<()> {
            self.writer.sync().await
        }

        async fn set_len(&mut self, size: u64) -> std::io::Result<()> {
            self.writer.set_len(size).await
        }
    }

    let tracked_file = ProgressWrapper {
        writer: file,
        callback: move |offset, _len| {
            progress_callback(transfer_id_clone.clone(), offset, 0);
        },
    };

    let _at_end = at_blob_content.write_all(tracked_file).await?;

    info!("Download complete, verifying file size");

    // Get file size from the written file
    let file_size = tokio::fs::metadata(&output_path).await?.len();
    info!("File size: {} bytes", file_size);

    // Also store in blob store for future reference
    let entry = db.get(&hash).await?;
    if entry.is_none() {
        info!("Blob not in store, importing...");
        // Read file back and import - ensures consistency
        let data = tokio::fs::read(&output_path).await?;
        db.import_bytes(data.into(), iroh::blobs::BlobFormat::Raw)
            .await?;
    }

    Ok(TransferInfo {
        id: transfer_id,
        file_name,
        file_size,
        bytes_transferred: file_size,
        status: TransferStatus::Completed,
        error: None,
        direction: TransferDirection::Receive,
    })
}
