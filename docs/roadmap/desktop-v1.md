# Desktop V1 Roadmap

This roadmap turns the Desktop Release decisions into implementation phases and acceptance criteria. It is scoped to Windows, Linux, and macOS; the Mobile Release remains future work. Source decisions live in `CONTEXT.md` and ADRs `0001` through `0023`.

## V1 Product Boundary

Desktop v1 is a single-file peer-to-peer Transfer app built on the Iroh 1.0-generation stack. It uses explicit Transfer Ticket sharing, supports Resumable Transfers for files of at least 100 GB, and ships desktop packages for macOS, Windows, and Linux.

V1 does not include mobile apps, directory transfer, Peer Discovery, automatic updates, paid platform signing, Vegam-owned relays, recipient-bound tickets, single-use tickets, full transfer history, or cloud/background file hosting.

## Phase 1: Desktop-Only Baseline

Remove mobile from the active production path while preserving hard-to-recover mobile knowledge for later.

Work:

- Remove Android/iOS-specific dependencies, plugins, capabilities, UI controls, and build targets from active desktop builds.
- Preserve mobile-specific findings in documentation before deleting implementation details that would be hard to reconstruct.
- Update user-facing and developer-facing docs away from macOS/Android and toward Windows/Linux/macOS.
- Keep `pnpm`, Tauri v2, React, TypeScript, Rust, and Iroh as the desktop stack.

Acceptance:

- `pnpm run check` and `cd src-tauri && cargo check` run against the desktop path without mobile dependencies.
- README/developer docs describe Desktop Release scope: macOS 12+, Windows 10/11, and mainstream Linux desktops with AppImage-compatible glibc.
- No Android QR scanning, Android content URI handling, Android release commands, or mobile plugins are required for desktop builds.

## Phase 2: Iroh Blobs Production Spike

Prove whether latest `iroh` plus `iroh-blobs` can carry the v1 transfer requirements before committing the implementation.

Work:

- Recheck current Iroh crate versions before implementation.
- Build a minimal desktop-to-desktop spike with latest Iroh 1.0-generation APIs.
- Prefer `iroh-blobs` if the spike passes; keep a custom Iroh stream protocol as fallback only if the spike fails the v1 bar.
- Use default public Iroh relay infrastructure.
- Capture direct vs relayed connection state.

Acceptance:

- Transfers a 100 GB file or sparse-file equivalent desktop-to-desktop.
- Uses bounded memory and does not read whole files into memory.
- Verifies Content Identity.
- Resumes after temporary network loss.
- Resumes after Sender and Receiver app restart.
- Supports manual pause/resume.
- Stores Receiver Partial Downloads in the managed resume area.
- Fails safely when Sender content changes.
- Shows whether the Transfer path is Direct or Relayed in diagnostics/spike output.

## Phase 3: Transfer Core

Implement the durable transfer model behind the UI.

Work:

- Model Sender, Receiver, Transfer, Resumable Transfer, Partial Download, Cancellation, Content Identity, and Transfer Ticket consistently with `CONTEXT.md`.
- Implement bearer Transfer Tickets.
- Allow multiple Receivers per Transfer Ticket until expiration or Sender Cancellation.
- Expire Transfer Tickets after 24 hours by default.
- Allow already-started Receivers with valid partial state to resume after ticket expiration.
- Require Sender availability; no cloud storage, relay-owned file hosting, or background availability service.
- Detect Sender content changes/deletion and mark the Transfer no-longer-resumable.
- If Sender moved the same file, ask the Sender to reselect it and continue only if Content Identity matches.
- Implement Sender-side and Receiver-side pause/resume.
- Implement Cancellation separately from pause.

Acceptance:

- A Receiver with a valid Transfer Ticket can complete a single-file Transfer.
- Any holder of a valid Transfer Ticket can receive while the Sender makes it available.
- New Receivers are blocked after ticket expiration.
- Already-started Receivers can resume after expiration when their partial state is valid.
- Sender Cancellation stops availability without deleting the Sender source file.
- Receiver Cancellation deletes Receiver partial state.
- Pause preserves resumable state.
- Content changes require a new Transfer Ticket.
- Moved files require reselect-and-match before continuing.

## Phase 4: Managed Resume Storage

Make Partial Downloads reliable, inspectable, and user-controlled.

Work:

- Store Receiver Partial Downloads in an app-managed resume area.
- Finalize or atomically move to the user-selected destination only after content verification succeeds.
- Show leftover Partial Downloads and their storage use.
- Mark stale and no-longer-resumable entries clearly.
- Provide cleanup actions.
- Avoid silent auto-delete in v1.

Acceptance:

- Failed or paused downloads do not leave corrupt-looking files at the final destination.
- Completed downloads appear at the user-selected destination only after verification.
- Users can see storage consumed by leftover Partial Downloads.
- Users can clean up stale/no-longer-resumable Partial Downloads.
- Vegam does not silently delete resumable partial state.

## Phase 5: Desktop UX

Build the v1 product surface around explicit ticket sharing and active Transfers.

Work:

- Provide Send and Receive flows for one file per Transfer.
- Generate and display Transfer Tickets with clear bearer-ticket language.
- Support copy/paste ticket sharing out-of-band.
- Show active/current Transfers only.
- Persist only enough transfer state to resume interrupted Transfers.
- Show manual pause/resume and cancel controls.
- Show active Receiver count only, not Receiver identities.
- Show subtle Direct/Relayed status in active Transfer detail.
- Add editable Device Display Name, defaulting to a random two-word name.
- Ensure Display Name is never treated as identity, authentication, authorization, or trust.

Acceptance:

- Sender can select one file, create a Transfer Ticket, copy it, and see availability state.
- Receiver can paste a Transfer Ticket, choose destination, and receive the file.
- UI clearly communicates that anyone with the ticket can download while available.
- UI clearly communicates that Sender must keep Vegam open for availability.
- UI supports pause/resume/cancel on both sides.
- Sender sees active Receiver count only.
- Users can edit Display Name.
- Active Transfer detail shows Direct or Relayed without exposing deep network internals.

## Phase 6: Diagnostics And Privacy

Support debugging without quietly leaking sensitive transfer data.

Work:

- Keep local logs that users can share on demand.
- Add an explicit diagnostics export path.
- Avoid file names, file paths, Transfer Tickets, peer IDs, IP addresses, exact file sizes, and content hashes in local logs by default.
- Allow short-lived internal transfer IDs and coarse file-size buckets.
- Add a user-visible runtime setting for PostHog.
- Keep PostHog disabled by default.
- When enabled, send only product-level events and coarse technical diagnostics.
- Forbid PostHog from sending file names, file paths, Transfer Tickets, peer IDs, IP addresses, exact file sizes, or content hashes.

Acceptance:

- Default app usage sends no PostHog telemetry.
- User can enable/disable PostHog from the UI.
- Diagnostics export is user-initiated.
- Logs and telemetry satisfy the forbidden-field rules.
- Diagnostics include enough coarse information to distinguish Direct vs Relayed transfer issues.

## Phase 7: Release Packaging

Produce unsigned/manual desktop release artifacts with user-verifiable integrity.

Work:

- Build macOS `.dmg`.
- Build Windows `.msi`.
- Build Linux `.AppImage`.
- Do not require paid Apple or Windows signing certificates for v1.
- Expect and document macOS Gatekeeper and Windows SmartScreen friction.
- Publish SHA-256 checksums for every release artifact.
- Make each checksum copyable.
- Provide a verify affordance in the download experience.
- Use manual downloads only; no automatic updates in v1.

Acceptance:

- Release artifacts exist for macOS, Windows, and Linux.
- Release instructions explain unsigned macOS/Windows behavior.
- Every artifact has a published SHA-256 checksum.
- Users can copy checksums and access a verification flow.
- No updater infrastructure is required for v1.

## Phase 8: End-To-End Release Validation

Validate the full v1 story before public release.

Work:

- Run transfer tests on macOS, Windows, and Linux.
- Test direct and relayed paths.
- Test 100 GB or sparse-file equivalent Transfers.
- Test app restart resume on Sender and Receiver.
- Test temporary network loss resume.
- Test manual pause/resume/cancel.
- Test ticket expiration and already-started resume after expiration.
- Test multiple Receivers for one Transfer Ticket.
- Test Sender file content change, deletion, and moved-file reselect.
- Test Partial Download visibility and cleanup.
- Test unsigned install/download instructions.

Acceptance:

- All v1 transfer acceptance criteria pass on supported desktop platforms or have documented platform-specific limitations.
- No mobile build path is required for release.
- No directory transfer path is exposed.
- No Peer Discovery path is exposed.
- No full transfer history is exposed.
- No sensitive telemetry/log fields are emitted by default.

## Implementation Order

1. Desktop-only cleanup.
2. Iroh blobs spike.
3. Transfer core and persistence.
4. Managed resume storage.
5. Desktop UX.
6. Diagnostics and privacy.
7. Packaging and checksum verification.
8. End-to-end release validation.

## Key Risks

- Latest `iroh-blobs` has a production-quality warning; the spike must decide whether this is acceptable.
- Resumability across app restarts and source-file moves requires careful persistent state design.
- 100 GB support makes bounded memory and disk cleanup non-negotiable.
- Unsigned macOS and Windows artifacts create user trust friction that release docs must address.
- Default public Iroh relay dependency means relay behavior and outages need clear diagnostics.
