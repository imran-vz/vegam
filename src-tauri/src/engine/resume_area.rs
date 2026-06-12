//! The managed resume area (Phase 4, ADR 0020): make Partial Downloads
//! visible, attribute their storage use, and give the user explicit cleanup.
//! Vegam never silently deletes resumable state — the GC can only collect
//! blobs whose records were removed by explicit user action or completion.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iroh_blobs::Hash;

use crate::engine::error::ApiError;
use crate::engine::types::{
    now_unix_ms, PartialDownloadEntry, PartialDownloadKind, ReceiveStatus, ResumeAreaReport,
};
use crate::engine::Engine;

/// Display-only staleness threshold: 7 days without activity.
const STALE_AFTER_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Sum the on-disk bytes of every store file belonging to each hash
/// (`{hash}.data`, `{hash}.obao4`, `{hash}.sizes4`, `{hash}.bitfield`, …).
async fn disk_bytes_by_hash(engine: &Engine) -> HashMap<String, u64> {
    let data_dir = engine.paths.blobs().join("data");
    let mut sizes: HashMap<String, u64> = HashMap::new();
    let Ok(mut entries) = tokio::fs::read_dir(&data_dir).await else {
        return sizes;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        // File names are `<64-hex-chars>.<ext>`.
        let Some((hash_hex, _ext)) = name.split_once('.') else {
            continue;
        };
        if hash_hex.len() != 64 {
            continue;
        }
        let len = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
        *sizes.entry(hash_hex.to_string()).or_default() += len;
    }
    sizes
}

pub async fn list_partial_downloads(engine: &Arc<Engine>) -> Result<ResumeAreaReport, ApiError> {
    let disk = disk_bytes_by_hash(engine).await;

    // Snapshot records without holding the lock across awaits.
    let (receives, send_hashes): (Vec<_>, HashSet<String>) = {
        let reg = engine.registry.lock().unwrap();
        (
            reg.receives
                .values()
                .map(|e| {
                    (
                        e.record.id.clone(),
                        e.record.hash.clone(),
                        e.record.file_name.clone(),
                        e.record.size,
                        e.record.status,
                        e.record.last_activity_ms,
                    )
                })
                .collect(),
            reg.sends.values().map(|t| t.record.hash.clone()).collect(),
        )
    };

    let now = now_unix_ms();
    let mut entries = Vec::new();
    let mut accounted: HashSet<String> = HashSet::new();

    for (id, hash_hex, file_name, size, status, last_activity) in receives {
        accounted.insert(hash_hex.clone());
        // Hash::from_str panics on wrong-length input; length-guard first.
        let parsed = (hash_hex.len() == 64)
            .then(|| hash_hex.parse::<Hash>().ok())
            .flatten();
        let local_bytes = match parsed {
            Some(hash) => engine
                .store
                .remote()
                .local(iroh_blobs::HashAndFormat::raw(hash))
                .await
                .map(|l| l.local_bytes())
                .unwrap_or(0),
            None => 0,
        };
        let resumable = !matches!(
            status,
            ReceiveStatus::Failed | ReceiveStatus::NoLongerResumable
        );
        entries.push(PartialDownloadEntry {
            entry_id: id,
            kind: PartialDownloadKind::Tracked,
            file_name: Some(file_name),
            local_bytes,
            total_bytes: Some(size),
            disk_bytes: disk.get(&hash_hex).copied().unwrap_or(0),
            resumable,
            stale: now.saturating_sub(last_activity) > STALE_AFTER_MS,
            status: Some(status),
            last_activity_ms: Some(last_activity),
        });
    }

    // Anything on disk not owned by a live receive record or a send (sends
    // keep outboards in the store) is an orphan — possible only from
    // crashes before the record-first ordering rule, or external tampering.
    for (hash_hex, bytes) in &disk {
        if accounted.contains(hash_hex) || send_hashes.contains(hash_hex) {
            continue;
        }
        entries.push(PartialDownloadEntry {
            entry_id: format!("orphan-{hash_hex}"),
            kind: PartialDownloadKind::Orphan,
            file_name: None,
            local_bytes: 0,
            total_bytes: None,
            disk_bytes: *bytes,
            resumable: false,
            stale: true,
            status: None,
            last_activity_ms: None,
        });
    }

    let total_disk_bytes = entries.iter().map(|e| e.disk_bytes).sum();
    Ok(ResumeAreaReport {
        total_disk_bytes,
        entries,
    })
}

/// Explicit user cleanup (ADR 0020). Tracked entries are cancelled and
/// their records removed; bytes are reclaimed by the next GC sweep. Orphans
/// are unprotected by construction and are likewise swept.
pub async fn cleanup_partial_download(
    engine: &Arc<Engine>,
    entry_id: String,
) -> Result<ResumeAreaReport, ApiError> {
    if let Some(hash_hex) = entry_id.strip_prefix("orphan-") {
        // Orphans hold no tag and are absent from the protect set; ensure no
        // stray tag survived a crash, then let the sweep reclaim the bytes.
        // (Length-guard: Hash::from_str panics on wrong-length input.)
        if let Some(hash) = (hash_hex.len() == 64)
            .then(|| hash_hex.parse::<Hash>().ok())
            .flatten()
        {
            let tags = engine
                .store
                .tags()
                .list()
                .await
                .map_err(|e| ApiError::internal(format!("listing tags failed: {e}")))?;
            // tags().list returns a stream of TagInfo.
            let mut stream = tags;
            while let Some(Ok(tag)) = n0_future::StreamExt::next(&mut stream).await {
                if tag.hash == hash {
                    let _ = engine.store.tags().delete(tag.name.clone()).await;
                }
            }
        }
    } else {
        // Tracked entry: cancel-if-active then drop the record + tag.
        let exists = {
            let reg = engine.registry.lock().unwrap();
            reg.receives.contains_key(&entry_id)
        };
        if !exists {
            return Err(ApiError::not_found("partial download not found"));
        }
        crate::engine::recv::cancel_receive_transfer(engine, &entry_id).await?;
    }
    list_partial_downloads(engine).await
}
