//! The Vegam transfer engine: owns the iroh endpoint, the blob store (the
//! managed resume area), the provider gate, and all transfer state.
//!
//! Deliberately free of Tauri imports — events flow through a broadcast
//! channel that `lib.rs` forwards to the UI. This keeps the engine
//! integration-testable without a Tauri runtime and keeps the iroh-blobs
//! surface behind a Vegam-owned boundary.

pub mod error;
pub mod events;
pub mod gate;
pub mod persist;
pub mod recv;
pub mod resume_area;
pub mod send;
pub mod settings;
pub mod ticket;
pub mod types;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use iroh::{endpoint::presets, Endpoint, EndpointId, RelayMode, SecretKey};
use iroh_blobs::{
    provider::events::{
        ConnectMode, EventMask, EventSender, ObserveMode, RequestMode, ThrottleMode,
    },
    store::{
        fs::{options::Options, FsStore},
        GcConfig, ProtectCb, ProtectOutcome,
    },
    BlobsProtocol, Hash,
};
use tokio::sync::{broadcast, watch};

use crate::engine::error::ErrorCode;
use crate::engine::events::EngineEvent;
use crate::engine::persist::{ReceiveRecord, SendRecord, TransfersFile};
use crate::engine::types::{
    ConnectionKind, ReceiveStatus, ReceiveTransferInfo, SendStatus, SendTransferInfo, Settings,
};

/// Filesystem layout under the app-local data dir.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
}

impl AppPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn secret_key(&self) -> PathBuf {
        self.root.join("identity").join("secret.key")
    }
    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn transfers(&self) -> PathBuf {
        self.root.join("transfers.json")
    }
    /// The managed resume area (ADR 0020): the FsStore root.
    pub fn blobs(&self) -> PathBuf {
        self.root.join("blobs")
    }
}

/// Runtime state of one Send Transfer (the persisted part is `record`).
pub struct SendTransfer {
    pub record: SendRecord,
    pub hash: Hash,
    pub import_progress: Option<f64>,
    pub error: Option<String>,
}

/// Control signal for a receive task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvControl {
    Run,
    Pause,
    Cancel,
}

/// Runtime state of one Receive Transfer.
pub struct ReceiveEntry {
    pub record: ReceiveRecord,
    pub control: watch::Sender<RecvControl>,
    pub local_bytes: u64,
    pub connection_kind: Option<ConnectionKind>,
    pub error_code: Option<ErrorCode>,
    pub error: Option<String>,
}

/// One accepted, in-flight provider request. Aborting `drain` drops the
/// RequestUpdate receiver, which kills the in-flight upload (the provider's
/// progress notification fails) — the mechanism behind sender pause/cancel.
pub struct ActiveRequest {
    pub hash: Hash,
    pub endpoint: Option<EndpointId>,
    pub drain: tokio::task::AbortHandle,
}

#[derive(Default)]
pub struct Registry {
    pub sends: HashMap<String, SendTransfer>,
    pub sends_by_hash: HashMap<Hash, String>,
    pub receives: HashMap<String, ReceiveEntry>,
    pub conn_endpoints: HashMap<u64, EndpointId>,
    pub active_requests: HashMap<(u64, u64), ActiveRequest>,
    /// Send ids with a content re-hash in flight (dedup guard).
    pub rehashing: HashSet<String>,
}

impl Registry {
    /// ADR 0016: the Sender sees a count of distinct active Receivers for a
    /// hash; identities never leave the engine.
    pub fn active_receiver_count(&self, hash: &Hash) -> u32 {
        let endpoints: HashSet<Option<EndpointId>> = self
            .active_requests
            .values()
            .filter(|r| &r.hash == hash)
            .map(|r| r.endpoint)
            .collect();
        endpoints.len() as u32
    }

    pub fn send_info(&self, transfer: &SendTransfer) -> SendTransferInfo {
        let r = &transfer.record;
        let importing = r.status == SendStatus::Importing;
        SendTransferInfo {
            id: r.id.clone(),
            file_name: r.file_name.clone(),
            size: r.size,
            source_path: r.source_path.to_string_lossy().to_string(),
            ticket: if importing {
                None
            } else {
                Some(r.ticket.clone())
            },
            issued_at_ms: if importing {
                None
            } else {
                Some(r.issued_at * 1000)
            },
            expires_at_ms: if importing {
                None
            } else {
                Some(
                    r.issued_at
                        .saturating_add(crate::engine::ticket::TICKET_TTL_SECS)
                        * 1000,
                )
            },
            status: r.status,
            active_receiver_count: self.active_receiver_count(&transfer.hash),
            import_progress: transfer.import_progress,
            error: transfer.error.clone(),
        }
    }

    pub fn receive_info(&self, entry: &ReceiveEntry) -> ReceiveTransferInfo {
        let r = &entry.record;
        ReceiveTransferInfo {
            id: r.id.clone(),
            file_name: r.file_name.clone(),
            size: r.size,
            destination_path: r.destination.to_string_lossy().to_string(),
            status: r.status,
            local_bytes: entry.local_bytes,
            connection_kind: entry.connection_kind,
            error_code: entry.error_code,
            error: entry.error.clone(),
        }
    }
}

/// Knobs that tests tighten; production uses the defaults.
#[derive(Debug, Clone)]
pub struct Tuning {
    pub relay_mode: RelayMode,
    /// Receiver wait between retries when the Sender reports a transient
    /// state (paused/reselecting/importing).
    pub stall_retry: Duration,
    /// GC sweep interval for the managed resume area.
    pub gc_interval: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            relay_mode: RelayMode::Default,
            stall_retry: Duration::from_secs(30),
            gc_interval: Duration::from_secs(60),
        }
    }
}

pub struct Engine {
    pub endpoint: Endpoint,
    #[allow(dead_code)] // Keeps the BlobsProtocol accepting connections.
    pub router: iroh::protocol::Router,
    pub store: FsStore,
    pub registry: Arc<Mutex<Registry>>,
    pub events: broadcast::Sender<EngineEvent>,
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    pub tuning: Tuning,
    /// Hashes the GC must never collect: every hash referenced by a live
    /// record. Doubly protects tracked blobs (tags + this set) so GC can
    /// only reclaim what explicit user action released (ADR 0020).
    protected: Arc<Mutex<HashSet<Hash>>>,
}

impl Engine {
    pub async fn init(root: PathBuf) -> Result<Arc<Self>> {
        Self::init_with_tuning(root, Tuning::default()).await
    }

    pub async fn init_with_tuning(root: PathBuf, tuning: Tuning) -> Result<Arc<Self>> {
        tokio::fs::create_dir_all(&root).await?;
        let paths = AppPaths::new(root);

        let secret = load_or_create_secret(&paths.secret_key())?;
        let settings = settings::load_or_create(&paths.settings())?;
        let transfers = persist::load(&paths.transfers())?;

        let protected: Arc<Mutex<HashSet<Hash>>> = Arc::new(Mutex::new(HashSet::new()));
        let protect_cb: ProtectCb = {
            let protected = protected.clone();
            Arc::new(move |set: &mut std::collections::HashSet<Hash>| {
                let protected = protected.clone();
                Box::pin(async move {
                    let guard = protected.lock().unwrap();
                    set.extend(guard.iter().copied());
                    ProtectOutcome::Continue
                })
            })
        };
        let blobs_root = paths.blobs();
        tokio::fs::create_dir_all(&blobs_root).await?;
        let mut store_options = Options::new(&blobs_root);
        store_options.gc = Some(GcConfig {
            interval: tuning.gc_interval,
            add_protected: Some(protect_cb),
        });
        let store = FsStore::load_with_opts(blobs_root.join("blobs.db"), store_options)
            .await
            .context("loading blob store")?;

        let endpoint = Endpoint::builder(presets::N0)
            .alpns(vec![iroh_blobs::protocol::ALPN.to_vec()])
            .secret_key(secret)
            .relay_mode(tuning.relay_mode.clone())
            .bind()
            .await
            .context("binding iroh endpoint")?;

        // The gate intercepts connects and get requests so the engine can
        // enforce ticket expiry, pause, and cancellation (ADR 0015/0018/0019).
        // Note: in iroh-blobs 0.102 every request type dispatches on
        // `mask.get`; the Disabled values on get_many/push are forward-compat
        // hardening, and the gate loop still explicitly rejects those
        // variants.
        let mask = EventMask {
            connected: ConnectMode::Intercept,
            get: RequestMode::InterceptLog,
            get_many: RequestMode::Disabled,
            push: RequestMode::Disabled,
            observe: ObserveMode::Intercept,
            throttle: ThrottleMode::None,
        };
        let (event_sender, provider_rx) = EventSender::channel(64, mask);

        let blobs = BlobsProtocol::new(&store, Some(event_sender));
        let router = iroh::protocol::Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs)
            .spawn();

        let (events, _) = broadcast::channel(256);

        let engine = Arc::new(Self {
            endpoint,
            router,
            store,
            registry: Arc::new(Mutex::new(Registry::default())),
            events,
            paths,
            settings: Mutex::new(settings),
            tuning,
            protected,
        });

        engine.restore_transfers(transfers).await?;
        gate::spawn(engine.clone(), provider_rx);
        gate::spawn_sweepers(engine.clone());

        Ok(engine)
    }

    /// Re-arm persisted transfers after an app restart.
    async fn restore_transfers(self: &Arc<Self>, transfers: TransfersFile) -> Result<()> {
        for record in transfers.sends {
            if record.hash.len() != 64 {
                // An import that never completed (no hash, no ticket): the
                // user re-creates the transfer; leftovers surface as orphans.
                continue;
            }
            let Ok(hash) = record.hash.parse::<Hash>() else {
                tracing::warn!("dropping send record with unparseable hash");
                continue;
            };
            let mut record = record;
            // Re-check the source file; flips Available/Expired/SourceMissing
            // or schedules a re-hash for ContentSuspect.
            send::refresh_send_status_from_disk(&mut record);
            let id = record.id.clone();
            let mut reg = self.registry.lock().unwrap();
            reg.sends_by_hash.insert(hash, id.clone());
            reg.sends.insert(
                id,
                SendTransfer {
                    record,
                    hash,
                    import_progress: None,
                    error: None,
                },
            );
        }

        let receive_records: Vec<ReceiveRecord> = transfers.receives;
        for record in receive_records {
            if record.hash.len() != 64 {
                continue;
            }
            let Ok(hash) = record.hash.parse::<Hash>() else {
                tracing::warn!("dropping receive record with unparseable hash");
                continue;
            };
            let resume_now = match record.status {
                // Was running by user intent — auto-resume (documented
                // assumption 1 in the Phase 3/4 plan).
                s if s.is_active() => true,
                // Paused stays paused; Failed/NoLongerResumable stay parked
                // (their resume-area bytes remain visible per ADR 0020).
                _ => false,
            };
            let id = record.id.clone();
            let (control, _) = watch::channel(if resume_now {
                RecvControl::Run
            } else {
                RecvControl::Pause
            });
            let mut record = record;
            if resume_now {
                record.status = ReceiveStatus::Connecting;
            }
            {
                let mut reg = self.registry.lock().unwrap();
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
            }
            if resume_now {
                recv::spawn_receive_task(self.clone(), id, hash);
            }
        }
        self.persist()?;
        Ok(())
    }

    /// Write the current transfer records to disk (atomic) and refresh the
    /// GC protect set. Call on every state transition; the registry lock
    /// must NOT be held by the caller.
    pub fn persist(&self) -> Result<()> {
        let (file, hashes) = {
            let reg = self.registry.lock().unwrap();
            let file = TransfersFile {
                schema_version: persist::SCHEMA_VERSION,
                sends: reg.sends.values().map(|t| t.record.clone()).collect(),
                receives: reg.receives.values().map(|e| e.record.clone()).collect(),
            };
            // Importing records carry an empty hash placeholder; skip them.
            // (Hash::from_str panics on wrong-length input instead of
            // erroring, so length-guard before parsing.)
            let hashes: HashSet<Hash> = file
                .sends
                .iter()
                .map(|r| r.hash.as_str())
                .chain(file.receives.iter().map(|r| r.hash.as_str()))
                .filter(|h| h.len() == 64)
                .filter_map(|h| h.parse::<Hash>().ok())
                .collect();
            (file, hashes)
        };
        *self.protected.lock().unwrap() = hashes;
        persist::save(&self.paths.transfers(), &file)
    }

    pub fn emit(&self, event: EngineEvent) {
        // No receivers is fine (e.g. during tests).
        let _ = self.events.send(event);
    }

    /// Emit the current state of a send transfer.
    pub fn emit_send(&self, id: &str) {
        let info = {
            let reg = self.registry.lock().unwrap();
            reg.sends.get(id).map(|t| reg.send_info(t))
        };
        if let Some(info) = info {
            self.emit(EngineEvent::SendTransferUpdated(info));
        }
    }

    /// Emit the current state of a receive transfer.
    pub fn emit_receive(&self, id: &str) {
        let info = {
            let reg = self.registry.lock().unwrap();
            reg.receives.get(id).map(|e| reg.receive_info(e))
        };
        if let Some(info) = info {
            self.emit(EngineEvent::ReceiveTransferUpdated(info));
        }
    }

    pub fn snapshot(&self) -> crate::engine::types::AppSnapshot {
        let reg = self.registry.lock().unwrap();
        crate::engine::types::AppSnapshot {
            settings: self.settings.lock().unwrap().clone(),
            send_transfers: reg.sends.values().map(|t| reg.send_info(t)).collect(),
            receive_transfers: reg.receives.values().map(|e| reg.receive_info(e)).collect(),
        }
    }

    pub async fn shutdown(&self) {
        let _ = tokio::time::timeout(Duration::from_secs(2), self.router.shutdown()).await;
        let _ = self.store.shutdown().await;
    }
}

/// Persist the endpoint secret key so the EndpointId — and therefore every
/// issued Transfer Ticket — survives app restarts (spike scenario C).
fn load_or_create_secret(path: &Path) -> Result<SecretKey> {
    if path.exists() {
        let hex_str = std::fs::read_to_string(path)?;
        let bytes: [u8; 32] = hex::decode(hex_str.trim())
            .context("secret key file is not valid hex")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("secret key file is not 32 bytes"))?;
        Ok(SecretKey::from_bytes(&bytes))
    } else {
        let key = SecretKey::generate();
        persist::write_atomic(path, hex::encode(key.to_bytes()).as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(key)
    }
}
