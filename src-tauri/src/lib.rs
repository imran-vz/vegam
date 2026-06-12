pub mod engine;
mod state;

use std::str::FromStr;
use std::sync::Arc;

use tauri::{Emitter, Manager, State};
use tauri_plugin_log::{log, Target, TargetKind};

use engine::error::ApiError;
use engine::ticket::VegamTicket;
use engine::types::{
    AppSnapshot, ReceiveTransferInfo, ResumeAreaReport, SendTransferInfo, Settings, TicketPreview,
};
use engine::Engine;
use state::AppState;

async fn get_engine(state: &State<'_, AppState>) -> Result<Arc<Engine>, ApiError> {
    state
        .engine()
        .await
        .ok_or_else(|| ApiError::internal("engine not initialized"))
}

#[tauri::command]
async fn init_app(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AppSnapshot, ApiError> {
    // Serialize concurrent init calls; the second caller sees the engine.
    let _init_guard = state.init_lock.lock().await;
    if let Some(engine) = state.engine().await {
        return Ok(engine.snapshot());
    }

    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|e| ApiError::internal(format!("no app data dir: {e}")))?;
    let engine = Engine::init(root)
        .await
        .map_err(|e| ApiError::internal(format!("engine init failed: {e}")))?;

    // Forward engine events to the UI.
    let mut rx = engine.events.subscribe();
    let forward_app = app.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let name = event.event_name();
                    let result = match event {
                        engine::events::EngineEvent::SendTransferUpdated(info) => {
                            forward_app.emit(name, info)
                        }
                        engine::events::EngineEvent::ReceiveTransferUpdated(info) => {
                            forward_app.emit(name, info)
                        }
                        engine::events::EngineEvent::ReceiveTransferProgress(p) => {
                            forward_app.emit(name, p)
                        }
                    };
                    if let Err(e) = result {
                        tracing::warn!("emitting event failed: {e}");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let snapshot = engine.snapshot();
    engine::telemetry::capture(&engine, engine::telemetry::Event::AppStarted, None);
    state.set_engine(engine).await;
    Ok(snapshot)
}

#[tauri::command]
async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, ApiError> {
    Ok(get_engine(&state).await?.snapshot())
}

#[tauri::command]
async fn create_send_transfer(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<SendTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::send::create_send_transfer(&engine, file_path).await
}

#[tauri::command]
async fn pause_send_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<SendTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::send::pause_send_transfer(&engine, &transfer_id)
}

#[tauri::command]
async fn resume_send_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<SendTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::send::resume_send_transfer(&engine, &transfer_id)
}

#[tauri::command]
async fn cancel_send_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), ApiError> {
    let engine = get_engine(&state).await?;
    engine::send::cancel_send_transfer(&engine, &transfer_id).await
}

#[tauri::command]
async fn reselect_send_source(
    state: State<'_, AppState>,
    transfer_id: String,
    file_path: String,
) -> Result<SendTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::send::reselect_send_source(&engine, &transfer_id, file_path).await
}

/// Pure parse — no network, no state.
#[tauri::command]
fn inspect_ticket(ticket: String) -> Result<TicketPreview, ApiError> {
    let parsed = VegamTicket::from_str(&ticket)
        .map_err(|e| ApiError::ticket_invalid(format!("not a valid Vegam ticket: {e}")))?;
    Ok(parsed.preview())
}

#[tauri::command]
async fn create_receive_transfer(
    state: State<'_, AppState>,
    ticket: String,
    destination_path: String,
) -> Result<ReceiveTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::recv::create_receive_transfer(&engine, ticket, destination_path).await
}

#[tauri::command]
async fn pause_receive_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<ReceiveTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::recv::pause_receive_transfer(&engine, &transfer_id)
}

#[tauri::command]
async fn resume_receive_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<ReceiveTransferInfo, ApiError> {
    let engine = get_engine(&state).await?;
    engine::recv::resume_receive_transfer(&engine, &transfer_id)
}

#[tauri::command]
async fn cancel_receive_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), ApiError> {
    let engine = get_engine(&state).await?;
    engine::recv::cancel_receive_transfer(&engine, &transfer_id).await
}

#[tauri::command]
async fn list_partial_downloads(state: State<'_, AppState>) -> Result<ResumeAreaReport, ApiError> {
    let engine = get_engine(&state).await?;
    engine::resume_area::list_partial_downloads(&engine).await
}

#[tauri::command]
async fn cleanup_partial_download(
    state: State<'_, AppState>,
    entry_id: String,
) -> Result<ResumeAreaReport, ApiError> {
    let engine = get_engine(&state).await?;
    engine::resume_area::cleanup_partial_download(&engine, entry_id).await
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings, ApiError> {
    let engine = get_engine(&state).await?;
    let settings = engine.settings.lock().unwrap().clone();
    Ok(settings)
}

#[tauri::command]
async fn set_analytics_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Settings, ApiError> {
    let engine = get_engine(&state).await?;
    // Clone+save under the lock so concurrent settings writes cannot
    // persist out of order (mirrors Engine::persist's discipline).
    let settings = {
        let mut guard = engine.settings.lock().unwrap();
        guard.analytics_enabled = enabled;
        let snapshot = guard.clone();
        engine::settings::save(&engine.paths.settings(), &snapshot)
            .map_err(|e| ApiError::io(format!("saving settings failed: {e}")))?;
        snapshot
    };
    Ok(settings)
}

/// User-initiated diagnostics export (ADR 0014): copies the local log file
/// to a destination the user chose. Nothing is exported automatically.
#[tauri::command]
async fn export_diagnostics(
    app: tauri::AppHandle,
    destination_path: String,
) -> Result<(), ApiError> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| ApiError::internal(format!("no log dir: {e}")))?;
    // Newest .log file in the app log dir.
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    let entries =
        std::fs::read_dir(&log_dir).map_err(|e| ApiError::io(format!("reading logs: {e}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "log").unwrap_or(false) {
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                    newest = Some((modified, path));
                }
            }
        }
    }
    let (_, source) = newest.ok_or_else(|| ApiError::not_found("no log file found to export"))?;
    std::fs::copy(&source, std::path::Path::new(&destination_path))
        .map_err(|e| ApiError::io(format!("copying log file: {e}")))?;
    Ok(())
}

#[tauri::command]
async fn set_display_name(
    state: State<'_, AppState>,
    display_name: String,
) -> Result<Settings, ApiError> {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::new(
            engine::error::ErrorCode::Rejected,
            "display name cannot be empty",
        ));
    }
    let engine = get_engine(&state).await?;
    let settings = {
        let mut guard = engine.settings.lock().unwrap();
        guard.display_name = trimmed.to_string();
        let snapshot = guard.clone();
        engine::settings::save(&engine.paths.settings(), &snapshot)
            .map_err(|e| ApiError::io(format!("saving settings failed: {e}")))?;
        snapshot
    };
    Ok(settings)
}

pub fn run() {
    let app_state = AppState::new();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                // Privacy default (ADR 0014): persist ONLY Vegam's own log
                // targets. Third-party crates (including iroh internals) can
                // carry peer addresses in error messages, and webview-
                // originated records can embed backend error chains; neither
                // lands in local logs by default.
                .filter(|metadata| metadata.target().starts_with("vegam_lib"))
                // Keep enough history for support: the default (40KB,
                // delete-on-rotate) destroys the evidence on the restart
                // that usually follows a bug.
                .max_file_size(10 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .build(),
        );

    builder
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            init_app,
            get_app_snapshot,
            create_send_transfer,
            pause_send_transfer,
            resume_send_transfer,
            cancel_send_transfer,
            reselect_send_source,
            inspect_ticket,
            create_receive_transfer,
            pause_receive_transfer,
            resume_receive_transfer,
            cancel_receive_transfer,
            list_partial_downloads,
            cleanup_partial_download,
            get_settings,
            set_display_name,
            set_analytics_enabled,
            export_diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                if let Some(engine) = tauri::async_runtime::block_on(state.engine()) {
                    tauri::async_runtime::block_on(engine.shutdown());
                }
            }
        });
}
