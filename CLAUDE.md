# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Vegam is a P2P file transfer app built with Tauri v2, React, TypeScript, and Iroh. It enables direct file transfers between devices using the Iroh networking library for NAT traversal and peer discovery.

## Build Commands

### Desktop Development

```bash
pnpm dev                    # Start dev server (frontend + Tauri)
pnpm build                  # Build production app
cargo check                 # Check Rust code (run in src-tauri/)
```

### Android Development

```bash
pnpm run tauri android build    # Build Android APK
pnpm run tauri android dev      # Run on Android device/emulator
```

### Testing

```bash
cargo test                  # Run Rust tests (run in src-tauri/)
cargo check                 # Verify compilation
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

### Data Flow

1. **Initialization**:
   - Frontend calls `initNode()`
   - Backend creates Iroh `Endpoint` with relay servers for NAT traversal
   - Returns node ID to display

2. **Send File**:
   - User selects file via Tauri dialog (returns content URI on Android, file path on desktop)
   - Backend reads file using `tauri-plugin-fs` (handles content URIs automatically)
   - Imports file to Iroh blob store
   - Creates `BlobTicket` with node address and blob hash
   - Returns ticket string to share

3. **Receive File**:
   - User pastes ticket string
   - Backend parses ticket to extract sender's node address and blob hash
   - Connects to sender via Iroh (using relay servers if needed)
   - Downloads blob and saves to output path

### Key Technical Details

- **Iroh Networking**: Uses in-memory blob store (`iroh::blobs::store::mem::Store`)
- **NAT Traversal**: Iroh endpoint automatically uses public relay servers
- **Android File Access**: `tauri-plugin-fs` handles content URIs transparently - standard `tokio::fs` calls work
- **Capabilities**: Defined in `src-tauri/capabilities/default.json` - must include `fs:default` for file operations
- **Platform Differences**:
  - Desktop: uses `tauri-plugin-log`
  - Android: uses `android_logger` (initialized in `init_logging()`)

### Tauri Commands

All commands defined in `src-tauri/src/lib.rs`:

- `init_node` - initialize Iroh endpoint
- `get_node_id` - get current node ID
- `send_file(file_path)` - create send ticket
- `receive_file(ticket, output_path)` - download file
- `get_transfer_status(transfer_id)` - query transfer state
- `list_peers` - get discovered peers
- `get_device_name` - get system device name

## Common Patterns

### Adding New Tauri Commands

1. Define Rust function with `#[tauri::command]` in `lib.rs`
2. Add to `invoke_handler!` macro
3. Create TypeScript wrapper in `src/lib/api.ts`
4. Update capabilities in `default.json` if needed

### Android Content URI Handling

Files selected on Android return `content://` URIs. The `tauri-plugin-fs` automatically resolves these when using `tokio::fs` APIs - no special handling needed in application code.

### State Access Pattern

```rust
#[tauri::command]
async fn my_command(state: State<'_, AppState>) -> Result<T, String> {
    let endpoint = state.get_endpoint().await
        .map_err(|e| format!("Error: {}", e))?;
    // ... use endpoint
}
```

## Dependencies

### Critical Rust Crates

- `iroh = "0.26"` - P2P networking (note: pinned version)
- `iroh-blobs = "0.26"` - blob storage
- `tauri-plugin-fs` - file system access (handles Android content URIs)
- `tauri-plugin-dialog` - file picker
- `redb = "1.5.2"` - embedded database (pinned version)

### Frontend

- `@tauri-apps/api` - Tauri JS bindings
- `@tauri-apps/plugin-dialog` - file dialogs
- React 19 + TypeScript + Vite
