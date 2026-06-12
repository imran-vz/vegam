//! Receiver-side Transfer lifecycle: resumable download into the managed
//! resume area, manual pause/resume, Cancellation, retry with backoff, and
//! verified atomic export to the user's destination
//! (ADRs 0005/0018/0019/0020).

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh_blobs::api::blobs::{ExportMode, ExportOptions, ExportProgressItem};
use iroh_blobs::api::remote::GetProgressItem;
use iroh_blobs::Hash;
use n0_future::StreamExt;
use tokio::sync::watch;

use crate::engine::error::{ApiError, ErrorCode};
use crate::engine::events::EngineEvent;
use crate::engine::persist::ReceiveRecord;
use crate::engine::ticket::VegamTicket;
use crate::engine::types::{
    now_unix_ms, ConnectionKind, ReceiveProgress, ReceiveStatus, ReceiveTransferInfo,
};
use crate::engine::{Engine, ReceiveEntry, RecvControl};

/// Wire error codes the Sender's gate uses (iroh-blobs protocol constants).
const ERR_PERMISSION: u64 = 1; // terminal: cancelled/expired/content-changed
const ERR_LIMIT: u64 = 2; // transient: paused/source-missing/importing
const ERR_INTERNAL: u64 = 3; // provider failure; repeated ⇒ probable content change

pub async fn create_receive_transfer(
    engine: &Arc<Engine>,
    ticket_str: String,
    destination_path: String,
) -> Result<ReceiveTransferInfo, ApiError> {
    let ticket = VegamTicket::from_str(&ticket_str)
        .map_err(|e| ApiError::ticket_invalid(format!("not a valid Vegam ticket: {e}")))?;
    let destination = PathBuf::from(&destination_path);
    if destination
        .parent()
        .map(|p| p.as_os_str().is_empty())
        .unwrap_or(true)
    {
        return Err(ApiError::io("destination must be an absolute file path"));
    }
    let hash = ticket.blob.hash();

    {
        let reg = engine.registry.lock().unwrap();
        let duplicate = reg
            .receives
            .values()
            .any(|e| e.record.hash == hash.to_string() && e.record.status.is_active());
        if duplicate {
            return Err(ApiError::new(
                ErrorCode::AlreadyExists,
                "this file is already being received",
            ));
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let record = ReceiveRecord {
        id: id.clone(),
        ticket: ticket_str.trim().to_string(),
        hash: hash.to_string(),
        file_name: ticket.file_name.clone(),
        size: ticket.size,
        issued_at: ticket.issued_at,
        destination,
        status: ReceiveStatus::Connecting,
        last_activity_ms: now_unix_ms(),
    };

    // Ordering rule: record before tag.
    let info = {
        let mut reg = engine.registry.lock().unwrap();
        let (control, _) = watch::channel(RecvControl::Run);
        reg.receives.insert(
            id.clone(),
            ReceiveEntry {
                record,
                control,
                local_bytes: 0,
                connection_kind: None,
                error_code: None,
                error: None,
            },
        );
        let e = reg.receives.get(&id).expect("just inserted");
        reg.receive_info(e)
    };
    engine.persist()?;
    engine
        .store
        .tags()
        .set(format!("recv/{id}"), hash)
        .await
        .map_err(|e| ApiError::internal(format!("tagging partial download failed: {e}")))?;

    spawn_receive_task(engine.clone(), id.clone(), hash);
    engine.emit_receive(&id);
    Ok(info)
}

pub fn pause_receive_transfer(
    engine: &Arc<Engine>,
    id: &str,
) -> Result<ReceiveTransferInfo, ApiError> {
    {
        let reg = engine.registry.lock().unwrap();
        let entry = reg
            .receives
            .get(id)
            .ok_or_else(|| ApiError::not_found("receive transfer not found"))?;
        if !entry.record.status.is_active() {
            return Err(ApiError::new(
                ErrorCode::Rejected,
                "transfer cannot be paused in its current state",
            ));
        }
        let _ = entry.control.send(RecvControl::Pause);
    }
    current_receive_info(engine, id)
}

pub fn resume_receive_transfer(
    engine: &Arc<Engine>,
    id: &str,
) -> Result<ReceiveTransferInfo, ApiError> {
    let respawn = {
        let mut reg = engine.registry.lock().unwrap();
        let entry = reg
            .receives
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found("receive transfer not found"))?;
        match entry.record.status {
            // A Paused entry restored from disk has no live task (its watch
            // receiver count is 0) — sending Run would go nowhere. Respawn
            // the task in that case.
            ReceiveStatus::Paused if entry.control.receiver_count() > 0 => {
                let _ = entry.control.send(RecvControl::Run);
                None
            }
            // Paused-with-no-task, or a NoLongerResumable/Failed transfer
            // retried by an explicit user resume (e.g. the Sender reselected
            // the file after we were rejected).
            ReceiveStatus::Paused | ReceiveStatus::Failed | ReceiveStatus::NoLongerResumable => {
                entry.record.status = ReceiveStatus::Connecting;
                entry.error = None;
                entry.error_code = None;
                let (control, _) = watch::channel(RecvControl::Run);
                entry.control = control;
                // Length-guard: Hash::from_str panics on wrong-length input.
                let parsed = (entry.record.hash.len() == 64)
                    .then(|| entry.record.hash.parse::<Hash>().ok())
                    .flatten();
                Some(parsed.ok_or_else(|| ApiError::internal("stored hash is invalid"))?)
            }
            _ => return Err(ApiError::new(ErrorCode::Rejected, "transfer is not paused")),
        }
    };
    if let Some(hash) = respawn {
        engine.persist()?;
        spawn_receive_task(engine.clone(), id.to_string(), hash);
    }
    engine.emit_receive(id);
    current_receive_info(engine, id)
}

/// Receiver Cancellation deletes the Partial Download state (ADR 0019).
pub async fn cancel_receive_transfer(engine: &Arc<Engine>, id: &str) -> Result<(), ApiError> {
    {
        let mut reg = engine.registry.lock().unwrap();
        let entry = reg
            .receives
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found("receive transfer not found"))?;
        entry.record.status = ReceiveStatus::Cancelled;
        let _ = entry.control.send(RecvControl::Cancel);
    }
    engine.emit_receive(id);

    let _ = engine.store.tags().delete(format!("recv/{id}")).await;
    {
        let mut reg = engine.registry.lock().unwrap();
        reg.receives.remove(id);
    }
    engine.persist()?;
    Ok(())
}

fn current_receive_info(engine: &Arc<Engine>, id: &str) -> Result<ReceiveTransferInfo, ApiError> {
    let reg = engine.registry.lock().unwrap();
    reg.receives
        .get(id)
        .map(|e| reg.receive_info(e))
        .ok_or_else(|| ApiError::not_found("receive transfer not found"))
}

fn set_receive_state(
    engine: &Arc<Engine>,
    id: &str,
    status: ReceiveStatus,
    error_code: Option<ErrorCode>,
    error: Option<String>,
) {
    {
        let mut reg = engine.registry.lock().unwrap();
        if let Some(entry) = reg.receives.get_mut(id) {
            entry.record.status = status;
            entry.record.last_activity_ms = now_unix_ms();
            entry.error_code = error_code;
            entry.error = error;
        }
    }
    if let Err(e) = engine.persist() {
        tracing::warn!("persisting receive status failed: {e}");
    }
    engine.emit_receive(id);
}

pub fn spawn_receive_task(engine: Arc<Engine>, id: String, hash: Hash) {
    tokio::spawn(async move {
        if let Err(e) = run_receive(&engine, &id, hash).await {
            tracing::debug!("receive task ended with error: {e}");
            // Never leave a record stranded in an active state with no task
            // behind it — flip to Failed so the user can resume or clean up.
            let stuck_active = {
                let reg = engine.registry.lock().unwrap();
                reg.receives
                    .get(&id)
                    .map(|entry| entry.record.status.is_active())
                    .unwrap_or(false)
            };
            if stuck_active {
                set_receive_state(
                    &engine,
                    &id,
                    ReceiveStatus::Failed,
                    Some(ErrorCode::Internal),
                    Some(format!("transfer failed: {e}")),
                );
            }
        }
    });
}

async fn run_receive(engine: &Arc<Engine>, id: &str, hash: Hash) -> anyhow::Result<()> {
    let haf = iroh_blobs::HashAndFormat::raw(hash);
    let ticket = {
        let reg = engine.registry.lock().unwrap();
        let Some(entry) = reg.receives.get(id) else {
            return Ok(());
        };
        VegamTicket::from_str(&entry.record.ticket)
            .map_err(|e| anyhow::anyhow!("stored ticket unparseable: {e}"))?
    };
    let mut control = {
        let reg = engine.registry.lock().unwrap();
        match reg.receives.get(id) {
            Some(e) => e.control.subscribe(),
            None => return Ok(()),
        }
    };

    let mut backoff = Duration::from_secs(1);
    let mut consecutive_internal_resets: u32 = 0;

    'outer: loop {
        // Honor control before any network work.
        let current_control = *control.borrow();
        match current_control {
            RecvControl::Cancel => return Ok(()),
            RecvControl::Pause => {
                let local = engine.store.remote().local(haf).await?;
                update_local_bytes(engine, id, local.local_bytes());
                set_receive_state(engine, id, ReceiveStatus::Paused, None, None);
                loop {
                    if control.changed().await.is_err() {
                        return Ok(());
                    }
                    match *control.borrow() {
                        RecvControl::Run => break,
                        RecvControl::Cancel => return Ok(()),
                        RecvControl::Pause => {}
                    }
                }
                set_receive_state(engine, id, ReceiveStatus::Connecting, None, None);
            }
            RecvControl::Run => {}
        }

        let local = engine.store.remote().local(haf).await?;
        update_local_bytes(engine, id, local.local_bytes());
        if local.is_complete() {
            break 'outer;
        }

        // Connect to the Sender.
        let conn = tokio::select! {
            biased;
            changed = control.changed() => {
                if changed.is_err() { return Ok(()); }
                continue 'outer;
            }
            conn = engine
                .endpoint
                .connect(ticket.blob.addr().clone(), iroh_blobs::protocol::ALPN) => {
                match conn {
                    Ok(c) => c,
                    Err(e) => {
                        set_receive_state(
                            engine, id,
                            ReceiveStatus::StalledRetrying,
                            Some(ErrorCode::Network),
                            Some(format!("connect failed: {e}")),
                        );
                        tokio::select! {
                            _ = tokio::time::sleep(backoff) => {}
                            _ = control.changed() => {}
                        }
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                        continue 'outer;
                    }
                }
            }
        };
        let kind = connection_kind(&conn);
        {
            let mut reg = engine.registry.lock().unwrap();
            if let Some(entry) = reg.receives.get_mut(id) {
                entry.connection_kind = Some(kind);
            }
        }
        set_receive_state(engine, id, ReceiveStatus::Downloading, None, None);

        let base = local.local_bytes();
        let total = ticket.size.max(base);
        let request = local.missing();
        let get = engine.store.remote().execute_get(conn.clone(), request);
        let mut stream = get.stream();
        let mut last_emit = Instant::now();
        let mut last_bytes = base;
        let mut last_speed_at = Instant::now();
        let mut session_error: Option<String> = None;
        let mut session_error_code: Option<u64> = None;
        let mut paused = false;

        loop {
            tokio::select! {
                item = stream.next() => match item {
                    Some(GetProgressItem::Progress(offset)) => {
                        consecutive_internal_resets = 0;
                        backoff = Duration::from_secs(1);
                        let bytes = base + offset;
                        if last_emit.elapsed() >= Duration::from_millis(250) {
                            let dt = last_speed_at.elapsed().as_secs_f64();
                            let speed = if dt > 0.0 {
                                ((bytes.saturating_sub(last_bytes)) as f64 / dt) as u64
                            } else {
                                0
                            };
                            last_bytes = bytes;
                            last_speed_at = Instant::now();
                            last_emit = Instant::now();
                            update_local_bytes(engine, id, bytes);
                            let kind = connection_kind(&conn);
                            {
                                let mut reg = engine.registry.lock().unwrap();
                                if let Some(entry) = reg.receives.get_mut(id) {
                                    entry.connection_kind = Some(kind);
                                }
                            }
                            engine.emit(EngineEvent::ReceiveTransferProgress(ReceiveProgress {
                                id: id.to_string(),
                                local_bytes: bytes,
                                total_bytes: total,
                                speed_bps: speed,
                                connection_kind: Some(kind),
                            }));
                        }
                    }
                    Some(GetProgressItem::Done(_stats)) => {
                        break;
                    }
                    Some(GetProgressItem::Error(e)) => {
                        session_error_code = e.iroh_error_code().map(|c| c.into_inner());
                        session_error = Some(format!("{e}"));
                        break;
                    }
                    None => break,
                },
                changed = control.changed() => {
                    if changed.is_err() { return Ok(()); }
                    match *control.borrow() {
                        RecvControl::Pause => { paused = true; break; }
                        RecvControl::Cancel => { return Ok(()); }
                        RecvControl::Run => {}
                    }
                }
            }
        }
        // Dropping stream/connection cancels the session; partial data stays
        // in the FsStore (the managed resume area).
        drop(stream);
        conn.close(0u32.into(), b"session end");

        if paused {
            continue 'outer;
        }

        if let Some(message) = session_error {
            match session_error_code {
                Some(ERR_PERMISSION) => {
                    // Terminal: sender cancelled, ticket expired for new
                    // receivers, or content changed.
                    set_receive_state(
                        engine,
                        id,
                        ReceiveStatus::NoLongerResumable,
                        Some(ErrorCode::Rejected),
                        Some("the sender no longer makes this file available to you".to_string()),
                    );
                    return Ok(());
                }
                Some(ERR_LIMIT) => {
                    // Sender paused or in a transient state; retry slowly.
                    set_receive_state(
                        engine,
                        id,
                        ReceiveStatus::StalledRetrying,
                        Some(ErrorCode::Rejected),
                        Some("the sender is paused or temporarily unavailable".to_string()),
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(engine.tuning.stall_retry) => {}
                        _ = control.changed() => {}
                    }
                }
                Some(ERR_INTERNAL) => {
                    consecutive_internal_resets += 1;
                    let (code, text) = if consecutive_internal_resets >= 3 {
                        (
                            ErrorCode::ContentMismatch,
                            "the sender's file may have changed; waiting for the sender to resolve it".to_string(),
                        )
                    } else {
                        (ErrorCode::Network, message)
                    };
                    set_receive_state(
                        engine,
                        id,
                        ReceiveStatus::StalledRetrying,
                        Some(code),
                        Some(text),
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = control.changed() => {}
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                }
                _ => {
                    // Local verification failures are terminal; everything
                    // else is a network blip worth retrying.
                    if message.contains("decode") || message.contains("Decode") {
                        set_receive_state(
                            engine,
                            id,
                            ReceiveStatus::Failed,
                            Some(ErrorCode::ContentMismatch),
                            Some("downloaded data failed verification".to_string()),
                        );
                        return Ok(());
                    }
                    set_receive_state(
                        engine,
                        id,
                        ReceiveStatus::StalledRetrying,
                        Some(ErrorCode::Network),
                        Some(message),
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = control.changed() => {}
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                }
            }
        }
    }

    // Verified complete — only now does anything touch the destination
    // (ADR 0020).
    set_receive_state(engine, id, ReceiveStatus::Verifying, None, None);
    let local = engine.store.remote().local(haf).await?;
    anyhow::ensure!(local.is_complete(), "completion loop exited early");
    update_local_bytes(engine, id, local.local_bytes());
    set_receive_state(engine, id, ReceiveStatus::Exporting, None, None);

    let destination = {
        let reg = engine.registry.lock().unwrap();
        match reg.receives.get(id) {
            Some(e) => e.record.destination.clone(),
            None => return Ok(()),
        }
    };
    export_atomic(engine, hash, &destination, &mut control).await?;
    // A cancel that landed during the export wins: the destination was not
    // written (export_atomic checked before renaming) and the cancel path
    // owns the cleanup.
    if *control.borrow() == RecvControl::Cancel {
        return Ok(());
    }

    set_receive_state(engine, id, ReceiveStatus::Complete, None, None);
    let _ = engine.store.tags().delete(format!("recv/{id}")).await;
    {
        let mut reg = engine.registry.lock().unwrap();
        reg.receives.remove(id);
    }
    engine.persist()?;
    Ok(())
}

/// Export to `<dest>.vegampart` in the destination directory, then rename
/// into place — atomic on the same filesystem, so a crash mid-export never
/// leaves a corrupt-looking file at the destination (ADR 0020). The final
/// rename intentionally replaces an existing file: the user chose the exact
/// destination in a save dialog that already confirmed overwrites.
async fn export_atomic(
    engine: &Arc<Engine>,
    hash: Hash,
    destination: &std::path::Path,
    control: &mut watch::Receiver<RecvControl>,
) -> anyhow::Result<()> {
    let file_name = destination
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let temp = destination.with_file_name(format!("{file_name}.vegampart"));

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // A .vegampart left by a crashed earlier export is stale; replace it.
    let _ = tokio::fs::remove_file(&temp).await;

    let mut stream = engine
        .store
        .export_with_opts(ExportOptions {
            hash,
            target: temp.clone(),
            mode: ExportMode::Copy,
        })
        .stream()
        .await;
    while let Some(item) = stream.next().await {
        match item {
            ExportProgressItem::Size(_) | ExportProgressItem::CopyProgress(_) => {}
            ExportProgressItem::Done => break,
            ExportProgressItem::Error(e) => {
                let _ = tokio::fs::remove_file(&temp).await;
                anyhow::bail!("export failed: {e}");
            }
        }
    }
    // Honor a cancel that arrived while exporting: never deliver the file
    // for a cancelled Transfer.
    if *control.borrow() == RecvControl::Cancel {
        let _ = tokio::fs::remove_file(&temp).await;
        return Ok(());
    }
    tokio::fs::rename(&temp, destination).await?;
    Ok(())
}

fn update_local_bytes(engine: &Arc<Engine>, id: &str, bytes: u64) {
    let mut reg = engine.registry.lock().unwrap();
    if let Some(entry) = reg.receives.get_mut(id) {
        entry.local_bytes = bytes;
        entry.record.last_activity_ms = now_unix_ms();
    }
}

/// Direct/Relayed from the selected path of the connection (ADR 0022).
fn connection_kind(conn: &iroh::endpoint::Connection) -> ConnectionKind {
    let paths = conn.paths();
    for p in paths.iter() {
        if p.is_selected() {
            return if p.is_ip() {
                ConnectionKind::Direct
            } else if p.is_relay() {
                ConnectionKind::Relayed
            } else {
                ConnectionKind::Unknown
            };
        }
    }
    ConnectionKind::Unknown
}
