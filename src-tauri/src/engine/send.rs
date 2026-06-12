//! Sender-side Transfer lifecycle: import (without copying), ticket
//! issuance, pause/resume, Cancellation, content-change detection, and
//! moved-file reselect (ADRs 0003/0004/0005/0015/0018/0019).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use iroh_blobs::api::blobs::{AddPathOptions, AddProgressItem, ImportMode};
use iroh_blobs::ticket::BlobTicket;
use iroh_blobs::{BlobFormat, Hash};
use iroh_tickets::Ticket as _;
use n0_future::StreamExt;

use crate::engine::error::{ApiError, ErrorCode};
use crate::engine::gate;
use crate::engine::persist::SendRecord;
use crate::engine::ticket::VegamTicket;
use crate::engine::types::{now_unix_secs, SendStatus, SendTransferInfo};
use crate::engine::{Engine, SendTransfer};

/// Stat snapshot used as the content-drift heuristic baseline. A mismatch
/// only makes the transfer ContentSuspect; the authoritative check is a
/// re-hash (ADR 0005: Content Identity, not mtime).
fn stat_source(path: &Path) -> std::io::Result<(u64, u64)> {
    let meta = std::fs::metadata(path)?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok((meta.len(), mtime_ms))
}

/// On restart: re-check the source file before re-arming a send record.
pub fn refresh_send_status_from_disk(record: &mut SendRecord) {
    if record.status.is_terminal() {
        return;
    }
    match stat_source(&record.source_path) {
        Err(_) => record.status = SendStatus::SourceMissing,
        Ok((len, mtime_ms)) => {
            if len != record.source_len || mtime_ms != record.source_mtime_unix_ms {
                record.status = SendStatus::ContentSuspect;
            } else if record.status == SendStatus::Available
                && now_unix_secs()
                    >= record
                        .issued_at
                        .saturating_add(crate::engine::ticket::TICKET_TTL_SECS)
            {
                record.status = SendStatus::Expired;
            } else if record.status == SendStatus::Importing
                || record.status == SendStatus::ContentSuspect
            {
                // An interrupted import or unfinished re-hash restarts as a
                // suspect; the sweeper schedules the re-hash.
                record.status = SendStatus::ContentSuspect;
            }
        }
    }
}

pub async fn create_send_transfer(
    engine: &Arc<Engine>,
    file_path: String,
) -> Result<SendTransferInfo, ApiError> {
    let source_path = std::fs::canonicalize(PathBuf::from(&file_path))
        .map_err(|_| ApiError::new(ErrorCode::SourceMissing, "file not found"))?;
    if !source_path.is_file() {
        return Err(ApiError::new(
            ErrorCode::SourceMissing,
            "only single files can be sent in v1",
        ));
    }
    let (source_len, source_mtime_unix_ms) =
        stat_source(&source_path).map_err(|_| ApiError::io("cannot read file metadata"))?;
    let file_name = source_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    let id = uuid::Uuid::new_v4().to_string();
    let record = SendRecord {
        id: id.clone(),
        source_path: source_path.clone(),
        file_name,
        size: source_len,
        hash: String::new(), // known after import
        issued_at: 0,
        ticket: String::new(),
        status: SendStatus::Importing,
        started_receivers: BTreeSet::new(),
        source_len,
        source_mtime_unix_ms,
    };

    // Ordering rule: record exists before any blob is imported/tagged.
    let info = {
        let mut reg = engine.registry.lock().unwrap();
        reg.sends.insert(
            id.clone(),
            SendTransfer {
                record,
                hash: Hash::new([0u8; 32]),
                import_progress: Some(0.0),
                error: None,
            },
        );
        let t = reg.sends.get(&id).expect("just inserted");
        reg.send_info(t)
    };
    engine.persist()?;
    engine.emit_send(&id);
    crate::engine::telemetry::capture(
        engine,
        crate::engine::telemetry::Event::SendTransferCreated,
        Some(source_len),
    );

    let task_engine = engine.clone();
    let task_id = id.clone();
    tokio::spawn(async move {
        if let Err(e) = run_import(&task_engine, &task_id, source_path).await {
            tracing::warn!("send import failed: {e}");
            {
                let mut reg = task_engine.registry.lock().unwrap();
                if let Some(t) = reg.sends.get_mut(&task_id) {
                    t.record.status = SendStatus::SourceMissing;
                    t.import_progress = None;
                    t.error = Some(e.to_string());
                }
            }
            let _ = task_engine.persist();
            task_engine.emit_send(&task_id);
        }
    });

    Ok(info)
}

/// Outcome of claiming a hash for a freshly imported send transfer.
enum HashClaim {
    /// This transfer now owns the hash; the previous stale owner (if any)
    /// must have its tag deleted.
    Claimed { stale_tag_id: Option<String> },
    /// Another live transfer already serves this hash; this one is a
    /// duplicate and was removed.
    Duplicate { existing_id: String },
    /// The record vanished mid-import (cancelled); nothing to do.
    Gone,
}

/// Import the source (TryReference: hashing only, no copy), persist a named
/// tag, mint the Transfer Ticket, and flip the record to Available.
async fn run_import(engine: &Arc<Engine>, id: &str, path: PathBuf) -> anyhow::Result<()> {
    let (hash, size) = import_path(engine, id, &path, true).await?;

    // Decide ownership of this hash in ONE lock scope so concurrent imports
    // of the same content cannot interleave (D3 re-share policy: dedup onto
    // a live transfer; replace stale records with fresh issuance).
    let claim = {
        let mut reg = engine.registry.lock().unwrap();
        if !reg.sends.contains_key(id) {
            HashClaim::Gone
        } else {
            match reg.sends_by_hash.get(&hash).cloned() {
                Some(other_id) if other_id != id => {
                    let other_live = reg
                        .sends
                        .get(&other_id)
                        .map(|t| {
                            matches!(
                                t.record.status,
                                SendStatus::Available | SendStatus::Paused | SendStatus::Importing
                            )
                        })
                        .unwrap_or(false);
                    if other_live {
                        // Mark the duplicate terminal so the UI clears it.
                        if let Some(t) = reg.sends.get_mut(id) {
                            t.record.status = SendStatus::Cancelled;
                            t.import_progress = None;
                        }
                        HashClaim::Duplicate {
                            existing_id: other_id,
                        }
                    } else {
                        // Stale owner (expired/changed/missing): replace it.
                        reg.sends.remove(&other_id);
                        reg.sends_by_hash.insert(hash, id.to_string());
                        HashClaim::Claimed {
                            stale_tag_id: Some(other_id),
                        }
                    }
                }
                Some(_) => HashClaim::Claimed { stale_tag_id: None },
                None => {
                    reg.sends_by_hash.insert(hash, id.to_string());
                    HashClaim::Claimed { stale_tag_id: None }
                }
            }
        }
    };

    let stale_tag_id = match claim {
        HashClaim::Gone => {
            tracing::debug!("send transfer cancelled during import");
            return Ok(());
        }
        HashClaim::Duplicate { existing_id } => {
            // Emit the terminal state for the duplicate, then drop it.
            engine.emit_send(id);
            {
                let mut reg = engine.registry.lock().unwrap();
                reg.sends.remove(id);
            }
            engine.persist()?;
            tracing::debug!("send deduplicated onto existing transfer");
            engine.emit_send(&existing_id);
            return Ok(());
        }
        HashClaim::Claimed { stale_tag_id } => stale_tag_id,
    };
    if let Some(stale_id) = stale_tag_id {
        let _ = engine.store.tags().delete(format!("send/{stale_id}")).await;
    }

    engine
        .store
        .tags()
        .set(format!("send/{id}"), hash)
        .await
        .map_err(|e| anyhow::anyhow!("tagging blob failed: {e}"))?;

    // If the transfer was cancelled while we were tagging, undo the tag.
    let cancelled_mid_flight = {
        let reg = engine.registry.lock().unwrap();
        !reg.sends.contains_key(id)
    };
    if cancelled_mid_flight {
        let _ = engine.store.tags().delete(format!("send/{id}")).await;
        let mut reg = engine.registry.lock().unwrap();
        if reg
            .sends_by_hash
            .get(&hash)
            .map(|v| v == id)
            .unwrap_or(false)
        {
            reg.sends_by_hash.remove(&hash);
        }
        return Ok(());
    }

    // Wait for relay registration so the ticket carries a relay URL.
    if !matches!(engine.tuning.relay_mode, iroh::RelayMode::Disabled) {
        let _ = tokio::time::timeout(Duration::from_secs(30), engine.endpoint.online()).await;
    }
    // A ticket minted before the endpoint knows any of its addresses is
    // unconnectable; wait until at least one (relay or direct) shows up.
    let addr_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while engine.endpoint.addr().addrs.is_empty() {
        if tokio::time::Instant::now() >= addr_deadline {
            tracing::warn!("endpoint has no addresses; ticket may be unconnectable");
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let (file_name, issued_at) = {
        let reg = engine.registry.lock().unwrap();
        let t = reg
            .sends
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("send transfer disappeared during import"))?;
        (t.record.file_name.clone(), now_unix_secs())
    };
    let ticket = VegamTicket {
        blob: BlobTicket::new(engine.endpoint.addr(), hash, BlobFormat::Raw),
        file_name,
        size,
        issued_at,
    };
    let ticket_str = ticket.encode_string();

    {
        let mut reg = engine.registry.lock().unwrap();
        if let Some(t) = reg.sends.get_mut(id) {
            t.hash = hash;
            t.record.hash = hash.to_string();
            t.record.size = size;
            t.record.issued_at = issued_at;
            t.record.ticket = ticket_str;
            t.record.status = SendStatus::Available;
            t.import_progress = None;
        }
    }
    engine.persist()?;
    engine.emit_send(id);
    Ok(())
}

/// Run a TryReference import of `path`. Returns (hash, size). The returned
/// blob is protected only by the temp tag until the caller tags it.
/// `report_progress` drives the transfer's import_progress; background
/// re-hashes pass false so a serving transfer never shows bogus progress.
async fn import_path(
    engine: &Arc<Engine>,
    id: &str,
    path: &Path,
    report_progress: bool,
) -> anyhow::Result<(Hash, u64)> {
    let mut progress = engine
        .store
        .add_path_with_opts(AddPathOptions {
            path: path.to_path_buf(),
            mode: ImportMode::TryReference,
            format: BlobFormat::Raw,
        })
        .stream()
        .await;

    let mut size = 0u64;
    let mut last_emit = std::time::Instant::now();
    let temp_tag = loop {
        match progress.next().await {
            Some(AddProgressItem::Size(s)) => size = s,
            Some(AddProgressItem::CopyProgress(_)) | Some(AddProgressItem::CopyDone) => {}
            Some(AddProgressItem::OutboardProgress(offset)) => {
                if report_progress && size > 0 && last_emit.elapsed() >= Duration::from_millis(250)
                {
                    last_emit = std::time::Instant::now();
                    {
                        let mut reg = engine.registry.lock().unwrap();
                        if let Some(t) = reg.sends.get_mut(id) {
                            t.import_progress = Some(offset as f64 / size as f64);
                        }
                    }
                    engine.emit_send(id);
                }
            }
            Some(AddProgressItem::Done(tag)) => break tag,
            Some(AddProgressItem::Error(e)) => anyhow::bail!("import failed: {e}"),
            None => anyhow::bail!("import stream ended unexpectedly"),
        }
    };
    let hash = temp_tag.hash();
    // The named tag (set by the caller) takes over GC protection; the temp
    // tag may drop after that. For the re-hash path the caller drops it
    // immediately, leaving the duplicate blob unprotected on purpose.
    drop(temp_tag);
    Ok((hash, size))
}

pub fn pause_send_transfer(engine: &Arc<Engine>, id: &str) -> Result<SendTransferInfo, ApiError> {
    let hash = {
        let mut reg = engine.registry.lock().unwrap();
        let t = reg
            .sends
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found("send transfer not found"))?;
        if !matches!(t.record.status, SendStatus::Available | SendStatus::Expired) {
            return Err(ApiError::new(
                ErrorCode::Rejected,
                "transfer cannot be paused in its current state",
            ));
        }
        t.record.status = SendStatus::Paused;
        t.hash
    };
    // Abort in-flight uploads; receivers see a transient error and retry
    // slowly until resume (ADR 0018).
    gate::abort_requests_for_hash(engine, &hash);
    engine.persist()?;
    engine.emit_send(id);
    current_send_info(engine, id)
}

pub fn resume_send_transfer(engine: &Arc<Engine>, id: &str) -> Result<SendTransferInfo, ApiError> {
    {
        let mut reg = engine.registry.lock().unwrap();
        let t = reg
            .sends
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found("send transfer not found"))?;
        if t.record.status != SendStatus::Paused {
            return Err(ApiError::new(ErrorCode::Rejected, "transfer is not paused"));
        }
        let expired = now_unix_secs()
            >= t.record
                .issued_at
                .saturating_add(crate::engine::ticket::TICKET_TTL_SECS);
        t.record.status = if expired {
            SendStatus::Expired
        } else {
            SendStatus::Available
        };
    }
    engine.persist()?;
    engine.emit_send(id);
    current_send_info(engine, id)
}

/// Sender Cancellation stops availability without touching the source file
/// (ADR 0019).
pub async fn cancel_send_transfer(engine: &Arc<Engine>, id: &str) -> Result<(), ApiError> {
    let hash = {
        let mut reg = engine.registry.lock().unwrap();
        let t = reg
            .sends
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found("send transfer not found"))?;
        t.record.status = SendStatus::Cancelled;
        t.hash
    };
    gate::abort_requests_for_hash(engine, &hash);

    // Emit the terminal state before removing the record.
    engine.emit_send(id);

    let _ = engine.store.tags().delete(format!("send/{id}")).await;
    {
        let mut reg = engine.registry.lock().unwrap();
        reg.sends.remove(id);
        if reg
            .sends_by_hash
            .get(&hash)
            .map(|v| v == id)
            .unwrap_or(false)
        {
            reg.sends_by_hash.remove(&hash);
        }
    }
    engine.persist()?;
    Ok(())
}

/// ADR 0005: if the Sender moved the file, ask for a reselect and continue
/// only if Content Identity matches.
pub async fn reselect_send_source(
    engine: &Arc<Engine>,
    id: &str,
    file_path: String,
) -> Result<SendTransferInfo, ApiError> {
    let new_path = std::fs::canonicalize(PathBuf::from(&file_path))
        .map_err(|_| ApiError::new(ErrorCode::SourceMissing, "file not found"))?;
    let expected_hash = {
        let mut reg = engine.registry.lock().unwrap();
        let t = reg
            .sends
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found("send transfer not found"))?;
        if !matches!(
            t.record.status,
            SendStatus::SourceMissing | SendStatus::ContentSuspect | SendStatus::ContentChanged
        ) {
            return Err(ApiError::new(
                ErrorCode::Rejected,
                "transfer does not need a reselect",
            ));
        }
        t.record.status = SendStatus::Importing;
        t.import_progress = Some(0.0);
        t.hash
    };
    engine.emit_send(id);

    let (hash, _) = import_path(engine, id, &new_path, true)
        .await
        .map_err(|e| {
            restore_after_failed_reselect(engine, id);
            ApiError::io(format!("could not read the selected file: {e}"))
        })?;

    if hash != expected_hash {
        restore_after_failed_reselect(engine, id);
        engine.emit_send(id);
        return Err(ApiError::new(
            ErrorCode::ContentMismatch,
            "the selected file has different content; create a new transfer ticket",
        ));
    }

    let (len, mtime) = stat_source(&new_path).map_err(|_| ApiError::io("cannot stat file"))?;
    {
        let mut reg = engine.registry.lock().unwrap();
        if let Some(t) = reg.sends.get_mut(id) {
            t.record.source_path = new_path;
            t.record.source_len = len;
            t.record.source_mtime_unix_ms = mtime;
            t.import_progress = None;
            let expired = now_unix_secs()
                >= t.record
                    .issued_at
                    .saturating_add(crate::engine::ticket::TICKET_TTL_SECS);
            t.record.status = if expired {
                SendStatus::Expired
            } else {
                SendStatus::Available
            };
        }
    }
    engine.persist()?;
    engine.emit_send(id);
    current_send_info(engine, id)
}

fn restore_after_failed_reselect(engine: &Arc<Engine>, id: &str) {
    let mut reg = engine.registry.lock().unwrap();
    if let Some(t) = reg.sends.get_mut(id) {
        t.record.status = SendStatus::SourceMissing;
        t.import_progress = None;
    }
}

fn current_send_info(engine: &Arc<Engine>, id: &str) -> Result<SendTransferInfo, ApiError> {
    let reg = engine.registry.lock().unwrap();
    reg.sends
        .get(id)
        .map(|t| reg.send_info(t))
        .ok_or_else(|| ApiError::not_found("send transfer not found"))
}

/// 30s sweeper body: stat each live send source; drift → ContentSuspect and
/// a background re-hash that either restores the previous state or confirms
/// ContentChanged (ADR 0005: re-hash is authoritative, mtime is not).
pub async fn sweep_send_sources(engine: &Arc<Engine>) {
    let candidates: Vec<(String, PathBuf, u64, u64, SendStatus)> = {
        let reg = engine.registry.lock().unwrap();
        reg.sends
            .values()
            .filter(|t| {
                matches!(
                    t.record.status,
                    SendStatus::Available
                        | SendStatus::Paused
                        | SendStatus::Expired
                        | SendStatus::ContentSuspect
                        | SendStatus::SourceMissing
                )
            })
            .map(|t| {
                (
                    t.record.id.clone(),
                    t.record.source_path.clone(),
                    t.record.source_len,
                    t.record.source_mtime_unix_ms,
                    t.record.status,
                )
            })
            .collect()
    };

    for (id, path, len, mtime, status) in candidates {
        match stat_source(&path) {
            Err(_) => {
                if status != SendStatus::SourceMissing {
                    set_send_status(engine, &id, SendStatus::SourceMissing);
                }
            }
            Ok((cur_len, cur_mtime)) => {
                if status == SendStatus::SourceMissing {
                    // File reappeared at the same path: treat as suspect and
                    // verify by re-hash before serving again.
                    set_send_status(engine, &id, SendStatus::ContentSuspect);
                    spawn_rehash(engine.clone(), id, path);
                } else if cur_len != len || cur_mtime != mtime {
                    if status != SendStatus::ContentSuspect {
                        set_send_status(engine, &id, SendStatus::ContentSuspect);
                    }
                    spawn_rehash(engine.clone(), id, path);
                } else if status == SendStatus::ContentSuspect {
                    // Stats match the baseline again but the suspect flag is
                    // set (e.g. interrupted re-hash after restart): verify.
                    spawn_rehash(engine.clone(), id, path);
                }
            }
        }
    }
}

fn set_send_status(engine: &Arc<Engine>, id: &str, status: SendStatus) {
    {
        let mut reg = engine.registry.lock().unwrap();
        if let Some(t) = reg.sends.get_mut(id) {
            t.record.status = status;
        }
    }
    if let Err(e) = engine.persist() {
        tracing::warn!("persisting send status failed: {e}");
    }
    engine.emit_send(id);
}

fn spawn_rehash(engine: Arc<Engine>, id: String, path: PathBuf) {
    // One re-hash at a time per transfer.
    {
        let mut reg = engine.registry.lock().unwrap();
        if !reg.rehashing.insert(id.clone()) {
            return;
        }
    }
    tokio::spawn(async move {
        let expected = {
            let reg = engine.registry.lock().unwrap();
            reg.sends.get(&id).map(|t| t.hash)
        };
        let Some(expected) = expected else {
            engine.registry.lock().unwrap().rehashing.remove(&id);
            return;
        };

        let result = import_path(&engine, &id, &path, false).await;
        let new_status = match result {
            Err(_) => SendStatus::SourceMissing,
            Ok((hash, _)) if hash == expected => {
                // Content unchanged (mtime-only churn): restore and update
                // the stat baseline.
                let stat = stat_source(&path).ok();
                let mut reg = engine.registry.lock().unwrap();
                if let Some(t) = reg.sends.get_mut(&id) {
                    if let Some((len, mtime)) = stat {
                        t.record.source_len = len;
                        t.record.source_mtime_unix_ms = mtime;
                    }
                    let expired = now_unix_secs()
                        >= t.record
                            .issued_at
                            .saturating_add(crate::engine::ticket::TICKET_TTL_SECS);
                    t.record.status = if expired {
                        SendStatus::Expired
                    } else {
                        SendStatus::Available
                    };
                }
                drop(reg);
                engine.registry.lock().unwrap().rehashing.remove(&id);
                if let Err(e) = engine.persist() {
                    tracing::warn!("persisting re-hash result failed: {e}");
                }
                engine.emit_send(&id);
                return;
            }
            Ok(_) => SendStatus::ContentChanged,
        };

        let hash = {
            let mut reg = engine.registry.lock().unwrap();
            reg.rehashing.remove(&id);
            if let Some(t) = reg.sends.get_mut(&id) {
                t.record.status = new_status;
                Some(t.hash)
            } else {
                None
            }
        };
        if new_status == SendStatus::ContentChanged {
            if let Some(h) = hash {
                gate::abort_requests_for_hash(&engine, &h);
            }
        }
        if let Err(e) = engine.persist() {
            tracing::warn!("persisting re-hash result failed: {e}");
        }
        engine.emit_send(&id);
    });
}
