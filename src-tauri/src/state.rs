use anyhow::Result;
use iroh::net::endpoint::Endpoint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferInfo {
    pub id: String,
    pub file_name: String,
    pub file_size: u64,
    pub bytes_transferred: u64,
    pub status: TransferStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub device_name: String,
    pub last_seen: u64,
}

pub struct AppState {
    pub endpoint: Arc<RwLock<Option<Endpoint>>>,
    pub transfers: Arc<RwLock<HashMap<String, TransferInfo>>>,
    pub peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            endpoint: Arc::new(RwLock::new(None)),
            transfers: Arc::new(RwLock::new(HashMap::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_endpoint(&self, endpoint: Endpoint) {
        let mut ep = self.endpoint.write().await;
        *ep = Some(endpoint);
    }

    pub async fn get_endpoint(&self) -> Result<Endpoint> {
        let ep = self.endpoint.read().await;
        ep.clone()
            .ok_or_else(|| anyhow::anyhow!("Iroh node not initialized"))
    }

    pub async fn add_transfer(&self, transfer: TransferInfo) {
        let mut transfers = self.transfers.write().await;
        transfers.insert(transfer.id.clone(), transfer);
    }

    // pub async fn update_transfer_progress(&self, id: &str, bytes_transferred: u64) {
    //     let mut transfers = self.transfers.write().await;
    //     if let Some(transfer) = transfers.get_mut(id) {
    //         transfer.bytes_transferred = bytes_transferred;
    //     }
    // }

    // pub async fn update_transfer_status(&self, id: &str, status: TransferStatus) {
    //     let mut transfers = self.transfers.write().await;
    //     if let Some(transfer) = transfers.get_mut(id) {
    //         transfer.status = status;
    //     }
    // }

    pub async fn get_transfer(&self, id: &str) -> Option<TransferInfo> {
        let transfers = self.transfers.read().await;
        transfers.get(id).cloned()
    }

    // pub async fn add_peer(&self, peer: PeerInfo) {
    //     let mut peers = self.peers.write().await;
    //     peers.insert(peer.node_id.clone(), peer);
    // }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
