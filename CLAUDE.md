# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Vegam is a P2P file transfer app built with Tauri v2, React, TypeScript, and Iroh. It enables direct file transfers between devices using the Iroh networking library for NAT traversal and peer discovery.

## Build Commands

### Desktop Development

```bash
pnpm tauri dev              # Start dev server (frontend + Tauri)
pnpm tauri build            # Build production app
make dev                    # Alternative: run desktop dev mode
make build-desktop          # Alternative: build desktop app
```

### Android Development

```bash
pnpm tauri android dev      # Run on Android device/emulator
pnpm tauri android build    # Build debug APK
make android-release        # Build signed release APK (requires keystore setup)
make android-install        # Install debug APK to connected device
```

### Testing and Validation

```bash
cd src-tauri && cargo check # Check Rust compilation
cd src-tauri && cargo test  # Run Rust tests
pnpm run check              # Check TypeScript
make check                  # Install deps + check Rust
```

## Architecture

### Frontend (React + TypeScript)

- **Entry**: `src/App.tsx` - main UI with Send/Receive tabs
- **Components**:
  - `SendFile.tsx` - file selection and ticket generation
  - `ReceiveFile.tsx` - ticket input and file receiving
- **API Layer**: `src/lib/api.ts` - TypeScript wrappers for Tauri commands
- **UI**: shadcn/ui components with Tailwind CSS

### Backend (Rust + Tauri)

- **Entry**: `src-tauri/src/lib.rs` - Tauri app initialization and command handlers
- **State Management**: `src-tauri/src/state.rs`
  - `AppState` - holds Iroh endpoint, transfers, and peers
  - Thread-safe with `Arc<RwLock<>>`

- **Iroh Integration**: `src-tauri/src/iroh/`
  - `node.rs` - Iroh endpoint initialization, node ID management
  - `transfer.rs` - file transfer logic (send/receive tickets)
  - `discovery.rs` - device name resolution
- **Platform Abstraction**: `src-tauri/src/platform.rs`
  - Handles Android content:// URIs via `tauri-plugin-android-fs`
  - Desktop uses standard tokio::fs

### Data Flow

1. **Initialization**:
   - Frontend calls `initNode()`
   - Backend creates Iroh `Endpoint` with relay servers for NAT traversal
   - Returns node ID to display

2. **Send File**:
   - User selects file via Tauri dialog (returns content URI on Android, file path on desktop)
   - Backend reads file using platform-specific `read_file()` in `platform.rs`
   - Imports file to Iroh in-memory blob store
   - Creates enhanced ticket format: `filename|size|blob_ticket`
   - Returns ticket string to share (includes metadata for receiver)

3. **Receive File**:
   - User pastes ticket string
   - Frontend parses ticket to extract filename (via `parse_ticket_metadata` command)
   - Save dialog opens with Downloads folder and original filename pre-filled
   - Backend parses full ticket to extract sender's node address and blob hash
   - Connects to sender via Iroh (using relay servers if needed)
   - Downloads blob with progress tracking and saves to selected path

### Key Technical Details

- **Iroh Networking**:
  - Uses in-memory blob store (`iroh::blobs::store::mem::Store`)
  - Blob provider runs in background tokio task (started via `start_blob_provider()`)
  - Direct peer-to-peer connections with automatic NAT traversal via relay servers

- **Ticket Format**: Enhanced format `filename|size|blob_ticket`
  - Backward compatible with legacy format (just blob_ticket)
  - Parsed via `parse_enhanced_ticket()` in `transfer.rs`
  - Allows receiver to get original filename before downloading

- **Android File Access**:
  - Platform-specific handling in `platform.rs`
  - Android: Uses `tauri-plugin-android-fs` to read content:// URIs
  - Desktop: Standard `tokio::fs::read()`
  - File paths from dialog are platform-specific but handled transparently

- **Progress Tracking**:
  - Transfer progress emitted via `transfer-progress` events
  - Custom `ProgressWrapper` in `receive_file()` tracks bytes written
  - Frontend listens to events and updates UI

- **Capabilities**: Defined in `src-tauri/capabilities/default.json` - must include `fs:default` for file operations

- **Logging**:
  - Desktop: uses `tauri-plugin-log`
  - Android: uses `android_logger` (initialized in `init_logging()`)
  - All logs go to stdout, log dir, and webview console

### Tauri Commands

All commands defined in `src-tauri/src/lib.rs`:

- `init_node` - initialize Iroh endpoint and blob store
- `get_node_id` - get current node ID
- `send_file(file_path)` - create send ticket with metadata
- `receive_file(ticket, output_path)` - download file from ticket
- `parse_ticket_metadata(ticket)` - extract filename/size from ticket (no download)
- `get_transfer_status(transfer_id)` - query transfer state
- `list_peers` - get discovered peers
- `get_device_name` - get system device name

TypeScript wrappers in `src/lib/api.ts` provide typed interfaces for all commands.

## Common Patterns

### Adding New Tauri Commands

1. Define Rust function with `#[tauri::command]` in `lib.rs`
2. Add to `invoke_handler!` macro
3. Create TypeScript wrapper in `src/lib/api.ts`
4. Update capabilities in `default.json` if needed

### Platform-Specific File Handling

Files selected on Android return `content://` URIs, while desktop returns standard paths. Use the centralized `platform::read_file()` helper which handles both:

```rust
// In commands that read files
let file_data = platform::read_file(&app, &file_path).await
    .map_err(|e| format!("Failed to read file: {}", e))?;
```

This function is platform-compiled and uses the appropriate API for each platform.

### State Access Pattern

```rust
#[tauri::command]
async fn my_command(state: State<'_, AppState>) -> Result<T, String> {
    let endpoint = state.get_endpoint().await
        .map_err(|e| format!("Error: {}", e))?;
    // ... use endpoint
}
```

### Working with Enhanced Ticket Format

When adding features that interact with tickets:

```rust
// Parse ticket to extract metadata
use crate::iroh::transfer::parse_enhanced_ticket;

let (filename, size, blob_ticket) = parse_enhanced_ticket(&ticket_str)?;
// Use blob_ticket for actual Iroh operations
// filename and size are for UI/metadata purposes
```

The format is backward compatible - old tickets without metadata still parse successfully with default values.

## Dependencies

### Critical Rust Crates

- `iroh = "0.26"` - P2P networking (pinned version - breaking changes expected)
- `iroh-blobs = "0.26"` - blob storage and transfer protocols
- `iroh-io` - async I/O primitives for Iroh
- `tauri-plugin-fs` - file system access (cross-platform)
- `tauri-plugin-android-fs` - Android-specific file access for content:// URIs
- `tauri-plugin-dialog` - file picker dialogs
- `redb = "1.5.2"` - embedded database (pinned version)

### Frontend

- `@tauri-apps/api` - Tauri JS bindings
- `@tauri-apps/plugin-dialog` - file picker
- `@tauri-apps/plugin-log` - logging (desktop only)
- React 19 + TypeScript + Vite
- shadcn/ui + Tailwind CSS for UI components

## Android Release Process

1. **First time setup**:

   ```bash
   make android-setup  # Creates keystore
   ```

2. **Build signed release**:

   ```bash
   export ANDROID_KEY_PASSWORD=your_password
   make android-release
   ```

3. **Output**: `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`

Note: Keep `upload-keystore.jks` secure and backed up. Add to `.gitignore`.
