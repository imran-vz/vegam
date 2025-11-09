# Vegam

**Fast, secure P2P file transfer between devices**

Vegam enables direct file transfers between macOS and Android devices without servers, clouds, or intermediaries. Built on [Iroh](https://iroh.computer) for automatic NAT traversal and peer discovery.

## Features

- **Direct P2P transfers** - Files go directly between devices
- **No file size limits** - Transfer anything
- **Zero configuration** - Works across NATs and firewalls
- **Cross-platform** - macOS and Android
- **Open source** - MIT licensed

## Download

**macOS**: Download `.dmg` from [Releases](../../releases)

**Android**: Download `.apk` from [Releases](../../releases)

## How It Works

1. **Send**: Select file → Share ticket string
2. **Receive**: Paste ticket → Download file

Devices connect directly using Iroh's relay network for NAT traversal. Files never touch third-party servers.

## Development

### Prerequisites

- Node.js 20+
- pnpm
- Rust 1.70+
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

**Android only**:

- Android Studio
- Android SDK 33+
- NDK

### Setup

```bash
git clone https://github.com/yourusername/vegam.git
cd vegam
pnpm install
```

### Run

```bash
# Desktop
pnpm tauri dev

# Android (device/emulator required)
pnpm tauri android dev
```

### Build

```bash
# macOS
pnpm tauri build

# Android APK
pnpm tauri android build
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
```

See [CLAUDE.md](CLAUDE.md) for detailed architecture.

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
