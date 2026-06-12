//! Opt-in PostHog product telemetry (ADR 0014).
//!
//! OFF by default. Nothing leaves the machine unless the user enables the
//! runtime setting AND the build carries a `VEGAM_POSTHOG_API_KEY`. The
//! narrow API is the privacy mechanism: callers can only pass a whitelisted
//! event name and a coarse size bucket — file names, paths, Transfer
//! Tickets, peer IDs, IP addresses, exact sizes, and content hashes have no
//! way in.

use std::sync::Arc;

use serde::Serialize;

use crate::engine::Engine;

/// Build-time PostHog credentials; absent in normal/dev builds, so the
/// telemetry path is compiled to a no-op sender even when the setting is on.
const POSTHOG_API_KEY: Option<&str> = option_env!("VEGAM_POSTHOG_API_KEY");
const POSTHOG_HOST: &str = match option_env!("VEGAM_POSTHOG_HOST") {
    Some(host) => host,
    None => "https://us.i.posthog.com",
};

/// Product-level events only.
#[derive(Debug, Clone, Copy)]
pub enum Event {
    AppStarted,
    SendTransferCreated,
    ReceiveTransferCompleted,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Event::AppStarted => "app_started",
            Event::SendTransferCreated => "send_transfer_created",
            Event::ReceiveTransferCompleted => "receive_transfer_completed",
        }
    }
}

/// Coarse file-size bucket — the only size information telemetry may carry.
pub fn size_bucket(bytes: u64) -> &'static str {
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;
    if bytes < MB {
        "<1MB"
    } else if bytes < 100 * MB {
        "1-100MB"
    } else if bytes < GB {
        "100MB-1GB"
    } else if bytes < 10 * GB {
        "1-10GB"
    } else if bytes < 100 * GB {
        "10-100GB"
    } else {
        ">100GB"
    }
}

#[derive(Serialize)]
struct CaptureBody {
    api_key: &'static str,
    event: &'static str,
    distinct_id: String,
    properties: Properties,
}

#[derive(Serialize)]
struct Properties {
    os: &'static str,
    app_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bucket: Option<&'static str>,
}

/// Fire-and-forget capture. Drops silently when the setting is off or no
/// key is compiled in. Failures are logged at debug only.
pub fn capture(engine: &Arc<Engine>, event: Event, size: Option<u64>) {
    let enabled = engine.settings.lock().unwrap().analytics_enabled;
    if !enabled {
        return;
    }
    let Some(api_key) = POSTHOG_API_KEY else {
        tracing::debug!("telemetry enabled but no API key compiled in; dropping event");
        return;
    };
    // Anonymous random id, persisted per install; never the endpoint id.
    let distinct_id = engine.analytics_id();
    let body = CaptureBody {
        api_key,
        event: event.name(),
        distinct_id,
        properties: Properties {
            os: std::env::consts::OS,
            app_version: env!("CARGO_PKG_VERSION"),
            size_bucket: size.map(size_bucket),
        },
    };
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let result = client
            .post(format!("{POSTHOG_HOST}/capture/"))
            .json(&body)
            .send()
            .await;
        if let Err(e) = result {
            tracing::debug!("telemetry send failed: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_are_coarse() {
        assert_eq!(size_bucket(0), "<1MB");
        assert_eq!(size_bucket(1024 * 1024), "1-100MB");
        assert_eq!(size_bucket(500 * 1024 * 1024), "100MB-1GB");
        assert_eq!(size_bucket(5 * 1024 * 1024 * 1024), "1-10GB");
        assert_eq!(size_bucket(50 * 1024 * 1024 * 1024), "10-100GB");
        assert_eq!(size_bucket(200 * 1024 * 1024 * 1024), ">100GB");
    }
}
