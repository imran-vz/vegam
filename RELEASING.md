# Releasing Vegam (Desktop v1)

Desktop v1 ships exactly three artifacts (ADR 0010): macOS `.dmg`,
Windows `.msi`, Linux `.AppImage`. They are unsigned (ADR 0012), every
artifact gets a published SHA-256 checksum (ADR 0013), and updates are
manual downloads only (ADR 0011).

## Cutting a release

1. Bump `version` in `src-tauri/tauri.conf.json` (this drives the artifact
   names), `src-tauri/Cargo.toml`, and `package.json`; run
   `cd src-tauri && cargo check` so `Cargo.lock` picks up the new version;
   commit all four files.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
3. The `release` GitHub Actions workflow builds the `.dmg`, `.msi`, and
   `.AppImage` on real macOS/Windows/Linux runners, uploads them to a
   **draft** release, generates `SHA256SUMS.txt`, and writes the checksums
   into the release body (copyable code block).
4. Review the draft: artifacts present, checksums in the body, install
   instructions below linked. Publish.

A macOS maintainer can also build the `.dmg` locally with
`pnpm tauri build --bundles dmg`; the Windows and Linux artifacts require
their native platforms (the CI runners cover this).

## Verifying a download (user instructions)

Compare the published checksum against the file you downloaded:

- **macOS / Linux**: `shasum -a 256 <file>` (or `sha256sum <file>`)
- **Windows** (PowerShell): `Get-FileHash <file> -Algorithm SHA256`

The output must match the value in `SHA256SUMS.txt` / the release notes
exactly. If it does not, delete the download.

## Expected install friction (unsigned builds, ADR 0012)

- **macOS Gatekeeper** (macOS 15 and newer — the common case): opening the
  app is blocked with no override in the dialog. Open **System Settings →
  Privacy & Security**, scroll to the security section, and click
  **Open Anyway** next to the Vegam message, then confirm.
  - On macOS 12–14 the older bypass also works: Control-click the app →
    **Open** → **Open**.
  - If macOS instead reports *"Vegam" is damaged and can't be opened* (this
    can happen for unsigned/un-notarized downloads, especially on Apple
    Silicon), clear the quarantine flag from Terminal:
    `xattr -cr /Applications/Vegam.app` — only after verifying the SHA-256
    checksum above.
  - These flows must be re-validated against a real quarantined download of
    each release; Gatekeeper behavior shifts between macOS versions.
- **Windows SmartScreen**: "Windows protected your PC" — click
  **More info** → **Run anyway**.
- **Linux**: mark the AppImage executable (`chmod +x Vegam*.AppImage`) and
  run it. Mainstream desktops with an AppImage-compatible glibc are
  supported (ADR 0023).

This friction is an accepted v1 trade-off; the SHA-256 verification above
is the integrity path that replaces paid code signing for now.

## Updates

There is no auto-updater in v1 (ADR 0011). Users install new versions by
downloading the new artifact and replacing the old install.
