# Vegam

**Fast, direct file transfer between your own computers**

Vegam transfers a single file directly between desktop computers without servers, clouds, or intermediaries. The Sender creates a Transfer Ticket, shares it out-of-band, and the Receiver uses it to download the file. Built on [Iroh](https://iroh.computer) for NAT traversal with relay fallback.

## Desktop Release Scope

Vegam v1 targets desktop platforms:

- **macOS** 12 and newer (`.dmg`)
- **Windows** 10 and 11 (`.msi`)
- **Linux** mainstream desktops with AppImage-compatible glibc (`.AppImage`)

Mobile (Android/iOS) is future work and is not part of the v1 production path. Preserved mobile implementation knowledge lives in [`docs/mobile/`](docs/mobile/).

## How It Works

1. **Send**: Select one file → Vegam creates a Transfer Ticket → copy and share it
2. **Receive**: Paste the Transfer Ticket → choose a destination → download

Transfer Tickets are bearer values: anyone holding a valid ticket can download the file while the Sender keeps Vegam open. In v1, Transfer Tickets will expire after 24 hours by default (this expiry is not implemented yet; today a ticket stays usable for as long as the Sender keeps Vegam open). Devices connect directly when possible and fall back to Iroh's public relay network when they can't; file data never rests on third-party servers.

## Download

Desktop v1 release artifacts are not published yet. When they are, you will download them from [Releases](../../releases): v1 releases will be unsigned, so expect macOS Gatekeeper and Windows SmartScreen warnings, and every artifact will ship with a SHA-256 checksum to verify before installing.

## Development

### Prerequisites

- Node.js 20+
- pnpm
- Rust (see `src-tauri/Cargo.toml` for the minimum version)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Setup

```bash
git clone https://github.com/yourusername/vegam.git
cd vegam
pnpm install
```

### Run

```bash
pnpm tauri dev
```

### Verify

```bash
pnpm run check
cd src-tauri && cargo check
cd src-tauri && cargo test
```

### Build

```bash
pnpm tauri build
```

## Tech Stack

- **Frontend**: React 19, TypeScript, Tailwind CSS, shadcn/ui
- **Backend**: Rust, Tauri v2
- **Networking**: [Iroh](https://iroh.computer) for P2P connectivity

## Project Structure

```text
src/               # React frontend
src-tauri/         # Rust backend
  src/iroh/        # Iroh integration
  src/state.rs     # App state management
  capabilities/    # Tauri permissions
docs/              # Product context, roadmap, ADRs, research
```

Product scope and terminology live in [CONTEXT.md](CONTEXT.md), the roadmap in [docs/roadmap/](docs/roadmap/), and decisions in [docs/adr/](docs/adr/).

## Contributing

Contributions welcome! Please open an issue first for major changes.

1. Fork the repo
2. Create feature branch (`git checkout -b feature/thing`)
3. Commit changes (`git commit -am 'Add thing'`)
4. Push to branch (`git push origin feature/thing`)
5. Open Pull Request

## License

MIT License - see [LICENSE](LICENSE) for details

## Acknowledgments

- [Iroh](https://iroh.computer) - P2P networking
- [Tauri](https://tauri.app) - Cross-platform framework
