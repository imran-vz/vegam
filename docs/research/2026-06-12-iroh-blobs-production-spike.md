# Iroh Blobs Production Spike Results

Date: 2026-06-12

Phase 2 of `docs/roadmap/desktop-v1.md`. Decides whether the latest
Iroh 1.0-generation stack (`iroh = 1.0.0-rc.1`, `iroh-blobs = 0.102.0`) meets
the v1 transfer bar set by ADR `0021`, despite the upstream README's
production-quality warning on this `iroh-blobs` line.

Spike code: `spikes/iroh-blobs/` (CLI + scripted scenarios). All scenarios ran
on macOS (Darwin 25.5.0, APFS) between two local processes over real Iroh
endpoints using the default public relay infrastructure. Two-machine and
cross-OS validation is Phase 8 scope.

## Version recheck (roadmap requirement)

Rechecked on 2026-06-12 via `cargo info`: `iroh = 1.0.0-rc.1`,
`iroh-blobs = 0.102.0`, both `rust-version = 1.91`. These match the
2026-06-12 research note. `sendme 0.35.0` pins `iroh` to exactly
`=1.0.0-rc.1` and `iroh-blobs` to `0.102.x`, and was used as the API
reference.

## Acceptance results

| Roadmap acceptance criterion | Result | Evidence (scenario) |
| --- | --- | --- |
| Transfers a 100 GB file or sparse-file equivalent desktop-to-desktop | PASS | F: 100 GB sparse source (random 1 MB block every 4 GB) transferred and exported; destination size and spot-checks verified |
| Bounded memory; no whole-file reads | PASS | F: max RSS sender/receiver far below the 2 GB assert bound (see below); sender imports via `ImportMode::TryReference` (no copy, no read-into-memory) |
| Verifies Content Identity | PASS | BLAKE3 verified streaming end-to-end; A–D/G byte-identical exports (`cmp`); E detects mid-transfer corruption |
| Resumes after temporary network loss | PASS | G: SIGSTOP'd sender (silent link, sender alive) → receiver errors, retries, resumes after SIGCONT with no full re-download. C additionally covers abrupt connection death |
| Resumes after Sender and Receiver app restart | PASS | B: receiver SIGKILL'd at ~40%, restart fetched only missing ~60%. C: sender SIGKILL'd, restarted from same store (persisted secret key → same EndpointId → original ticket stays valid), receiver completed |
| Supports manual pause/resume | PASS | D: stdin `pause` halts fetch (zero progress for 10 s, partial preserved, nothing at destination), `resume` completes, export byte-identical |
| Stores Receiver Partial Downloads in the managed resume area | PASS | B: after kill, store dir holds `<hash>.data` (partial payload), `<hash>.obao4` (outboard), `<hash>.sizes4`; `status` reports `LOCAL_BYTES`/`IS_COMPLETE: false`; destination stays untouched until verification completes |
| Fails safely when Sender content changes | PASS | E: source mutated mid-transfer under `TryReference` → the provider fails verification and resets the stream; the receiver exits non-zero with `stream reset by peer: error 3` (`ERR_INTERNAL` — NOT distinguishable receiver-side as a verification failure), no export, destination untouched |
| Shows Direct vs Relayed in spike output | PASS | A proves the reporting line is emitted; the live `Relayed paths=[relay*]` → `Direct paths=[relay,direct*,…]` upgrade was observed mid-transfer in the C and F logs via `Connection::paths()` (`is_relay()`/`is_ip()`/`is_selected()`) |

### Scenario F numbers (100 GiB = 107,374,182,400 bytes)

From the final run (after review tightened the measurement to attach the
sender's RSS monitor at process spawn, so the figure covers the 100 GiB
import/hashing phase as well as serving):

- Source: sparse file, 100 GiB logical / 25 MB physical (random 1 MB block
  every 4 GB, holes elsewhere).
- Sender max RSS: **35 MB** (import + serve). Receiver max RSS: **32 MB**
  (assert bound was 2 GB; sampled at 1 s intervals for the whole run).
- Transfer wall time: **1229 s** (~87 MB/s sustained, two processes on one
  machine over real Iroh endpoints, Direct path). An earlier run of the same
  scenario additionally showed a live mid-transfer Direct→Relayed→Direct
  path migration.
- Destination size exact; random-block and hole-region spot checks pass
  (full integrity already guaranteed by BLAKE3 verified streaming — every
  16 KiB chunk verified before hitting the store).
- Import of 100 GiB via `TryReference` copies nothing; only the outboard is
  computed and stored.

## Key API findings for the production implementation (Phase 3+)

- **Store**: `iroh_blobs::store::fs::FsStore::load(dir)` — persistent,
  resume-capable store. Partial blobs live as `{hash}.data` +
  `{hash}.obao4` + `{hash}.sizes4` (+ bitfield state) under `data/`. This is
  the natural managed resume area for ADR `0020`.
- **Import without copying**: `store.add_path_with_opts(AddPathOptions {
  mode: ImportMode::TryReference, format: BlobFormat::Raw, .. })` references
  the source file in place; only the outboard (~64 B per 16 KiB chunk group,
  ≈400 MB for 100 GB) is computed and stored. Hold the returned `TempTag` or
  persist a named tag via `store.tags().set(name, hash)` to protect from GC.
- **Resume**: `store.remote().local(hash_and_format)` returns a `LocalInfo`
  with a persisted bitfield; `local.missing()` yields a `GetRequest` covering
  only absent ranges; `store.remote().execute_get(conn, request)` streams
  with `GetProgressItem::{Progress(bytes), Done(Stats), Error}`. Progress
  offsets are session-relative (add `local.local_bytes()` for absolute).
- **Export**: `store.export_with_opts(ExportOptions { mode: ExportMode::Copy,
  .. })` — uses reflink on APFS (instant, no extra space), falls back to copy.
  Run it only after `local.is_complete()`. Caveat: the export writes the
  TARGET path directly (no temp+rename inside iroh-blobs), so on
  non-reflink filesystems (Windows NTFS, most Linux setups) a crash
  mid-export still leaves a truncated file at the destination. The
  spike's "nothing corrupt-looking at the destination" property therefore
  needs the production implementation to export to a sibling temp name and
  atomically rename (the Phase 3/4 plan's `.vegampart` rule) — verify-first
  ordering alone is not sufficient.
- **Ticket**: `iroh_blobs::ticket::BlobTicket { EndpointAddr, Hash,
  BlobFormat }`, base32 string form. The hash inside the ticket IS the
  Content Identity. Note: the ticket alone is the full bearer credential.
- **Sender identity across restarts**: persist the endpoint `SecretKey`;
  `Endpoint::builder(presets::N0).secret_key(k)` reproduces the EndpointId so
  previously shared tickets keep working after a Sender app restart.
- **Direct vs Relayed**: `Connection::paths()` snapshot — per-path
  `is_ip()` / `is_relay()` / `is_selected()`; also `paths_stream()` for live
  updates (relay→direct hole-punch upgrades were observed mid-transfer).
- **Online readiness**: wait on `endpoint.online()` (bounded by a timeout)
  before minting a ticket so it carries a relay URL.
- **Content change semantics**: with `TryReference`, a changed source makes
  the provider fail verification while serving — but what the Receiver
  observes is a generic `stream reset by peer: error 3` (`ERR_INTERNAL`),
  NOT a receiver-local decode/verification error; it is indistinguishable
  from other provider-side failures. The provider's own event stream also
  carries no cause (its `Aborted` update has stats only). Production error
  classification must treat repeated `ERR_INTERNAL` resets on the same hash
  as probable content change, and the Sender app must detect the state
  itself (len/mtime as a *suspicion* trigger, confirmed by re-hash — ADR
  `0005` defines sameness by Content Identity, not mtime) to mark the
  Transfer no-longer-resumable.
- **Session accounting**: `GetProgressItem::Done(Stats)` is emitted only for
  sessions that complete; a session ending in `Error` reports no `Stats`.
  Production byte/throughput accounting must derive from `Progress` offsets
  plus `local_bytes()` deltas, never from summing `Done(Stats)` (learned via
  scenario C's original assertion, which counted sessions and was wrong).

## Gotchas / risks carried into Phase 3

1. **Ecosystem pin**: originally, `time >= 0.3.48` broke `rcgen 0.14.8`
   (transitive via the rc-era iroh stack), so the spike pinned
   `time = 0.3.47`. Superseded on 2026-06-16: `iroh = 1.0.0`,
   `iroh-blobs = 0.103.0`, and `irpc = 0.17.0` compile with `time = 0.3.49`,
   so the production app and spike no longer carry this pin.
2. **`iroh-blobs` production warning**: upstream still labels 0.10x
   pre-production. The spike found no blocking defect across kill/stall/
   corruption scenarios; residual risk is API churn before iroh 1.0 final.
   Mitigation: keep the transfer surface behind a small Vegam-owned module
   boundary so version bumps stay local.
3. **Receiver-side disk**: partial data + export both live on the user's
   disk; export is reflink-cheap on APFS but a real copy elsewhere (Windows
   NTFS has no reflink via this path; budget for it in Phase 4 UX).
4. **Single-machine evidence**: relay→direct upgrade was observed, but
   sustained relay-only throughput and cross-platform behavior are untested
   here; Phase 8 covers real two-machine matrices.
5. **Error classification**: `GetError` variants need mapping into
   user-meaningful states (sender offline vs content changed vs local I/O) in
   Phase 3; the spike treats them uniformly as retryable.

## Verdict

**Adopt `iroh-blobs` for the v1 transfer path** (per ADR `0021`'s
prefer-blobs-if-spike-passes rule). All nine roadmap acceptance criteria for
Phase 2 pass. The custom-stream fallback (Option B of the 2026-06-12 reset
note) is no longer under consideration for v1.
