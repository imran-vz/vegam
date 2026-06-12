//! Phase 3 acceptance tests: two Engines in one process, talking over real
//! iroh endpoints via local direct addresses (no public relay dependency).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use iroh::RelayMode;
use vegam_lib::engine::types::{ReceiveStatus, SendStatus};
use vegam_lib::engine::{Engine, Tuning};

fn test_tuning() -> Tuning {
    Tuning {
        relay_mode: RelayMode::Disabled,
        stall_retry: Duration::from_millis(500),
        gc_interval: Duration::from_millis(500),
    }
}

struct TestDirs {
    root: PathBuf,
}

impl TestDirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("vegam-it-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn engine_dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for TestDirs {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn write_random_file(path: &Path, len: usize) {
    use rand::RngCore;
    let mut data = vec![0u8; len];
    rand::rng().fill_bytes(&mut data);
    std::fs::write(path, data).unwrap();
}

async fn wait_for_send_status(
    engine: &Arc<Engine>,
    id: &str,
    want: SendStatus,
    timeout: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status = {
            let reg = engine.registry.lock().unwrap();
            reg.sends.get(id).map(|t| t.record.status)
        };
        if status == Some(want) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait until the receive transfer reaches `want`, or — for terminal-success
/// flows where the record is pruned — until it disappears with `want` being
/// Complete/Cancelled.
async fn wait_for_receive_status(
    engine: &Arc<Engine>,
    id: &str,
    want: ReceiveStatus,
    timeout: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status = {
            let reg = engine.registry.lock().unwrap();
            reg.receives.get(id).map(|e| e.record.status)
        };
        match status {
            Some(s) if s == want => return true,
            None if matches!(want, ReceiveStatus::Complete | ReceiveStatus::Cancelled) => {
                // Pruned after reaching the terminal state.
                return true;
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn create_available_send(sender: &Arc<Engine>, path: &Path) -> (String, String) {
    let info =
        vegam_lib::engine::send::create_send_transfer(sender, path.to_string_lossy().to_string())
            .await
            .expect("create send");
    assert!(
        wait_for_send_status(
            sender,
            &info.id,
            SendStatus::Available,
            Duration::from_secs(30)
        )
        .await,
        "send never became available"
    );
    let ticket = {
        let reg = sender.registry.lock().unwrap();
        reg.sends.get(&info.id).unwrap().record.ticket.clone()
    };
    (info.id, ticket)
}

#[tokio::test(flavor = "multi_thread")]
async fn happy_path_byte_identical() {
    let dirs = TestDirs::new("happy");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 4 * 1024 * 1024);
    let (_, ticket) = create_available_send(&sender, &source).await;

    let dest = dirs.file("dest.bin");
    let info = vegam_lib::engine::recv::create_receive_transfer(
        &receiver,
        ticket,
        dest.to_string_lossy().to_string(),
    )
    .await
    .expect("create receive");

    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await,
        "receive never completed"
    );
    let src_bytes = std::fs::read(&source).unwrap();
    let dst_bytes = std::fs::read(&dest).unwrap();
    assert_eq!(src_bytes, dst_bytes, "destination differs from source");

    // No history: the completed receive record is pruned (ADR 0009).
    {
        let reg = receiver.registry.lock().unwrap();
        assert!(reg.receives.is_empty());
    }

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sender_pause_stalls_receiver_resume_completes() {
    let dirs = TestDirs::new("pause");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 4 * 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;

    // Pause BEFORE the receiver starts: deterministic stall.
    vegam_lib::engine::send::pause_send_transfer(&sender, &send_id).expect("pause send");

    let dest = dirs.file("dest.bin");
    let info = vegam_lib::engine::recv::create_receive_transfer(
        &receiver,
        ticket,
        dest.to_string_lossy().to_string(),
    )
    .await
    .expect("create receive");

    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::StalledRetrying,
            Duration::from_secs(30)
        )
        .await,
        "receiver never stalled on the paused sender"
    );
    assert!(!dest.exists(), "destination written while sender paused");

    vegam_lib::engine::send::resume_send_transfer(&sender, &send_id).expect("resume send");

    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await,
        "receive never completed after sender resume"
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        std::fs::read(&dest).unwrap()
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn expiry_rejects_new_but_admits_started_receiver() {
    let dirs = TestDirs::new("expiry");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;

    // Backdate issuance past the 24h TTL.
    {
        let mut reg = sender.registry.lock().unwrap();
        let t = reg.sends.get_mut(&send_id).unwrap();
        t.record.issued_at = t
            .record
            .issued_at
            .saturating_sub(vegam_lib::engine::ticket::TICKET_TTL_SECS + 60);
    }
    sender.persist().unwrap();

    // A NEW receiver must be rejected terminally.
    let dest = dirs.file("dest.bin");
    let info = vegam_lib::engine::recv::create_receive_transfer(
        &receiver,
        ticket,
        dest.to_string_lossy().to_string(),
    )
    .await
    .expect("create receive");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::NoLongerResumable,
            Duration::from_secs(30)
        )
        .await,
        "new receiver was not rejected after expiry"
    );
    assert!(!dest.exists());

    // Seed the receiver as already-started (as if it began before expiry),
    // then a user-initiated resume must succeed (ADR 0015).
    {
        let mut reg = sender.registry.lock().unwrap();
        let t = reg.sends.get_mut(&send_id).unwrap();
        t.record
            .started_receivers
            .insert(receiver.endpoint.id().to_string());
    }
    sender.persist().unwrap();

    vegam_lib::engine::recv::resume_receive_transfer(&receiver, &info.id).expect("resume");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await,
        "started receiver could not resume after expiry"
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        std::fs::read(&dest).unwrap()
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn content_change_is_terminal_after_rehash() {
    let dirs = TestDirs::new("content");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 1024 * 1024);
    let (send_id, _ticket) = create_available_send(&sender, &source).await;

    // Overwrite with different content (also bumps len/mtime).
    write_random_file(&source, 1024 * 1024 + 17);
    vegam_lib::engine::send::sweep_send_sources(&sender).await;

    assert!(
        wait_for_send_status(
            &sender,
            &send_id,
            SendStatus::ContentChanged,
            Duration::from_secs(30)
        )
        .await,
        "content change was not confirmed by re-hash"
    );

    sender.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mtime_only_touch_keeps_transfer_available() {
    let dirs = TestDirs::new("touch");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 1024 * 1024);
    let (send_id, _ticket) = create_available_send(&sender, &source).await;

    // Rewrite identical content: mtime changes, Content Identity does not.
    let bytes = std::fs::read(&source).unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await; // ensure mtime moves
    std::fs::write(&source, &bytes).unwrap();
    vegam_lib::engine::send::sweep_send_sources(&sender).await;

    // The re-hash must restore Available (ADR 0005: mtime is not identity).
    assert!(
        wait_for_send_status(
            &sender,
            &send_id,
            SendStatus::Available,
            Duration::from_secs(30)
        )
        .await,
        "mtime-only churn killed the transfer"
    );

    sender.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn moved_file_reselect_and_continue() {
    let dirs = TestDirs::new("reselect");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 4 * 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;

    // Hold the receiver in a stall so the move happens mid-Transfer.
    vegam_lib::engine::send::pause_send_transfer(&sender, &send_id).expect("pause");
    let dest = dirs.file("dest.bin");
    let info = vegam_lib::engine::recv::create_receive_transfer(
        &receiver,
        ticket,
        dest.to_string_lossy().to_string(),
    )
    .await
    .expect("create receive");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::StalledRetrying,
            Duration::from_secs(30)
        )
        .await
    );

    // Move the source; the sweeper marks it missing.
    let moved = dirs.file("moved.bin");
    std::fs::rename(&source, &moved).unwrap();
    vegam_lib::engine::send::sweep_send_sources(&sender).await;
    assert!(
        wait_for_send_status(
            &sender,
            &send_id,
            SendStatus::SourceMissing,
            Duration::from_secs(10)
        )
        .await,
        "moved source not detected"
    );

    // Reselect with a WRONG file first: must be refused (Content Identity).
    let wrong = dirs.file("wrong.bin");
    write_random_file(&wrong, 4 * 1024 * 1024);
    let err = vegam_lib::engine::send::reselect_send_source(
        &sender,
        &send_id,
        wrong.to_string_lossy().to_string(),
    )
    .await
    .expect_err("wrong file accepted");
    assert_eq!(
        err.code,
        vegam_lib::engine::error::ErrorCode::ContentMismatch
    );

    // Reselect with the moved file: hash matches, transfer continues with
    // the SAME ticket (ADR 0005).
    vegam_lib::engine::send::reselect_send_source(
        &sender,
        &send_id,
        moved.to_string_lossy().to_string(),
    )
    .await
    .expect("reselect with matching content");

    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await,
        "receiver did not complete after reselect"
    );
    assert_eq!(
        std::fs::read(&moved).unwrap(),
        std::fs::read(&dest).unwrap()
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_restart_resumes_from_partial_state() {
    let dirs = TestDirs::new("restart");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 16 * 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;

    // Phase 1 of the receive: run against a paused sender so it parks in
    // StalledRetrying deterministically, then "restart the app".
    vegam_lib::engine::send::pause_send_transfer(&sender, &send_id).expect("pause");
    let receiver_dir = dirs.engine_dir("b");
    let dest = dirs.file("dest.bin");
    let recv_id = {
        let receiver = Engine::init_with_tuning(receiver_dir.clone(), test_tuning())
            .await
            .unwrap();
        let info = vegam_lib::engine::recv::create_receive_transfer(
            &receiver,
            ticket,
            dest.to_string_lossy().to_string(),
        )
        .await
        .expect("create receive");
        assert!(
            wait_for_receive_status(
                &receiver,
                &info.id,
                ReceiveStatus::StalledRetrying,
                Duration::from_secs(30)
            )
            .await
        );
        receiver.shutdown().await;
        info.id
    };

    // Resume the sender, then "relaunch" the receiver from the same root:
    // the active record must auto-resume and complete.
    vegam_lib::engine::send::resume_send_transfer(&sender, &send_id).expect("resume");
    let receiver = Engine::init_with_tuning(receiver_dir, test_tuning())
        .await
        .unwrap();
    {
        let reg = receiver.registry.lock().unwrap();
        assert!(
            reg.receives.contains_key(&recv_id),
            "receive record lost across restart"
        );
    }
    assert!(
        wait_for_receive_status(
            &receiver,
            &recv_id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await,
        "receive did not complete after restart"
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        std::fs::read(&dest).unwrap()
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_cancel_removes_record_and_partial_state() {
    let dirs = TestDirs::new("cancel");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;
    vegam_lib::engine::send::pause_send_transfer(&sender, &send_id).expect("pause");

    let dest = dirs.file("dest.bin");
    let info = vegam_lib::engine::recv::create_receive_transfer(
        &receiver,
        ticket,
        dest.to_string_lossy().to_string(),
    )
    .await
    .expect("create receive");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::StalledRetrying,
            Duration::from_secs(30)
        )
        .await
    );

    vegam_lib::engine::recv::cancel_receive_transfer(&receiver, &info.id)
        .await
        .expect("cancel");

    {
        let reg = receiver.registry.lock().unwrap();
        assert!(reg.receives.is_empty(), "record not removed on cancel");
    }
    assert!(!dest.exists(), "destination touched by a cancelled receive");
    // The resume-area report shows no tracked entries for it.
    let report = vegam_lib::engine::resume_area::list_partial_downloads(&receiver)
        .await
        .unwrap();
    assert!(
        report.entries.iter().all(|e| e.entry_id != info.id),
        "cancelled transfer still listed in the resume area"
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn paused_receive_survives_restart_and_resumes() {
    let dirs = TestDirs::new("pausedrestart");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 4 * 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;
    // Hold the receiver in a stall so we can pause it deterministically.
    vegam_lib::engine::send::pause_send_transfer(&sender, &send_id).expect("pause send");

    let receiver_dir = dirs.engine_dir("b");
    let dest = dirs.file("dest.bin");
    let recv_id = {
        let receiver = Engine::init_with_tuning(receiver_dir.clone(), test_tuning())
            .await
            .unwrap();
        let info = vegam_lib::engine::recv::create_receive_transfer(
            &receiver,
            ticket,
            dest.to_string_lossy().to_string(),
        )
        .await
        .expect("create receive");
        assert!(
            wait_for_receive_status(
                &receiver,
                &info.id,
                ReceiveStatus::StalledRetrying,
                Duration::from_secs(30)
            )
            .await
        );
        // User pauses, then "quits the app".
        vegam_lib::engine::recv::pause_receive_transfer(&receiver, &info.id).expect("pause recv");
        assert!(
            wait_for_receive_status(
                &receiver,
                &info.id,
                ReceiveStatus::Paused,
                Duration::from_secs(10)
            )
            .await
        );
        receiver.shutdown().await;
        info.id
    };

    vegam_lib::engine::send::resume_send_transfer(&sender, &send_id).expect("resume send");

    // Relaunch: the paused record must stay paused, and a user resume must
    // actually restart the download (regression test for the restored-pause
    // dead-channel blocker).
    let receiver = Engine::init_with_tuning(receiver_dir, test_tuning())
        .await
        .unwrap();
    {
        let reg = receiver.registry.lock().unwrap();
        let entry = reg.receives.get(&recv_id).expect("record survived restart");
        assert_eq!(entry.record.status, ReceiveStatus::Paused);
    }
    vegam_lib::engine::recv::resume_receive_transfer(&receiver, &recv_id).expect("resume recv");
    assert!(
        wait_for_receive_status(
            &receiver,
            &recv_id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await,
        "paused-then-restarted receive did not complete after resume"
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        std::fs::read(&dest).unwrap()
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_manual_pause_and_resume() {
    let dirs = TestDirs::new("recvpause");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 4 * 1024 * 1024);
    let (send_id, ticket) = create_available_send(&sender, &source).await;
    vegam_lib::engine::send::pause_send_transfer(&sender, &send_id).expect("pause send");

    let dest = dirs.file("dest.bin");
    let info = vegam_lib::engine::recv::create_receive_transfer(
        &receiver,
        ticket,
        dest.to_string_lossy().to_string(),
    )
    .await
    .expect("create receive");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::StalledRetrying,
            Duration::from_secs(30)
        )
        .await
    );

    vegam_lib::engine::recv::pause_receive_transfer(&receiver, &info.id).expect("pause");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::Paused,
            Duration::from_secs(10)
        )
        .await,
        "receiver never reported Paused"
    );
    assert!(!dest.exists());

    // Sender comes back while the receiver stays paused: still paused.
    vegam_lib::engine::send::resume_send_transfer(&sender, &send_id).expect("resume send");
    tokio::time::sleep(Duration::from_secs(2)).await;
    {
        let reg = receiver.registry.lock().unwrap();
        assert_eq!(
            reg.receives.get(&info.id).unwrap().record.status,
            ReceiveStatus::Paused,
            "paused receiver started downloading on its own"
        );
    }

    vegam_lib::engine::recv::resume_receive_transfer(&receiver, &info.id).expect("resume");
    assert!(
        wait_for_receive_status(
            &receiver,
            &info.id,
            ReceiveStatus::Complete,
            Duration::from_secs(60)
        )
        .await
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        std::fs::read(&dest).unwrap()
    );

    sender.shutdown().await;
    receiver.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn gate_rejects_get_many_requests() {
    use iroh_blobs::protocol::GetManyRequest;

    let dirs = TestDirs::new("getmany");
    let sender = Engine::init_with_tuning(dirs.engine_dir("a"), test_tuning())
        .await
        .unwrap();
    let receiver = Engine::init_with_tuning(dirs.engine_dir("b"), test_tuning())
        .await
        .unwrap();

    let source = dirs.file("source.bin");
    write_random_file(&source, 1024 * 1024);
    let (send_id, ticket_str) = create_available_send(&sender, &source).await;
    let hash = {
        let reg = sender.registry.lock().unwrap();
        reg.sends.get(&send_id).unwrap().hash
    };

    // Issue a raw GetMany for the same content: the gate must reject it
    // even though a plain Get would be accepted.
    let ticket: vegam_lib::engine::ticket::VegamTicket = ticket_str.parse().unwrap();
    let conn = receiver
        .endpoint
        .connect(ticket.blob.addr().clone(), iroh_blobs::protocol::ALPN)
        .await
        .expect("connect");
    let request = GetManyRequest::builder()
        .hash(hash, iroh_blobs::protocol::ChunkRanges::all())
        .build();
    let get = receiver.store.remote().execute_get_many(conn, request);
    let mut stream = get.stream();
    let mut failed = false;
    while let Some(item) = n0_future::StreamExt::next(&mut stream).await {
        match item {
            iroh_blobs::api::remote::GetProgressItem::Error(_) => {
                failed = true;
                break;
            }
            iroh_blobs::api::remote::GetProgressItem::Done(_) => break,
            _ => {}
        }
    }
    assert!(failed, "GetMany request was not rejected by the gate");

    sender.shutdown().await;
    receiver.shutdown().await;
}
