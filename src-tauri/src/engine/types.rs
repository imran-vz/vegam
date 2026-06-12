//! Domain types shared between the engine, persistence, and the Tauri API.
//!
//! These are the wire shapes the frontend sees; keep them in sync with
//! `src/lib/api.ts`. Enums serialize camelCase; struct fields stay
//! snake_case to match the existing TS convention.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SendStatus {
    Importing,
    Available,
    Paused,
    Expired,
    /// Source len/mtime drifted; re-hash pending. Transient (gate answers
    /// RateLimited) — mtime-only churn must not kill transfers (ADR 0005).
    ContentSuspect,
    /// Re-hash confirmed the content differs. Terminal.
    ContentChanged,
    /// Source file is gone; the Sender may reselect it (ADR 0005).
    SourceMissing,
    Cancelled,
}

impl SendStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, SendStatus::ContentChanged | SendStatus::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiveStatus {
    Connecting,
    Downloading,
    Paused,
    StalledRetrying,
    Verifying,
    Exporting,
    Complete,
    Failed,
    Cancelled,
    NoLongerResumable,
}

impl ReceiveStatus {
    /// States that should be running a download task.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            ReceiveStatus::Connecting
                | ReceiveStatus::Downloading
                | ReceiveStatus::StalledRetrying
                | ReceiveStatus::Verifying
                | ReceiveStatus::Exporting
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionKind {
    Direct,
    Relayed,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendTransferInfo {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub source_path: String,
    /// `None` while importing.
    pub ticket: Option<String>,
    pub issued_at_ms: Option<u64>,
    pub expires_at_ms: Option<u64>,
    pub status: SendStatus,
    /// ADR 0016: count only, never identities.
    pub active_receiver_count: u32,
    /// 0..=1 during importing.
    pub import_progress: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveTransferInfo {
    pub id: String,
    pub file_name: String,
    pub size: u64,
    pub destination_path: String,
    pub status: ReceiveStatus,
    pub local_bytes: u64,
    pub connection_kind: Option<ConnectionKind>,
    pub error_code: Option<crate::engine::error::ErrorCode>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveProgress {
    pub id: String,
    pub local_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub connection_kind: Option<ConnectionKind>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketPreview {
    pub file_name: String,
    pub size: u64,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    /// Advisory only — the Sender's gate is authoritative.
    pub is_probably_expired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartialDownloadEntry {
    pub entry_id: String,
    pub kind: PartialDownloadKind,
    pub file_name: Option<String>,
    pub local_bytes: u64,
    pub total_bytes: Option<u64>,
    pub disk_bytes: u64,
    pub resumable: bool,
    pub stale: bool,
    pub status: Option<ReceiveStatus>,
    pub last_activity_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PartialDownloadKind {
    Tracked,
    Orphan,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResumeAreaReport {
    pub total_disk_bytes: u64,
    pub entries: Vec<PartialDownloadEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub settings: Settings,
    pub send_transfers: Vec<SendTransferInfo>,
    pub receive_transfers: Vec<ReceiveTransferInfo>,
}

pub fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
