// Peer discovery and announcement via gossip protocol
//
// This module handles automatic peer discovery on the network by broadcasting
// presence announcements via the gossip protocol and listening for announcements
// from other peers.

use anyhow::Result;
use iroh_gossip::api::{GossipReceiver, GossipSender};
use n0_future::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::state::{AppState, PeerInfo};

const ANNOUNCEMENT_INTERVAL: Duration = Duration::from_secs(30);
const PEER_TIMEOUT: Duration = Duration::from_secs(90);

/// Peer announcement message broadcast via gossip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAnnouncement {
    pub node_id: String,
    pub device_name: String,
    pub timestamp: u64,
}

impl PeerAnnouncement {
    pub fn new(node_id: String, device_name: String) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            node_id,
            device_name,
            timestamp,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(Into::into)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }
}

/// Spawn background task for peer discovery
///
/// This task:
/// 1. Periodically broadcasts presence announcements
/// 2. Listens for announcements from other peers
/// 3. Updates peer list in AppState
/// 4. Emits events to frontend
pub fn spawn_discovery_task(
    mut receiver: GossipReceiver,
    sender: GossipSender,
    node_id: String,
    handle: AppHandle,
) {
    tokio::spawn(async move {
        info!("Starting peer discovery task");

        let device_name = get_device_name();
        let mut announcement_timer = interval(ANNOUNCEMENT_INTERVAL);

        loop {
            tokio::select! {
                // Periodic broadcast of our presence
                _ = announcement_timer.tick() => {
                    let announcement = PeerAnnouncement::new(
                        node_id.clone(),
                        device_name.clone()
                    );

                    match announcement.to_bytes() {
                        Ok(bytes) => {
                            if let Err(e) = sender.broadcast(bytes.into()).await {
                                warn!("Failed to broadcast announcement: {}", e);
                            } else {
                                info!("Broadcasted presence announcement");
                            }
                        }
                        Err(e) => {
                            error!("Failed to serialize announcement: {}", e);
                        }
                    }

                    // Check for timed-out peers
                    if let Err(e) = cleanup_stale_peers(&handle).await {
                        warn!("Failed to cleanup stale peers: {}", e);
                    }
                }

                // Listen for announcements from other peers
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(event)) => {
                            // Extract content from Event::Received variant
                            let content = match event {
                                iroh_gossip::api::Event::Received(m) => m.content,
                                _ => continue,
                            };

                            match PeerAnnouncement::from_bytes(&content) {
                                Ok(announcement) => {
                                    // Ignore our own announcements
                                    if announcement.node_id != node_id {
                                        if let Err(e) = handle_peer_announcement(
                                            announcement,
                                            &handle
                                        ).await {
                                            warn!("Failed to handle peer announcement: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse peer announcement: {}", e);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            warn!("Failed to receive gossip message: {:?}", e);
                        }
                        None => {
                            warn!("Gossip receiver closed");
                            break;
                        }
                    }
                }
            }
        }
    });
}

/// Handle a peer announcement
async fn handle_peer_announcement(
    announcement: PeerAnnouncement,
    handle: &AppHandle,
) -> Result<()> {
    let state = handle.state::<AppState>();

    let peer_info = PeerInfo {
        node_id: announcement.node_id.clone(),
        device_name: announcement.device_name.clone(),
        last_seen: announcement.timestamp,
    };

    // Check if this is a new peer
    let peers = state.peers.read().await;
    let is_new = !peers.contains_key(&announcement.node_id);
    drop(peers);

    // Add or update peer
    state.add_peer(peer_info.clone()).await;

    if is_new {
        info!(
            "Discovered new peer: {} ({})",
            peer_info.device_name, peer_info.node_id
        );

        // Emit peer discovered event
        handle.emit("peer-discovered", peer_info)?;
    }

    // Emit peer list updated event
    let all_peers = state.get_peers().await;
    handle.emit("peer-list-updated", all_peers)?;

    Ok(())
}

/// Remove peers that haven't been seen recently
async fn cleanup_stale_peers(handle: &AppHandle) -> Result<()> {
    let state = handle.state::<AppState>();
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let peers = state.peers.read().await;
    let stale_peers: Vec<String> = peers
        .iter()
        .filter(|(_, peer)| current_time - peer.last_seen > PEER_TIMEOUT.as_secs())
        .map(|(id, _)| id.clone())
        .collect();
    drop(peers);

    for node_id in stale_peers {
        info!("Removing stale peer: {}", node_id);
        state.remove_peer(&node_id).await;

        // Emit peer lost event
        handle.emit("peer-lost", node_id)?;
    }

    Ok(())
}

/// Get device hostname for friendly peer naming
pub fn get_device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .unwrap_or_else(|| "Unknown Device".to_string())
}
