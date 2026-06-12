# Desktop V1 End-To-End Validation Report

Date: 2026-06-12. Phase 8 of `docs/roadmap/desktop-v1.md`.

Validation host: macOS (Darwin 25.5.0, Apple Silicon, APFS). Evidence
sources: the automated integration suite (`src-tauri/tests/transfer_core.rs`,
12 tests, two-to-three real iroh engines per test), the Phase 2 production
spike (`docs/research/2026-06-12-iroh-blobs-production-spike.md`, 7 scripted
scenarios), and local packaging runs.

## Validation matrix

| Roadmap Phase 8 item | macOS | Windows | Linux | Evidence |
| --- | --- | --- | --- | --- |
| Transfer tests | PASS | GAP | GAP | Integration suite: 13/13 green (happy path, pause/stall/resume both sides, expiry both arms, content change, mtime-only churn, moved-file reselect, restarts, receiver + sender cancel, GetMany rejection, multi-receiver) |
| Direct path | PASS | GAP | GAP | All integration tests run over local direct connections; spike scenario A/F confirm Direct selection |
| Relayed path | PARTIAL | GAP | GAP | Spike logs show live Relayed→Direct upgrades and a mid-transfer Direct→Relayed→Direct migration over the default public relays; no sustained relay-only soak test was run |
| 100 GB / sparse equivalent | PASS | GAP | GAP | Spike scenario F: 100 GiB transferred and exported, 35 MB sender / 32 MB receiver max RSS, 1229 s |
| App restart resume (Sender and Receiver) | PASS | GAP | GAP | Integration: receiver_restart_resumes_from_partial_state, paused_receive_survives_restart_and_resumes; spike scenario C (sender restart, persisted key keeps ticket valid) |
| Temporary network loss resume | PASS | GAP | GAP | Spike scenario G (SIGSTOP link stall) and C (abrupt connection death), byte-exact resume |
| Manual pause/resume/cancel | PASS | GAP | GAP | Integration: sender + receiver pause/resume tests; receiver cancel (receiver_cancel_removes_record_and_partial_state) and sender cancel (sender_cancel_terminates_receiver_without_touching_source); spike scenario D |
| Ticket expiration + started-receiver resume | PASS | GAP | GAP | Integration: expiry_rejects_new_but_admits_started_receiver; gate unit decision table incl. exact boundary |
| Multiple Receivers per ticket | PASS | GAP | GAP | Integration: multiple_receivers_one_ticket (two receivers, one bearer ticket, both byte-identical) |
| Sender content change / deletion / moved-file | PASS | GAP | GAP | Integration: content_change_is_terminal_after_rehash, mtime_only_touch_keeps_transfer_available, moved_file_reselect_and_continue |
| Partial Download visibility and cleanup | PASS | GAP | GAP | Integration: receiver_cancel_removes_record_and_partial_state + resume-area report; Storage tab UI |
| Unsigned install/download instructions | PASS (docs) | PASS (docs) | PASS (docs) | RELEASING.md documents Gatekeeper/SmartScreen/AppImage friction and SHA-256 verification commands per OS |

## Packaging (Phase 7)

- macOS `.dmg`: built locally (`pnpm tauri build --bundles dmg`) →
  `Vegam_0.1.0_aarch64.dmg` (11 MB), SHA-256 computed via `shasum -a 256`
  (verification flow exercised end-to-end).
- Windows `.msi` and Linux `.AppImage`: cannot be built on this macOS host.
  The release pipeline (`.github/workflows/release.yml`) builds all three on
  native runners on a version tag, publishes them to a draft release, and
  attaches `SHA256SUMS.txt` plus copyable checksums in the release body.
  **Gap until the first tag runs the pipeline.**
- No updater infrastructure exists or is required (ADR 0011).

## Boundary checks (all PASS on this host)

- No mobile build path required: no mobile deps, plugins, or build targets
  remain (Phase 1).
- No directory transfer path exposed: file pickers are single-file; the
  engine rejects non-files.
- No Peer Discovery path exposed: gossip removed in Phase 3; tickets are the
  only connection path.
- No full transfer history: completed/cancelled records are pruned; the UI
  shows active transfers only.
- No sensitive telemetry/log fields by default: log filter persists only
  Vegam targets; telemetry is opt-in, key-gated, and structurally limited to
  coarse fields (Phase 6 review-audited).

## Documented platform gaps (release blockers until closed)

1. **Windows and Linux runtime validation** — the entire transfer matrix has
   run only on macOS. Required: run the integration suite and a manual
   two-machine smoke test on Windows 10/11 and a mainstream Linux desktop.
   The suite is platform-independent Rust; CI or two volunteer machines can
   close this.
2. **Real two-machine transfers** — all automated evidence is
   process-to-process on one host (real iroh endpoints, but one kernel).
   Required: one cross-network smoke test (different LANs, relay involved)
   per the spike's recommendation.
3. **Relay-only soak** — relayed connectivity was observed working, but no
   long transfer was forced to stay on relays.
4. **First release-pipeline run** — the workflow is untested until the first
   `v*` tag; expect one iteration of CI debugging. The macOS job builds a
   universal binary (`--target universal-apple-darwin`) so Intel Macs are
   covered per ADR 0023; the locally built dmg is aarch64-only and is
   evidence of the build path, not a releasable artifact.
5. **`.vegampart` cross-volume rename** — export uses same-directory
   temp+rename; destinations on exotic mounts (network shares) where rename
   semantics differ have not been tested.
6. **Quarantined-install validation** — the Gatekeeper/SmartScreen
   instructions in RELEASING.md have not been exercised against a real
   browser-downloaded, quarantined artifact; macOS in particular may show
   the "damaged" dialog for unsigned downloads (the documented `xattr`
   fallback covers it, but the actual first-run flow must be confirmed per
   release).

Per the roadmap's acceptance ("pass on supported desktop platforms or have
documented platform-specific limitations"), Phase 8 is complete on macOS
with the limitations above documented as the cross-platform release
checklist.
