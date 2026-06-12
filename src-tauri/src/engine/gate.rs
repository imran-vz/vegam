//! The provider gate: every incoming connection and request crosses this
//! boundary, where the engine enforces ticket expiry (ADR 0015), sender
//! pause (ADR 0018), Cancellation (ADR 0019), and content-change safety
//! (ADR 0005), and counts active Receivers (ADR 0016).
//!
//! Abort-code discipline: `Permission` (wire ERR_PERMISSION) is TERMINAL —
//! receivers map it to NoLongerResumable. `RateLimited` (wire ERR_LIMIT) is
//! TRANSIENT — receivers keep their partial state and retry slowly. Pause,
//! a missing source awaiting reselect, and in-progress (re-)imports are all
//! transient.

use std::sync::Arc;
use std::time::Duration;

use iroh_blobs::provider::events::{AbortReason, ProviderMessage, RequestUpdate};
use tokio::sync::mpsc;

use crate::engine::types::{now_unix_secs, SendStatus};
use crate::engine::{ActiveRequest, Engine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    Accept { newly_started: bool },
    Reject(RejectReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// Terminal: cancelled, content changed, or expired-for-new-receivers.
    Permission,
    /// Transient: paused, source missing (reselect pending), importing,
    /// or content re-hash pending. Try again later.
    RateLimited,
}

impl From<RejectReason> for AbortReason {
    fn from(r: RejectReason) -> Self {
        match r {
            RejectReason::Permission => AbortReason::Permission,
            RejectReason::RateLimited => AbortReason::RateLimited,
        }
    }
}

/// Pure decision function — unit-tested table-style below.
///
/// `started` is whether this requester already had a request accepted before
/// expiry (ADR 0015 lets started Receivers resume after expiry).
pub fn decide(status: SendStatus, issued_at: u64, started: bool, now: u64) -> GateDecision {
    use SendStatus::*;
    match status {
        Cancelled | ContentChanged => GateDecision::Reject(RejectReason::Permission),
        Paused | SourceMissing | Importing | ContentSuspect => {
            GateDecision::Reject(RejectReason::RateLimited)
        }
        Available | Expired => {
            let expires_at = issued_at.saturating_add(crate::engine::ticket::TICKET_TTL_SECS);
            if now < expires_at {
                GateDecision::Accept {
                    newly_started: !started,
                }
            } else if started {
                GateDecision::Accept {
                    newly_started: false,
                }
            } else {
                GateDecision::Reject(RejectReason::Permission)
            }
        }
    }
}

/// Spawn the gate task consuming provider messages.
pub fn spawn(engine: Arc<Engine>, mut rx: mpsc::Receiver<ProviderMessage>) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            handle_message(&engine, msg).await;
        }
        tracing::debug!("provider gate channel closed");
    });
}

async fn handle_message(engine: &Arc<Engine>, msg: ProviderMessage) {
    match msg {
        ProviderMessage::ClientConnected(m) => {
            let conn_id = m.inner.connection_id;
            if let Some(ep) = m.inner.endpoint_id {
                engine
                    .registry
                    .lock()
                    .unwrap()
                    .conn_endpoints
                    .insert(conn_id, ep);
            }
            // Per-Transfer policy is decided per request (the hash arrives
            // with the request); connections themselves are accepted.
            m.tx.send(Ok(())).await.ok();
        }
        ProviderMessage::ClientConnectedNotify(m) => {
            let conn_id = m.inner.connection_id;
            if let Some(ep) = m.inner.endpoint_id {
                engine
                    .registry
                    .lock()
                    .unwrap()
                    .conn_endpoints
                    .insert(conn_id, ep);
            }
        }
        ProviderMessage::ConnectionClosed(m) => {
            let conn_id = m.inner.connection_id;
            let affected: Vec<String> = {
                let mut reg = engine.registry.lock().unwrap();
                reg.conn_endpoints.remove(&conn_id);
                let keys: Vec<(u64, u64)> = reg
                    .active_requests
                    .keys()
                    .filter(|(c, _)| *c == conn_id)
                    .cloned()
                    .collect();
                let mut send_ids = Vec::new();
                for key in keys {
                    if let Some(active) = reg.active_requests.remove(&key) {
                        active.drain.abort();
                        if let Some(id) = reg.sends_by_hash.get(&active.hash) {
                            send_ids.push(id.clone());
                        }
                    }
                }
                send_ids
            };
            for id in affected {
                engine.emit_send(&id);
            }
        }
        ProviderMessage::GetRequestReceived(m) => {
            let hash = m.inner.request.hash;
            let conn_id = m.inner.connection_id;
            let req_id = m.inner.request_id;

            // Resolve transfer + requester and decide.
            let (decision, send_id, endpoint) = {
                let reg = engine.registry.lock().unwrap();
                let endpoint = reg.conn_endpoints.get(&conn_id).copied();
                match reg.sends_by_hash.get(&hash) {
                    Some(id) => match reg.sends.get(id) {
                        Some(t) => {
                            let started = endpoint
                                .map(|e| t.record.started_receivers.contains(&e.to_string()))
                                .unwrap_or(false);
                            (
                                decide(
                                    t.record.status,
                                    t.record.issued_at,
                                    started,
                                    now_unix_secs(),
                                ),
                                Some(id.clone()),
                                endpoint,
                            )
                        }
                        None => (
                            GateDecision::Reject(RejectReason::Permission),
                            None,
                            endpoint,
                        ),
                    },
                    // Unknown hash: also stops this Device from re-serving
                    // blobs it merely received as a Receiver.
                    None => (
                        GateDecision::Reject(RejectReason::Permission),
                        None,
                        endpoint,
                    ),
                }
            };

            match decision {
                GateDecision::Reject(reason) => {
                    m.tx.send(Err(reason.into())).await.ok();
                }
                GateDecision::Accept { newly_started } => {
                    // Record the started receiver BEFORE replying so a crash
                    // cannot lose the resume-after-expiry entitlement.
                    if newly_started {
                        if let (Some(id), Some(ep)) = (&send_id, endpoint) {
                            {
                                let mut reg = engine.registry.lock().unwrap();
                                if let Some(t) = reg.sends.get_mut(id) {
                                    t.record.started_receivers.insert(ep.to_string());
                                }
                            }
                            if let Err(e) = engine.persist() {
                                tracing::warn!("persisting started receiver failed: {e}");
                            }
                        }
                    }
                    m.tx.send(Ok(())).await.ok();

                    // Drain the update stream (capacity-32 channel MUST be
                    // consumed or the provider stalls). Aborting this task
                    // drops the receiver, which aborts the in-flight send —
                    // the mechanism behind sender pause/cancel.
                    let drain_engine = engine.clone();
                    let drain =
                        tokio::spawn(drain_request_updates(drain_engine, m.rx, conn_id, req_id));
                    {
                        let mut reg = engine.registry.lock().unwrap();
                        reg.active_requests.insert(
                            (conn_id, req_id),
                            ActiveRequest {
                                hash,
                                endpoint,
                                drain: drain.abort_handle(),
                            },
                        );
                    }
                    if let Some(id) = send_id {
                        engine.emit_send(&id);
                    }
                }
            }
        }
        // In iroh-blobs 0.102 every request type dispatches on `mask.get`,
        // so these variants can arrive despite get_many/push being Disabled
        // in the mask. Reject them all explicitly: Vegam serves single-blob
        // Get requests only.
        ProviderMessage::GetManyRequestReceived(m) => {
            m.tx.send(Err(AbortReason::Permission)).await.ok();
        }
        ProviderMessage::PushRequestReceived(m) => {
            m.tx.send(Err(AbortReason::Permission)).await.ok();
        }
        ProviderMessage::ObserveRequestReceived(m) => {
            m.tx.send(Err(AbortReason::Permission)).await.ok();
        }
        ProviderMessage::GetRequestReceivedNotify(_)
        | ProviderMessage::GetManyRequestReceivedNotify(_)
        | ProviderMessage::PushRequestReceivedNotify(_)
        | ProviderMessage::ObserveRequestReceivedNotify(_) => {
            // Notify-mode variants cannot occur with our Intercept mask.
        }
        ProviderMessage::Throttle(m) => {
            // ThrottleMode::None means these should not arrive; answer
            // permissively to avoid stalling a future mask change.
            m.tx.send(Ok(())).await.ok();
        }
    }
}

/// Consume a request's update stream until it ends, then unregister the
/// request and refresh the Sender's receiver count.
async fn drain_request_updates(
    engine: Arc<Engine>,
    mut rx: irpc::channel::mpsc::Receiver<RequestUpdate>,
    conn_id: u64,
    req_id: u64,
) {
    while let Ok(Some(_update)) = rx.recv().await {
        // Transfer events are not individually interesting yet; the stream
        // must simply be drained.
    }
    let send_id = {
        let mut reg = engine.registry.lock().unwrap();
        reg.active_requests
            .remove(&(conn_id, req_id))
            .and_then(|active| reg.sends_by_hash.get(&active.hash).cloned())
    };
    if let Some(id) = send_id {
        engine.emit_send(&id);
    }
}

/// Abort all in-flight provider requests for a hash (sender pause/cancel).
pub fn abort_requests_for_hash(engine: &Engine, hash: &iroh_blobs::Hash) {
    let mut reg = engine.registry.lock().unwrap();
    let keys: Vec<(u64, u64)> = reg
        .active_requests
        .iter()
        .filter(|(_, a)| &a.hash == hash)
        .map(|(k, _)| *k)
        .collect();
    for key in keys {
        if let Some(active) = reg.active_requests.remove(&key) {
            active.drain.abort();
        }
    }
}

/// Periodic maintenance: flip Available→Expired for UI truthfulness (the
/// gate checks the wall clock exactly, so enforcement never depends on this)
/// and watch send sources for content drift.
pub fn spawn_sweepers(engine: Arc<Engine>) {
    let expiry_engine = engine.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let now = now_unix_secs();
            let flipped: Vec<String> = {
                let mut reg = expiry_engine.registry.lock().unwrap();
                let mut ids = Vec::new();
                for (id, t) in reg.sends.iter_mut() {
                    if t.record.status == SendStatus::Available
                        && now
                            >= t.record
                                .issued_at
                                .saturating_add(crate::engine::ticket::TICKET_TTL_SECS)
                    {
                        t.record.status = SendStatus::Expired;
                        ids.push(id.clone());
                    }
                }
                ids
            };
            if !flipped.is_empty() {
                if let Err(e) = expiry_engine.persist() {
                    tracing::warn!("persisting expiry sweep failed: {e}");
                }
                for id in flipped {
                    expiry_engine.emit_send(&id);
                }
            }
        }
    });

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            crate::engine::send::sweep_send_sources(&engine).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ticket::TICKET_TTL_SECS;

    const ISSUED: u64 = 1_000_000;
    const BEFORE_EXPIRY: u64 = ISSUED + 10;
    const AFTER_EXPIRY: u64 = ISSUED + TICKET_TTL_SECS + 10;

    #[test]
    fn decision_table() {
        use GateDecision::*;
        use RejectReason::*;
        use SendStatus::*;
        let cases: &[(SendStatus, u64, bool, GateDecision)] = &[
            // Fresh ticket, new receiver: accept and mark started.
            (
                Available,
                BEFORE_EXPIRY,
                false,
                Accept {
                    newly_started: true,
                },
            ),
            // Fresh ticket, known receiver.
            (
                Available,
                BEFORE_EXPIRY,
                true,
                Accept {
                    newly_started: false,
                },
            ),
            // Expired: new receivers rejected terminally…
            (Available, AFTER_EXPIRY, false, Reject(Permission)),
            (Expired, AFTER_EXPIRY, false, Reject(Permission)),
            // …but started receivers may resume (ADR 0015).
            (
                Available,
                AFTER_EXPIRY,
                true,
                Accept {
                    newly_started: false,
                },
            ),
            (
                Expired,
                AFTER_EXPIRY,
                true,
                Accept {
                    newly_started: false,
                },
            ),
            // The Expired label is cosmetic: clock says valid → accept.
            (
                Expired,
                BEFORE_EXPIRY,
                false,
                Accept {
                    newly_started: true,
                },
            ),
            // Transient states: RateLimited regardless of receiver history.
            (Paused, BEFORE_EXPIRY, false, Reject(RateLimited)),
            (Paused, BEFORE_EXPIRY, true, Reject(RateLimited)),
            (SourceMissing, BEFORE_EXPIRY, true, Reject(RateLimited)),
            (Importing, BEFORE_EXPIRY, false, Reject(RateLimited)),
            (ContentSuspect, BEFORE_EXPIRY, true, Reject(RateLimited)),
            // Terminal states: Permission regardless of receiver history.
            (Cancelled, BEFORE_EXPIRY, true, Reject(Permission)),
            (ContentChanged, BEFORE_EXPIRY, true, Reject(Permission)),
        ];
        for (status, now, started, expected) in cases {
            let got = decide(*status, ISSUED, *started, *now);
            assert_eq!(
                got, *expected,
                "decide({status:?}, started={started}, now={now})"
            );
        }
    }

    #[test]
    fn expiry_boundary_is_exact() {
        let at_expiry = ISSUED + TICKET_TTL_SECS;
        assert_eq!(
            decide(SendStatus::Available, ISSUED, false, at_expiry - 1),
            GateDecision::Accept {
                newly_started: true
            }
        );
        assert_eq!(
            decide(SendStatus::Available, ISSUED, false, at_expiry),
            GateDecision::Reject(RejectReason::Permission)
        );
    }
}
