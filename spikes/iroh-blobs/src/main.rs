//! Phase 2 production spike for the Vegam Desktop Release (ADR 0021).
//!
//! Proves whether `iroh = 1.0.0` + `iroh-blobs = 0.103` can carry the v1
//! transfer bar: 100 GB single-file Transfers, bounded memory, Content
//! Identity verification, resume after network loss and app restart, manual
//! pause/resume, Partial Downloads in a managed resume area, safe failure on
//! Sender content change, and Direct-vs-Relayed reporting.
//!
//! Machine-readable lines on stdout (one `KEY: value` per line) so the spike
//! harness can assert on them. Human chatter goes to stderr.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use iroh::{endpoint::presets, Endpoint, RelayMode, SecretKey, TransportAddr};
use iroh_blobs::{
    api::{
        blobs::{
            AddPathOptions, AddProgressItem, ExportMode, ExportOptions, ExportProgressItem,
            ImportMode,
        },
        remote::GetProgressItem,
        Store,
    },
    store::fs::FsStore,
    ticket::BlobTicket,
    BlobFormat, BlobsProtocol,
};
use n0_future::StreamExt;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "vegam-iroh-blobs-spike")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import a file (without copying) and serve it until killed.
    Send {
        /// Spike state dir (secret key + blob store). Reuse to keep the same
        /// EndpointId and blob store across restarts.
        #[arg(long)]
        store: PathBuf,
        /// File to make available.
        #[arg(long)]
        file: PathBuf,
        /// Strip direct IP addresses from the ticket so the Receiver must
        /// start over a relay path.
        #[arg(long)]
        relay_only_ticket: bool,
    },
    /// Receive a blob into the managed resume area, then export to dest.
    Recv {
        /// Spike state dir (secret key + blob store) — the managed resume area.
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        ticket: String,
        /// Final destination path. Only written after the blob verifies complete.
        #[arg(long)]
        dest: PathBuf,
        /// Reconnect attempts after a connection/stream error.
        #[arg(long, default_value_t = 0)]
        retries: u32,
        /// Seconds to wait between reconnect attempts.
        #[arg(long, default_value_t = 2)]
        retry_delay_secs: u64,
        /// Read pause/resume/quit commands from stdin.
        #[arg(long)]
        interactive: bool,
    },
    /// Report local partial state for a ticket without going to the network.
    Status {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        ticket: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    match Cli::parse().command {
        Command::Send {
            store,
            file,
            relay_only_ticket,
        } => send(store, file, relay_only_ticket).await,
        Command::Recv {
            store,
            ticket,
            dest,
            retries,
            retry_delay_secs,
            interactive,
        } => recv(store, ticket, dest, retries, retry_delay_secs, interactive).await,
        Command::Status { store, ticket } => status(store, ticket).await,
    }
}

/// Persist the endpoint secret key inside the spike state dir so the
/// EndpointId — and therefore any issued ticket — survives process restarts.
fn load_or_create_secret(dir: &Path) -> Result<SecretKey> {
    let path = dir.join("secret.key");
    if path.exists() {
        let hex_str = std::fs::read_to_string(&path)?;
        let bytes: [u8; 32] = hex::decode(hex_str.trim())?
            .try_into()
            .map_err(|_| anyhow::anyhow!("secret key file is not 32 bytes"))?;
        Ok(SecretKey::from_bytes(&bytes))
    } else {
        let key = SecretKey::generate();
        std::fs::write(&path, hex::encode(key.to_bytes()))?;
        Ok(key)
    }
}

async fn open_store(dir: &Path) -> Result<FsStore> {
    tokio::fs::create_dir_all(dir).await?;
    FsStore::load(dir.join("blobs"))
        .await
        .context("loading FsStore")
}

async fn send(store_dir: PathBuf, file: PathBuf, relay_only_ticket: bool) -> Result<()> {
    tokio::fs::create_dir_all(&store_dir).await?;
    let secret = load_or_create_secret(&store_dir)?;
    let store = open_store(&store_dir).await?;

    let file = std::fs::canonicalize(&file)?;
    eprintln!("importing {} (TryReference, no copy)", file.display());
    let t0 = Instant::now();
    let mut progress = store
        .add_path_with_opts(AddPathOptions {
            path: file.clone(),
            mode: ImportMode::TryReference,
            format: BlobFormat::Raw,
        })
        .stream()
        .await;
    let mut size = 0u64;
    let mut temp_tag = None;
    while let Some(item) = progress.next().await {
        match item {
            AddProgressItem::Size(s) => size = s,
            AddProgressItem::CopyProgress(_) | AddProgressItem::CopyDone => {}
            AddProgressItem::OutboardProgress(_) => {}
            AddProgressItem::Done(t) => {
                temp_tag = Some(t);
                break;
            }
            AddProgressItem::Error(e) => anyhow::bail!("import failed: {e}"),
        }
    }
    let temp_tag = temp_tag.context("import produced no tag")?;
    let hash = temp_tag.hash();
    // Named tag: keeps the blob alive past this process (Content Identity
    // anchor for sender restarts).
    store.tags().set(format!("spike-{hash}"), hash).await?;
    eprintln!(
        "import done in {:.1}s (hashing only; data referenced in place)",
        t0.elapsed().as_secs_f64()
    );

    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![iroh_blobs::protocol::ALPN.to_vec()])
        .secret_key(secret)
        .relay_mode(RelayMode::Default)
        .bind()
        .await?;
    let blobs = BlobsProtocol::new(&store, None);
    let router = iroh::protocol::Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, blobs)
        .spawn();

    // Wait until the relay knows us so the ticket carries a relay URL.
    let ep = router.endpoint();
    let _ = tokio::time::timeout(Duration::from_secs(30), ep.online()).await;

    let mut addr = ep.addr();
    if relay_only_ticket {
        addr.addrs = addr
            .addrs
            .iter()
            .filter(|a| matches!(a, TransportAddr::Relay(_)))
            .cloned()
            .collect();
    }
    let ticket = BlobTicket::new(addr, hash, BlobFormat::Raw);

    println!("SIZE: {size}");
    println!("HASH: {hash}");
    println!("ENDPOINT_ID: {}", ep.id());
    println!("TICKET: {ticket}");
    println!("SERVING: 1");
    eprintln!("serving until SIGINT/SIGTERM…");

    tokio::signal::ctrl_c().await?;
    drop(temp_tag);
    let _ = tokio::time::timeout(Duration::from_secs(2), router.shutdown()).await;
    Ok(())
}

#[derive(Debug, PartialEq)]
enum Cmd {
    Pause,
    Resume,
    Quit,
}

fn spawn_stdin_reader() -> mpsc::Receiver<Cmd> {
    let (tx, rx) = mpsc::channel(8);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            if std::io::BufRead::read_line(&mut stdin.lock(), &mut line).unwrap_or(0) == 0 {
                break;
            }
            let cmd = match line.trim() {
                "pause" => Cmd::Pause,
                "resume" => Cmd::Resume,
                "quit" => Cmd::Quit,
                _ => continue,
            };
            if tx.blocking_send(cmd).is_err() {
                break;
            }
        }
    });
    rx
}

/// One `KEY: value` snapshot of the connection's network paths.
fn report_paths(conn: &iroh::endpoint::Connection) {
    let paths = conn.paths();
    let mut selected_kind = "unknown";
    let mut kinds = Vec::new();
    for p in paths.iter() {
        let kind = if p.is_ip() {
            "direct"
        } else if p.is_relay() {
            "relay"
        } else {
            "unknown"
        };
        kinds.push(format!("{kind}{}", if p.is_selected() { "*" } else { "" }));
        if p.is_selected() {
            selected_kind = kind;
        }
    }
    println!(
        "CONNECTION: {} paths=[{}]",
        match selected_kind {
            "direct" => "Direct",
            "relay" => "Relayed",
            _ => "Unknown",
        },
        kinds.join(",")
    );
}

async fn recv(
    store_dir: PathBuf,
    ticket: String,
    dest: PathBuf,
    retries: u32,
    retry_delay_secs: u64,
    interactive: bool,
) -> Result<()> {
    let ticket = BlobTicket::from_str(&ticket).context("parsing ticket")?;
    tokio::fs::create_dir_all(&store_dir).await?;
    let secret = load_or_create_secret(&store_dir)?;
    let store = open_store(&store_dir).await?;
    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![])
        .secret_key(secret)
        .relay_mode(RelayMode::Default)
        .bind()
        .await?;

    let haf = ticket.hash_and_format();
    let mut cmd_rx = if interactive {
        Some(spawn_stdin_reader())
    } else {
        None
    };
    let mut retries_left = retries;
    let mut paused = false;
    let t0 = Instant::now();
    let mut session_payload_total: u64 = 0;

    loop {
        let local = store.remote().local(haf).await?;
        println!("LOCAL_BYTES: {}", local.local_bytes());
        if local.is_complete() {
            break;
        }

        if paused {
            println!("PAUSED: {}", local.local_bytes());
            let rx = cmd_rx.as_mut().context("paused without stdin reader")?;
            loop {
                match rx.recv().await {
                    Some(Cmd::Resume) => {
                        println!("RESUMED: 1");
                        paused = false;
                        break;
                    }
                    Some(Cmd::Quit) | None => {
                        println!("QUIT_WHILE_PAUSED: {}", local.local_bytes());
                        return Ok(());
                    }
                    Some(Cmd::Pause) => {}
                }
            }
            continue;
        }

        eprintln!("connecting to {}…", ticket.addr().id);
        let conn = match endpoint
            .connect(ticket.addr().clone(), iroh_blobs::protocol::ALPN)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                if retries_left == 0 {
                    println!("FAILED: connect: {e}");
                    anyhow::bail!("connect failed: {e}");
                }
                retries_left -= 1;
                println!("RETRYING: connect failed: {e}");
                tokio::time::sleep(Duration::from_secs(retry_delay_secs)).await;
                continue;
            }
        };
        report_paths(&conn);

        let base = local.local_bytes();
        let request = local.missing();
        let get = store.remote().execute_get(conn.clone(), request);
        let mut stream = get.stream();
        let mut last_print = Instant::now();
        let mut error: Option<String> = None;
        let mut want_pause = false;

        loop {
            let cmd_fut = async {
                match cmd_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    // never resolves; keeps select! shape uniform
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                item = stream.next() => match item {
                    Some(GetProgressItem::Progress(offset)) => {
                        if last_print.elapsed() >= Duration::from_secs(1) {
                            println!("PROGRESS: {}", base + offset);
                            report_paths(&conn);
                            last_print = Instant::now();
                        }
                    }
                    Some(GetProgressItem::Done(stats)) => {
                        session_payload_total += stats.payload_bytes_read;
                        println!("SESSION_PAYLOAD_BYTES: {}", stats.payload_bytes_read);
                        break;
                    }
                    Some(GetProgressItem::Error(e)) => {
                        error = Some(format!("{e:?}"));
                        break;
                    }
                    None => break,
                },
                cmd = cmd_fut => match cmd {
                    Some(Cmd::Pause) => { want_pause = true; break; }
                    Some(Cmd::Quit) | None => {
                        println!("QUIT: 1");
                        return Ok(());
                    }
                    Some(Cmd::Resume) => {}
                }
            }
        }
        // Dropping the stream/connection cancels the fetch; partial data
        // stays in the FsStore (the managed resume area).
        drop(stream);
        conn.close(0u32.into(), b"spike loop done");

        if want_pause {
            paused = true;
            continue;
        }
        if let Some(e) = error {
            if retries_left == 0 {
                println!("FAILED: {e}");
                anyhow::bail!("transfer failed: {e}");
            }
            retries_left -= 1;
            println!("RETRYING: {e}");
            tokio::time::sleep(Duration::from_secs(retry_delay_secs)).await;
        }
    }

    // Only a verified-complete blob reaches this point.
    let local = store.remote().local(haf).await?;
    anyhow::ensure!(local.is_complete(), "loop exited without complete blob");
    println!("COMPLETE: {}", local.local_bytes());
    println!("TOTAL_SESSION_PAYLOAD_BYTES: {session_payload_total}");
    println!("ELAPSED_SECS: {:.1}", t0.elapsed().as_secs_f64());

    let mut export = store
        .export_with_opts(ExportOptions {
            hash: ticket.hash(),
            target: std::path::absolute(&dest)?,
            mode: ExportMode::Copy,
        })
        .stream()
        .await;
    while let Some(item) = export.next().await {
        match item {
            ExportProgressItem::Size(_) | ExportProgressItem::CopyProgress(_) => {}
            ExportProgressItem::Done => break,
            ExportProgressItem::Error(e) => anyhow::bail!("export failed: {e}"),
        }
    }
    println!("EXPORTED: {}", dest.display());

    endpoint.close().await;
    store.shutdown().await?;
    Ok(())
}

async fn status(store_dir: PathBuf, ticket: String) -> Result<()> {
    let ticket = BlobTicket::from_str(&ticket).context("parsing ticket")?;
    let store = open_store(&store_dir).await?;
    let local = store.remote().local(ticket.hash_and_format()).await?;
    println!("LOCAL_BYTES: {}", local.local_bytes());
    println!("IS_COMPLETE: {}", local.is_complete());

    // Show the managed resume area contents for this blob.
    let data_dir = store_dir.join("blobs").join("data");
    let prefix = ticket.hash().to_hex().to_string();
    if let Ok(mut entries) = tokio::fs::read_dir(&data_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                let len = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                println!("RESUME_FILE: {name} {len}");
            }
        }
    }
    store.shutdown().await?;
    Ok(())
}

/// Compile-time use of the Store alias so the import list stays honest.
#[allow(dead_code)]
fn _store_type_check(s: &FsStore) -> &Store {
    s
}
