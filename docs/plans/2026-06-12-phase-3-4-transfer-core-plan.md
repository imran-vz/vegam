# Phase 3 + 4 Implementation Plan: Transfer Core and Managed Resume Storage

Date: 2026-06-12. Derived from the Phase 2 spike
(`docs/research/2026-06-12-iroh-blobs-production-spike.md`,
`spikes/iroh-blobs/`) and ADRs 0003–0022. Verified against the vendored
`iroh-blobs 0.102.0` sources, then dependency-checked against
`iroh-blobs 0.103.0` on 2026-06-16.

## Verified API facts the plan relies on

- `BlobsProtocol::new(store: &Store, events: Option<EventSender>)`;
  `EventSender::channel(capacity, mask) -> (EventSender, mpsc::Receiver<ProviderMessage>)`.
- `EventMask { connected, get, get_many, push, observe, throttle }`.
  **Critical (corrected by review):** in 0.103.0 the `get_many`/`push`/
  `observe` mask fields are dead code — `EventSender::request()` dispatches
  ALL four request types on `mask.get` (events.rs:462 via provider.rs
  151-178). So `get_many: Disabled` provides no protection by itself; with
  `get: InterceptLog`, GetMany/Push/Observe requests arrive as their own
  `ProviderMessage` intercept variants. The gate loop MUST exhaustively
  match every `ProviderMessage` variant and explicitly reject
  GetMany/Push/Observe (unhandled intercepts fail closed only by accident).
  Keep `get_many`/`push: Disabled` in the mask anyway as forward-compat
  hardening for when upstream fixes per-type routing, and add a gate test
  that sends a real GetMany/Push and asserts rejection.
- `ClientConnected { connection_id, endpoint_id: Option<EndpointId> }`,
  `RequestReceived<GetRequest> { connection_id, request_id, request }` with a
  public `request.hash` — maps connection → receiver identity, request →
  Transfer.
- Replying `Err(AbortReason::Permission)` / `Err(AbortReason::RateLimited)`
  aborts with wire codes `ERR_PERMISSION = 1` / `ERR_LIMIT = 2`; receivers
  recover them via `GetError::iroh_error_code()` — the channel for
  distinguishing "rejected: expired/cancelled" from "rejected: paused, retry".
- With `RequestMode::InterceptLog`, each accepted request hands the gate an
  `mpsc::Receiver<RequestUpdate>` (capacity 32). If the gate drops it, the
  in-flight send aborts (provider `notify_payload_write` fails) — the
  mechanism for killing in-flight uploads on pause/cancel. Corollary: the
  gate MUST actively drain each accepted request's update stream.
- GC is periodic-only: `FsStore::load_with_opts(.., Options { gc:
  Some(GcConfig { interval, add_protected }), .. })`; `gc_run_once` is
  private. Blob deletion goes through tag removal + sweep.
- `iroh_tickets::Ticket` trait (KIND prefix + postcard + base32 lowercase) is
  how `BlobTicket` gets its `blob...` string; implement the same trait for
  the Vegam ticket. iroh-blobs 0.103 uses `iroh-tickets = "1.0.0"`.

## Resolved design decisions

### D1. Vegam Transfer Ticket
`VegamTicket { blob: BlobTicket, file_name: String, size: u64, issued_at: u64 }`
implementing `iroh_tickets::Ticket` with `KIND = "vegam"` → string form
`vegam<base32>`. Wire format: single-variant postcard enum embedding the
BlobTicket's own `encode_bytes()` as bytes (delegation keeps upstream wire
compatibility). **Drop the AES ticket codec entirely** — its key derives from
the node id embedded in the ticket (obfuscation, not encryption; see the
2026-06-12 reset note); ADR 0003 makes tickets bearer by design and QUIC/TLS
covers transport. `issued_at` in the ticket is advisory (receiver pre-flight
UX); authoritative expiry is enforced sender-side (D3). **Display Name is NOT
carried in the ticket** (ADR 0017: cosmetic, not identity; avoids trust
inference from a bearer artifact).

### D2. Sender model
`SendTransfer { id, source_path, file_name, size, hash, issued_at, ticket,
status: Importing|Available|Paused|Expired|ContentSuspect|ContentChanged|
SourceMissing|Cancelled, started_receivers: BTreeSet<EndpointId> (persisted),
source_len, source_mtime_unix_ms }`. Import via `TryReference`; named tag
`send/<id>`; same-hash dedup per the re-share policy in D3.

### D3. Expiry/pause/cancel enforcement (provider gate)
`EventMask { connected: Intercept, get: InterceptLog, get_many: Disabled,
push: Disabled, observe: Intercept, throttle: None }` (see corrected mask
fact above — the gate must still explicitly reject GetMany/Push/Observe).
Gate decision is a pure function
`decide(transfer, requester: Option<EndpointId>, now)`.

**Abort-code discipline (corrected by review): `Permission` is reserved for
genuinely TERMINAL outcomes; transient states use `RateLimited` so receivers
keep their resumable state and retry.**

- unknown hash → `Permission` (terminal; also stops re-serving received
  blobs);
- `Cancelled` → `Permission` (terminal);
- `ContentChanged` (re-hash-confirmed, see below) → `Permission` (terminal);
- expired and requester ∉ `started_receivers` → `Permission` (terminal);
- `Paused` → `RateLimited` (transient);
- `SourceMissing` → `RateLimited` (transient — the Sender may reselect the
  moved file per ADR 0005; receivers must survive the gap);
- `Importing` (initial import or reselect re-hash in progress) →
  `RateLimited` (transient);
- `ContentSuspect` (len/mtime drift, re-hash pending) → `RateLimited`;
- `Available` && now < issued_at+24h → accept + insert requester into
  `started_receivers` + persist;
- past expiry && requester ∈ `started_receivers` → accept.

Receiver identity = `EndpointId` from `ClientConnected` (receivers persist
their secret key, so it survives restarts; `None` ⇒ treated as new, rejected
post-expiry; in 0.103.0 it is in practice always `Some`).

Content-change detection (corrected by review — ADR 0005 defines sameness by
Content Identity, NOT mtime): a `stat()` len/mtime mismatch only flips the
transfer to `ContentSuspect` and schedules a background re-hash of the
source; hash equal → restore previous state and update the stat baseline
(mtime-only churn from backup tools must NOT kill transfers); hash different
→ `ContentChanged` (terminal). There is NO provider-side verification-failure
cause signal (the provider's `Aborted` update carries stats only), so the
sweeper + re-hash is the authoritative sender-side mechanism; repeated
receiver-side `ERR_INTERNAL` resets are a corroborating trigger to schedule
the re-hash sooner.

60s sweeper flips Available→Expired for UI; the gate checks wall-clock
exactly. Pause = reject new (`RateLimited`) + drop in-flight update
receivers (aborts streams). Cancel = in-flight abort +
`tags().delete("send/<id>")` + prune record; source file untouched (ADR
0019). Reselect: re-import new path (state `Importing` meanwhile), hash
match → restore (same ticket stays valid, in-flight receivers resume after
their next retry), mismatch → revert to `SourceMissing` and return
`contentMismatch` (new ticket required).
Receiver count (ADR 0016): distinct EndpointIds with a live accepted request
(Started seen, not ended); identities never cross the Tauri boundary.

Re-share / same-hash dedup policy (corrected by review): `create_send_transfer`
for a hash with an existing record returns the existing transfer only when it
is `Available`/`Paused`/`Importing`. For `Expired`/`Cancelled`/
`ContentChanged`/`SourceMissing` it REPLACES the record: new id, fresh
`issued_at`, cleared `started_receivers`, fresh ticket. **Documented bearer
consequence:** the gate authorizes by content hash and requests carry only
the hash, so re-sharing identical content makes ALL previously issued
tickets for that content valid again (they embed the same hash and endpoint
id). This is inherent to content-addressed bearer tickets without recipient
binding (ADR 0003/0007); the Phase 5 UI copy for re-sharing must say so.

### D4. Receiver model
`ReceiveTransfer { id, ticket, hash, file_name, size, issued_at, destination,
status: Connecting|Downloading|Paused|StalledRetrying|Verifying|Exporting|
Complete|Failed|Cancelled|NoLongerResumable }`. One task per transfer:
spike loop + `watch::Receiver<Control>` (Run|Pause|Cancel). Tag `recv/<id>`.
Retry: exponential backoff 1s→30s cap, indefinitely in StalledRetrying;
`ERR_PERMISSION` → NoLongerResumable (terminal — the gate only emits it for
cancelled/expired-for-new/confirmed-content-change; `resume_receive_transfer`
from NoLongerResumable retries exactly once so a user can recover from a
sender-side reselect that completed after the rejection); `ERR_LIMIT` →
sender paused or in a transient state (source missing/reselecting,
importing, content re-hash pending), stay StalledRetrying with slow 30s
retry; receiver-local verification `DecodeError` → Failed (contentMismatch);
`ERR_INTERNAL` stream resets → backoff retry, and after 3 consecutive resets
surface `error_code: "contentMismatch"` as the probable cause (spike
scenario E: sender content change manifests receiver-side as ERR_INTERNAL
resets, not as a local decode error) while continuing slow retries until the
sender gate resolves to a terminal answer; other errors → backoff retry. Completion: assert
`is_complete()` → export `ExportMode::Copy` to `<dest>.vegampart` → atomic
`rename` (ADR 0020: nothing corrupt-looking at the destination). Then delete
tag + prune record (no history, ADR 0009). Cancel = watch signal + tag
delete + prune; GC reclaims. Direct/Relayed from `Connection::paths()` on
connect + throttled progress emits.

### D5. Persistence
Plain JSON (not redb) under `app_local_data_dir`:
`identity/secret.key` (hex, 0600), `settings.json` (schema_version,
display_name), `transfers.json` (schema_version, sends, receives), `blobs/`
(FsStore = managed resume area). Atomic `.tmp`+rename writes on state
transitions only; byte counts never persisted (FsStore bitfield is truth).
Ordering rule: write metadata record BEFORE importing/tagging so the store
never holds meaningful partials unknown to metadata. Prune Complete/Cancelled;
keep Failed/NoLongerResumable receive records (they own visible resume-area
bytes) until cleaned.

### D6. Managed resume area + GC (Phase 4)
`GcConfig { interval: 60s, add_protected }` where the callback protects every
hash referenced by any live record. GC can only collect blobs whose records
were explicitly removed → "no silent auto-delete" holds; reclamation lags ≤
sweep interval. `list_partial_downloads` → `ResumeAreaReport` from
`remote().local()` + on-disk `blobs/data/{hash}.*` sizes + `resumable` +
`stale` (7 days inactivity, display-only) flags; orphan data files surfaced
as explicit `orphan` entries. `cleanup_partial_download`: tracked → cancel +
tag delete + prune; orphan → acknowledged, swept by GC.

### D7. Engine/threading
`AppState(RwLock<Option<Arc<Engine>>>)`; `Engine { endpoint, router, store,
registry: Arc<Mutex<Registry>>, events: broadcast::Sender<EngineEvent>,
paths, settings }`. Engine has ZERO tauri imports (events via broadcast;
lib.rs forwards to the Tauri Emitter) → integration-testable without Tauri
and keeps iroh-blobs behind a Vegam-owned boundary. Commands never block:
import/download run as tasks emitting progress events. Logging: transfer ids
only — no file names/paths/tickets/hashes/peer ids at default level.

### D8. Display Name
`settings.json`, `adjective-noun` from embedded word lists (`rand`),
editable via `set_display_name`, never on the wire in Phase 3/4.

## Cargo.toml rework
Remove: iroh-gossip, iroh-base, redb, quic-rpc, iroh-io, bytes, hostname,
aes-gcm, sha2, base64, data-encoding, tracing-subscriber.
Add/change: `iroh = "=1.0.0"`, `iroh-blobs = "0.103"`,
`iroh-tickets = "=1.0.0"`, `irpc = "0.17"`, `postcard = { version = "1",
features = ["use-std"] }`, `hex = "0.4"`, `n0-future = "0.3"`, `rand =
"0.9"` (names only). Keep: serde, serde_json, tokio(full), tracing, anyhow, uuid, log,
tauri + dialog/log/fs/opener/clipboard-manager plugins.

## Command surface (Rust) and TS contract
Commands (all `Result<T, ApiError>`, `ApiError { code: ErrorCode, message }`):
`init_app`, `get_app_snapshot`, `create_send_transfer(file_path)`,
`pause_send_transfer(id)`, `resume_send_transfer(id)`,
`cancel_send_transfer(id)`, `reselect_send_source(id, file_path)`,
`inspect_ticket(ticket)` (pure parse), `create_receive_transfer(ticket,
destination_path)`, `pause/resume/cancel_receive_transfer(id)`,
`list_partial_downloads`, `cleanup_partial_download(entry_id)`,
`get_settings`, `set_display_name(display_name)`.
Removed: init_node, get_node_id, send_file, receive_file,
get_transfer_status, list_peers, get_device_name, parse_ticket_metadata,
get_relay_status (per-transfer `connection_kind` covers ADR 0022 for now).
Events: `send-transfer-updated`, `receive-transfer-updated`,
`receive-transfer-progress` (throttled 250ms).

TS types (no `any`): SendTransferStatus, ReceiveTransferStatus,
ConnectionKind ("direct"|"relayed"|"unknown"), ErrorCode ("ticketInvalid"|
"ticketExpired"|"notFound"|"sourceMissing"|"contentMismatch"|
"alreadyExists"|"io"|"network"|"rejected"|"internal"), SendTransferInfo
(incl. `ticket: string | null`, `active_receiver_count`, `import_progress`),
ReceiveTransferInfo (incl. `local_bytes`, `connection_kind`, `error_code`),
ReceiveProgress, TicketPreview (incl. `is_probably_expired` advisory),
PartialDownloadEntry (kind "tracked"|"orphan", disk_bytes, resumable,
stale), ResumeAreaReport, Settings, AppSnapshot. Rust enums
`#[serde(rename_all = "camelCase")]`; struct fields snake_case.

## Implementation order (each step keeps cargo check green)
1. Remove Peer Discovery on old deps (delete discovery.rs/node.rs, gossip
   types, list_peers/get_device_name; frontend references).
2. Dependency swap + engine skeleton (Cargo.toml; delete src/iroh/ +
   platform.rs; create engine/mod.rs init, engine/error.rs,
   engine/settings.rs; thin state.rs; gut lib.rs to init_app/snapshot/
   settings commands; gate stub rejects all).
3. engine/ticket.rs (VegamTicket + tests) + inspect_ticket.
4. engine/persist.rs (records, atomic write, load+prune, tests).
5. engine/events.rs + engine/send.rs (import task, tag, ticket mint after
   `endpoint.online()`, stat helpers, reselect) + send commands.
6. engine/gate.rs (real gate, pure decide() with table tests, receiver
   count, in-flight abort, sweepers) + pause/resume commands.
7. engine/recv.rs (download task, classification, .vegampart export+rename,
   cancel) + receive commands + progress events.
8. Restart resume in Engine::init (re-arm sends, re-spawn active receives,
   paused stay paused, prune terminal).
9. engine/resume_area.rs (report, cleanup, GC protect callback) + commands
   (Phase 4).
10. Frontend: rewrite api.ts, update state-machines.ts, rewire
    SendFile/ReceiveFile/App.tsx (functional parity; full UX is Phase 5).
11. tests/transfer_core.rs: two Engines in one process (local direct addrs;
    relay variants #[ignore]): happy path; receiver-restart resume (only
    missing bytes); sender pause stalls/resume completes; backdated
    issued_at rejects new but admits seeded started receiver; source
    mutation → receiver observes ERR_INTERNAL resets, sender re-hash flips
    to contentChanged, receiver lands NoLongerResumable; moved-file
    reselect-and-continue: rename the source mid-transfer → receiver stalls
    on ERR_LIMIT keeping its partial, sender reselects the new path, hash
    matches, receiver resumes and completes (ADR 0005 acceptance); a
    mtime-only touch does NOT kill the transfer (ContentSuspect re-hash
    restores Available); a GetMany request is explicitly rejected by the
    gate; receiver cancel frees bytes after sweep. Then clippy -D warnings,
    fmt, cargo test.

## Documented assumptions (flagged, not blocking)
1. Receives that were actively downloading at shutdown auto-resume on next
   launch; paused stay paused (one-line policy change if undesired).
2. Expired sends keep serving already-started Receivers until Sender
   Cancellation (ADR 0015 sets no outer resume deadline).
3. "Stale" Partial Download = 7 days without activity, display-only.
4. Re-sharing identical content re-validates previously issued tickets for
   that content (content-addressed bearer semantics, see D3). The Phase 5
   re-share UI copy must state this.
