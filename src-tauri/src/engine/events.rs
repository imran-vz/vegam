//! Engine domain events. The engine has zero Tauri imports — `lib.rs`
//! subscribes to this broadcast channel and forwards to the Tauri emitter.

use crate::engine::types::{ReceiveProgress, ReceiveTransferInfo, SendTransferInfo};

#[derive(Debug, Clone)]
pub enum EngineEvent {
    SendTransferUpdated(SendTransferInfo),
    ReceiveTransferUpdated(ReceiveTransferInfo),
    ReceiveTransferProgress(ReceiveProgress),
}

impl EngineEvent {
    /// The Tauri event name this engine event maps to.
    pub fn event_name(&self) -> &'static str {
        match self {
            EngineEvent::SendTransferUpdated(_) => "send-transfer-updated",
            EngineEvent::ReceiveTransferUpdated(_) => "receive-transfer-updated",
            EngineEvent::ReceiveTransferProgress(_) => "receive-transfer-progress",
        }
    }
}
