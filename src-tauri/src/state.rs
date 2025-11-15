use anyhow::Result;
use iroh_blobs::{api::tags::TagInfo, Hash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::iroh::Iroh;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferInfo {
    pub id: String,
    pub file_name: String,
    pub file_size: u64,
    pub bytes_transferred: u64,
    pub status: TransferStatus,
    pub error: Option<String>,
    pub direction: TransferDirection,
    #[serde(default)]
    pub speed_bps: u64, // bytes per second
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub device_name: String,
    pub last_seen: u64,
}

pub struct AppState {
    pub iroh: Arc<RwLock<Option<Iroh>>>,
    #[cfg(debug_assertions)]
    pub iroh_debug: Arc<RwLock<Option<Iroh>>>,
    // Keep tags alive to prevent MemStore GC of blobs during transfer
    pub blob_tags: Arc<RwLock<HashMap<Hash, Arc<TagInfo>>>>,
    pub transfers: Arc<RwLock<HashMap<String, TransferInfo>>>,
    pub peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            iroh: Arc::new(RwLock::new(None)),
            #[cfg(debug_assertions)]
            iroh_debug: Arc::new(RwLock::new(None)),
            blob_tags: Arc::new(RwLock::new(HashMap::new())),
            transfers: Arc::new(RwLock::new(HashMap::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_iroh(&self, iroh: Iroh) {
        let mut i = self.iroh.write().await;
        *i = Some(iroh);
    }

    #[cfg(debug_assertions)]
    pub async fn set_iroh_debug(&self, iroh: Iroh) {
        let mut i = self.iroh_debug.write().await;
        *i = Some(iroh);
    }

    pub async fn get_iroh(&self) -> Result<Iroh> {
        let iroh = self.iroh.read().await;
        iroh.clone()
            .ok_or_else(|| anyhow::anyhow!("Iroh node not initialized"))
    }

    #[cfg(debug_assertions)]
    pub async fn get_iroh_debug(&self) -> Result<Iroh> {
        let iroh = self.iroh_debug.read().await;
        iroh.clone()
            .ok_or_else(|| anyhow::anyhow!("Iroh debug node not initialized"))
    }

    /// Store tag to keep blob alive in MemStore
    pub async fn add_blob_tag(&self, hash: Hash, tag: Arc<TagInfo>) {
        let mut tags = self.blob_tags.write().await;
        tags.insert(hash, tag);
    }

    /// Remove tag to allow MemStore GC of blob
    #[allow(dead_code)]
    pub async fn remove_blob_tag(&self, hash: &Hash) {
        let mut tags = self.blob_tags.write().await;
        tags.remove(hash);
    }

    pub async fn add_transfer(&self, transfer: TransferInfo) {
        let mut transfers = self.transfers.write().await;
        transfers.insert(transfer.id.clone(), transfer);
    }

    // Reserved for future transfer progress tracking
    #[allow(dead_code)]
    pub async fn update_transfer_progress(&self, id: &str, bytes_transferred: u64) {
        let mut transfers = self.transfers.write().await;
        if let Some(transfer) = transfers.get_mut(id) {
            transfer.bytes_transferred = bytes_transferred;
            if bytes_transferred > 0 && transfer.status == TransferStatus::Pending {
                transfer.status = TransferStatus::InProgress;
            }
        }
    }

    // Reserved for future transfer status updates
    #[allow(dead_code)]
    pub async fn update_transfer_status(
        &self,
        id: &str,
        status: TransferStatus,
        error: Option<String>,
    ) {
        let mut transfers = self.transfers.write().await;
        if let Some(transfer) = transfers.get_mut(id) {
            transfer.status = status;
            transfer.error = error;
        }
    }

    pub async fn get_transfer(&self, id: &str) -> Option<TransferInfo> {
        let transfers = self.transfers.read().await;
        transfers.get(id).cloned()
    }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }

    pub async fn add_peer(&self, peer: PeerInfo) {
        let mut peers = self.peers.write().await;
        peers.insert(peer.node_id.clone(), peer);
    }

    pub async fn remove_peer(&self, node_id: &str) {
        let mut peers = self.peers.write().await;
        peers.remove(node_id);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
