# Iroh Blobs Production Spike

Phase 2 of `docs/roadmap/desktop-v1.md` (decision recorded in ADR `0021`).
Proves whether `iroh = 1.0.0-rc.1` + `iroh-blobs = 0.102` meet the Vegam
desktop v1 transfer bar before the production implementation commits to them.

This crate is intentionally **not** part of the production app. It is a CLI
with three subcommands:

- `send --store <dir> --file <path>` — import the file without copying
  (`ImportMode::TryReference`), print a Transfer Ticket, and serve until
  killed. The `--store` dir persists the endpoint secret key, so a restarted
  sender keeps the same EndpointId and previously issued tickets stay valid.
- `recv --store <dir> --ticket <t> --dest <path>` — fetch into the store (the
  managed resume area), resuming from any local partial data, then export to
  `dest` only after the blob verifies complete. `--retries N` reconnects after
  errors; `--interactive` accepts `pause` / `resume` / `quit` on stdin.
- `status --store <dir> --ticket <t>` — report local partial bytes and the
  resume-area files for a ticket without touching the network.

Machine-readable `KEY: value` lines go to stdout; human chatter to stderr.

## Running the acceptance scenarios

```bash
cargo build --release
./scenarios/a-sanity.sh                      # 100MB, byte identity, Direct/Relayed report
./scenarios/b-receiver-restart.sh            # SIGKILL receiver, resume from partial
./scenarios/c-sender-restart-network-loss.sh # SIGKILL sender, restart, receiver retries
./scenarios/d-pause-resume.sh                # manual pause/resume via stdin
./scenarios/e-content-change.sh              # mutate source mid-transfer, must fail safely
./scenarios/g-network-stall.sh               # SIGSTOP/SIGCONT sender (link loss, sender alive)
./scenarios/f-100gb.sh                       # 100GB sparse equivalent, bounded RSS (needs ~115GB free)
```

Scenarios use `/tmp/vegam-spike` (override with `SPIKE_WORK`). Each prints
`SCENARIO_RESULT: PASS|FAIL` and exits non-zero on failure.

Results and the production-readiness verdict live in
`docs/research/2026-06-12-iroh-blobs-production-spike.md`.

## Note on the `time` crate pin

`Cargo.lock` pins `time = 0.3.47`: `time >= 0.3.48` breaks the build of
`rcgen 0.14.8` (transitive via iroh) with a trait-coherence error, while
`vergen-gitcl 9.1` (transitive via iroh-relay) needs `time >= 0.3.45`. If the
build breaks after a `cargo update`, re-pin with
`cargo update time --precise 0.3.47`.
