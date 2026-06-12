//! Transfer metadata persistence: just enough state to resume interrupted
//! Transfers across app restarts. No full history is kept (ADR 0009) —
//! Complete and Cancelled records are pruned.
//!
//! Ordering rule: a record is written BEFORE its blob is imported or tagged,
//! so the blob store never holds meaningful data unknown to metadata (this
//! is what makes the GC protect-callback in Phase 4 safe).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::engine::types::{ReceiveStatus, SendStatus};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRecord {
    pub id: String,
    pub source_path: PathBuf,
    pub file_name: String,
    pub size: u64,
    /// Content Identity, hex-encoded BLAKE3 hash.
    pub hash: String,
    /// Unix seconds; expiry = issued_at + 24h (ADR 0015).
    pub issued_at: u64,
    pub ticket: String,
    pub status: SendStatus,
    /// EndpointIds (hex) that had a request accepted before expiry; they may
    /// resume after expiry (ADR 0015). Persisted so this survives restarts.
    pub started_receivers: BTreeSet<String>,
    pub source_len: u64,
    pub source_mtime_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveRecord {
    pub id: String,
    /// Full ticket string — needed to reconnect across restarts.
    pub ticket: String,
    /// Content Identity, hex-encoded BLAKE3 hash.
    pub hash: String,
    pub file_name: String,
    pub size: u64,
    pub issued_at: u64,
    pub destination: PathBuf,
    pub status: ReceiveStatus,
    pub last_activity_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransfersFile {
    pub schema_version: u32,
    pub sends: Vec<SendRecord>,
    pub receives: Vec<ReceiveRecord>,
}

/// Should this record survive a save/load cycle? Terminal-and-useless
/// records are pruned; Failed/NoLongerResumable receive records are KEPT
/// because they still own bytes in the managed resume area that the user
/// must be able to see and clean up (ADR 0020).
pub fn keep_send(record: &SendRecord) -> bool {
    record.status != SendStatus::Cancelled
}

pub fn keep_receive(record: &ReceiveRecord) -> bool {
    !matches!(
        record.status,
        ReceiveStatus::Complete | ReceiveStatus::Cancelled
    )
}

pub fn load(path: &Path) -> Result<TransfersFile> {
    if !path.exists() {
        return Ok(TransfersFile {
            schema_version: SCHEMA_VERSION,
            ..Default::default()
        });
    }
    let raw = std::fs::read_to_string(path).context("reading transfers file")?;
    let mut file: TransfersFile = match serde_json::from_str(&raw) {
        Ok(file) => file,
        Err(e) => {
            // A corrupt metadata file must not brick the app. Preserve it
            // for inspection and start with an empty list; orphaned blobs
            // surface in the Storage tab rather than being lost.
            tracing::warn!("transfers file unparseable ({e}); moving it aside");
            let aside = path.with_extension("json.corrupt");
            let _ = std::fs::rename(path, &aside);
            return Ok(TransfersFile {
                schema_version: SCHEMA_VERSION,
                ..Default::default()
            });
        }
    };
    file.sends.retain(keep_send);
    file.receives.retain(keep_receive);
    Ok(file)
}

pub fn save(path: &Path, file: &TransfersFile) -> Result<()> {
    let mut pruned = file.clone();
    pruned.schema_version = SCHEMA_VERSION;
    pruned.sends.retain(keep_send);
    pruned.receives.retain(keep_receive);
    let json = serde_json::to_string_pretty(&pruned)?;
    write_atomic(path, json.as_bytes())
}

/// Atomic write: uniquely named temp file in the same directory, then
/// rename. The unique name keeps concurrent writers from interleaving on
/// one temp file (last rename wins, but every rename installs a complete,
/// valid file).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent directory"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string()),
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&tmp, bytes).context("writing temp file")?;
    std::fs::rename(&tmp, path).context("renaming temp file into place")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("vegam-persist-test-{}", uuid::Uuid::new_v4()))
            .join("transfers.json")
    }

    fn sample_send(status: SendStatus) -> SendRecord {
        SendRecord {
            id: uuid::Uuid::new_v4().to_string(),
            source_path: PathBuf::from("/tmp/file.bin"),
            file_name: "file.bin".into(),
            size: 42,
            hash: "ab".repeat(32),
            issued_at: 1_000,
            ticket: "vegamabc".into(),
            status,
            started_receivers: BTreeSet::new(),
            source_len: 42,
            source_mtime_unix_ms: 5,
        }
    }

    fn sample_receive(status: ReceiveStatus) -> ReceiveRecord {
        ReceiveRecord {
            id: uuid::Uuid::new_v4().to_string(),
            ticket: "vegamabc".into(),
            hash: "cd".repeat(32),
            file_name: "file.bin".into(),
            size: 42,
            issued_at: 1_000,
            destination: PathBuf::from("/tmp/out.bin"),
            status,
            last_activity_ms: 7,
        }
    }

    #[test]
    fn missing_file_loads_empty() {
        let file = load(&tmp_path()).unwrap();
        assert_eq!(file.schema_version, SCHEMA_VERSION);
        assert!(file.sends.is_empty());
        assert!(file.receives.is_empty());
    }

    #[test]
    fn roundtrip_prunes_terminal_records() {
        let path = tmp_path();
        let file = TransfersFile {
            schema_version: SCHEMA_VERSION,
            sends: vec![
                sample_send(SendStatus::Available),
                sample_send(SendStatus::Expired),
                sample_send(SendStatus::Cancelled),
            ],
            receives: vec![
                sample_receive(ReceiveStatus::Downloading),
                sample_receive(ReceiveStatus::Paused),
                sample_receive(ReceiveStatus::Failed),
                sample_receive(ReceiveStatus::NoLongerResumable),
                sample_receive(ReceiveStatus::Complete),
                sample_receive(ReceiveStatus::Cancelled),
            ],
        };
        save(&path, &file).unwrap();
        let loaded = load(&path).unwrap();
        // Cancelled send pruned; Expired kept (may still serve started receivers).
        assert_eq!(loaded.sends.len(), 2);
        // Complete + Cancelled receives pruned; Failed/NoLongerResumable kept
        // (they own visible resume-area bytes).
        assert_eq!(loaded.receives.len(), 4);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn write_atomic_replaces_content() {
        let path = tmp_path();
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
