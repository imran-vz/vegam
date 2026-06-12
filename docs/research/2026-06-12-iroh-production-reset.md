# Iroh Production Reset Research

Date: 2026-06-12

This note resets Vegam's production direction around a desktop-first release for Windows, Linux, and macOS. Mobile remains a future requirement, but Android/iOS work is out of scope for the next production milestone.

## Assumptions

- "Latest Iroh stuff" means tracking the current Iroh 1.0-generation APIs, not preserving compatibility with the older `iroh = 0.26` architecture described in older project docs.
- "Production" means a desktop app that can be built, distributed, and relied on for real file transfers, not only a local demo.
- This pass documents research and decisions only. It does not update runtime code, dependency pins, README, or build scripts.

## Primary Sources Checked

- `cargo search iroh`: latest observed `iroh = 1.0.0-rc.1`.
- `cargo info iroh`: `iroh = 1.0.0-rc.1`, Rust version `1.91`, docs at https://docs.rs/iroh/1.0.0-rc.1, repository https://github.com/n0-computer/iroh.
- `cargo info iroh-blobs`: `iroh-blobs = 0.102.0`, Rust version `1.91`, docs at https://docs.rs/iroh-blobs/0.102.0, repository https://github.com/n0-computer/iroh-blobs.
- `cargo info iroh-gossip`: `iroh-gossip = 0.100.0`, Rust version `1.91`, docs at https://docs.rs/iroh-gossip/0.100.0.
- `cargo info sendme`: `sendme = 0.35.0`, Rust version `1.91`, repository https://github.com/n0-computer/sendme.
- Iroh concepts docs: https://docs.iroh.computer/concepts/endpoints, https://docs.iroh.computer/concepts/discovery, https://docs.iroh.computer/protocols/blobs.
- Local crate sources downloaded by Cargo for `iroh-1.0.0-rc.1`, `iroh-blobs-0.102.0`, `iroh-gossip-0.100.0`, `sendme-0.35.0`, and `dumbpipe-0.38.0`.

## Current Iroh Ecosystem Findings

| Area | Finding | Impact on Vegam |
| --- | --- | --- |
| Core connectivity | `iroh` is currently published as `1.0.0-rc.1`. The main API is `Endpoint`, with peer identity based on public keys / endpoint IDs. | Vegam should target the 1.0-generation API shape and avoid old 0.26 assumptions. |
| Relay and NAT traversal | Iroh uses relay servers for reachability and hole-punching assistance. If direct connectivity fails, encrypted traffic can continue over relay fallback. | Desktop production must test direct and relayed paths across real networks. |
| Protocol routing | Application protocols are selected by ALPN. The latest examples use `Router::builder(endpoint).accept(ALPN, protocol).spawn()`. | Vegam needs a clear transfer protocol boundary instead of mixing discovery, transfer, and UI state. |
| Addressing | Connecting usually needs an `EndpointId` plus address information, or an address lookup mechanism. | A transfer ticket must carry enough addressing data or rely on a lookup service. This is a product/security decision, not only an implementation detail. |
| Blob transfer | `iroh-blobs = 0.102.0` provides BLAKE3 verified streaming, blob ranges, sequences, stores, tickets, and downloader APIs. | This fits file transfer well, especially integrity verification and future directory support. |
| Blob production warning | The `iroh-blobs 0.102.0` README says this version is not yet considered production quality and recommends `iroh-blobs 0.35` for production quality. | This is the main dependency risk. We need a spike and release-readiness decision before committing production work to latest blobs. |
| Reference apps | `sendme 0.35.0` is a maintained file/directory transfer reference using `iroh = 1.0.0-rc.1` and `iroh-blobs = 0.102`. `dumbpipe 0.38.0` is a maintained raw stream transfer reference using `iroh = 1.0.0-rc.1`. | Use `sendme` as the closest reference for file/directory transfer and `dumbpipe` as a reference for custom streaming over Iroh. |
| Rust version | Latest Iroh-generation crates report `rust-version = 1.91`. | The production toolchain/CI must be updated from the repo's older Rust assumptions. |

## Repo Drift Observed

- `README.md`, `AGENTS.md`, and `CLAUDE.md` still describe macOS plus Android as the product direction.
- `src-tauri/Cargo.toml` already references newer Iroh-family crates (`iroh = 0.95`, `iroh-blobs = 0.97`, `iroh-gossip = 0.95`) but not the latest observed releases.
- Android-specific dependencies, plugins, generated files, and QR scanning UI are still present.
- `src-tauri/src/platform.rs` has Android-specific file URI handling and desktop `tokio::fs::read`.
- The current send path reads the whole file into memory before adding it to the blob store. That is not production-grade for large files.
- The current blob store path is inconsistent: `Iroh::new` creates a data directory but uses `MemStore`.
- The current gossip discovery creates a new random topic per node, so it is not yet a real cross-device discovery story unless peers have a shared topic.
- The current `vegam://` ticket "encryption" derives the key from the sender node ID embedded in the ticket. Anyone with the ticket can derive the same key, so this should be treated as obfuscation, not recipient-only encryption.

## Transfer Design Options

### Option A: Latest `iroh-blobs`

Use `iroh-blobs 0.102.x` with `iroh 1.0.0-rc.x`.

Pros:

- Matches the project's file-transfer shape.
- Provides BLAKE3 verified transfer.
- Supports blob sequences and collections, which could later support directories.
- `sendme` is a close maintained reference.

Risks:

- The crate itself says the current generation is not yet production quality.
- API churn may continue until Iroh 1.0 final and matching blob crates settle.
- We still need to design ticket semantics, availability, lifetime, permissions, and progress reporting.

Recommended next check:

- Build a minimal desktop-only spike that sends and receives one large file between two machines using latest `iroh` and `iroh-blobs`.
- Verify memory usage, interrupted transfer behavior, direct-vs-relay behavior, and whether the warning blocks production.

### Option B: Custom Iroh Protocol Over Streams

Use `iroh` directly with a Vegam-specific ALPN and stream file bytes over QUIC streams.

Pros:

- Avoids relying on the current `iroh-blobs` production-quality warning.
- Gives Vegam full control over file metadata, progress, cancelation, and ticket format.
- `dumbpipe` is a maintained reference for simple stream transfer over latest Iroh.

Risks:

- Vegam would own integrity verification, resume behavior, protocol compatibility, and multi-file semantics.
- More custom protocol surface means more testing before production.
- We may recreate functionality that `iroh-blobs` already provides.

Recommended next check:

- Keep this as the fallback path if the `iroh-blobs` spike fails production-readiness criteria.

## Production Readiness Questions

- Should transfer tickets be bearer credentials, or should they be recipient-bound?
- Does a Sender need to keep Vegam open until the Receiver completes the transfer?
- Is resumable transfer required for production v1, or only nice-to-have?
- Is directory transfer required for production v1, or single-file transfer only?
- Should peer discovery be part of production v1, or should production v1 use explicit ticket sharing only?
- What file sizes must be supported without reading entire files into memory?
- What desktop distribution paths are required first: DMG, MSI/EXE, AppImage/deb/rpm?
- Do we require automatic updates for the first production release?
- What telemetry/logging is acceptable for transfer diagnosis while preserving privacy?

## Recommended Near-Term Direction

1. Keep the next production milestone desktop-only: Windows, Linux, macOS.
2. Update documentation and planning around latest `iroh = 1.0.0-rc.1`, while rechecking crate versions before implementation.
3. Run a focused transfer spike against latest `iroh` plus `iroh-blobs` before committing to the blob path.
4. Treat the current ticket format and discovery flow as prototypes, not production decisions.
5. Remove mobile build/dependency pressure from the production path only after the desktop requirement grilling confirms scope.
