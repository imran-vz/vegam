use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::engine::Engine;

/// Tauri-managed app state: just the engine handle, set once by `init_app`.
pub struct AppState {
    engine: RwLock<Option<Arc<Engine>>>,
    /// Serializes `init_app` so concurrent calls (e.g. React StrictMode
    /// double-effects in dev) cannot run two Engine::init on one data dir.
    pub init_lock: Mutex<()>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            engine: RwLock::new(None),
            init_lock: Mutex::new(()),
        }
    }

    pub async fn engine(&self) -> Option<Arc<Engine>> {
        self.engine.read().await.clone()
    }

    pub async fn set_engine(&self, engine: Arc<Engine>) {
        let mut guard = self.engine.write().await;
        *guard = Some(engine);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
