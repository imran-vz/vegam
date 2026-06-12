# AGENTS.md

Guidance for coding agents working in this repository.

## Read First

- Start with `CONTEXT.md`, then `docs/roadmap/`, then `docs/plans/`, then the ADRs in `docs/adr/`.
- Treat those docs as the source of truth for product scope and terminology.
- Do not infer requirements from stale code, old README text, or previous Android direction when they conflict with the roadmap or ADRs.
- If implementation reality contradicts the docs, stop and surface the contradiction before coding.

## Product Direction

Vegam is currently being realigned for a Desktop Release: macOS, Windows, and Linux.

- Mobile is future work, not part of v1.
- Desktop v1 is single-file transfer only.
- Use explicit Transfer Ticket sharing; do not add Peer Discovery for v1.
- Prefer the Iroh 1.0-generation stack and `iroh-blobs` only if the documented production spike passes.
- Keep changes aligned with the roadmap phases unless the user explicitly changes priority.

## Core Principles

- Make surgical changes. Touch only files needed for the task.
- Prefer simple, verifiable implementation over speculative abstraction.
- Optimize for speed and reliability in transfers, startup, resume, and release workflows.
- Maintain strong UI/backend contracts; do not use TypeScript `any` to hide unclear Tauri command payloads or response shapes.
- Do not silently choose between ambiguous product interpretations; ask or document the assumption.
- Preserve user work and unrelated local changes.
- Update docs when a decision changes behavior, scope, terminology, privacy, release, or architecture.
- Do not reintroduce mobile code into the desktop v1 production path.
- Do not treat Display Name as identity, authentication, authorization, or trust.
- Do not log or send sensitive transfer data by˝ default.

## Verification Commands

Use `pnpm` for frontend/package work.

After every major code change, run the relevant checks and report any skipped command with the reason.

Baseline checks:

```bash
pnpm run check
cd src-tauri && cargo fmt --check
cd src-tauri && cargo check
```

When Rust behavior changes:

```bash
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

When frontend behavior changes:

```bash
pnpm run build
```

Before a release/package change is considered done:

```bash
pnpm tauri build
```

Notes:

- There is no dedicated JavaScript lint script at the time of writing; do not claim one passed unless it exists.
- Some release/package checks may be platform-specific. Run what is possible locally and document gaps.
- Documentation-only changes do not require full build/test runs, but should still pass `git diff --check`.

## Useful Commands

```bash
pnpm install
pnpm tauri dev
pnpm tauri build
pnpm run check
cd src-tauri && cargo check
cd src-tauri && cargo test
```

## Working Agreement

- Keep final summaries short and factual.
- Include what changed, what was verified, and what remains unresolved.
- If a task is blocked by an unresolved product decision, ask one precise question and recommend an answer.
