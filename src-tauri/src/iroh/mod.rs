pub mod discovery;
pub mod node;
pub mod ticket_codec;
pub mod transfer;

use std::{ops::Deref, path::PathBuf, str::FromStr, sync::Arc};

use anyhow::Result;
use iroh::protocol::Router;
use iroh_base::{EndpointAddr, EndpointId};
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::Gossip,
    proto::TopicId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// Re-export Blobs for ease of use
pub use iroh_blobs::api::blobs::Blobs;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GossipTicket {
    pub topic_id: TopicId,
    pub node_id: EndpointId,
}

impl GossipTicket {
    pub fn new(topic_id: TopicId, node_id: EndpointId) -> Self {
        Self { topic_id, node_id }
    }
}

impl GossipTicket {
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Infallible")
    }
}

impl FromStr for GossipTicket {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let bytes = data_encoding::BASE32
            .decode(s.as_bytes())
            .map_err(|_| anyhow::anyhow!("Invalid base32 string"))?;
        Self::from_bytes(&bytes)
    }
}

impl std::fmt::Display for GossipTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut text = data_encoding::BASE32.encode(&self.to_bytes());
        text.make_ascii_lowercase();
        write!(f, "{}", text)
    }
}

#[derive(Debug, Clone)]
pub struct GossipClient {
    pub client: Gossip,
    ticket: GossipTicket,
    channel: Arc<RwLock<GossipChannel>>,
}

impl Deref for GossipClient {
    type Target = Gossip;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl GossipClient {
    pub async fn new(gossip: Gossip, node_id: EndpointId) -> Result<Self> {
        let topic_id = TopicId::from_bytes(rand::random());
        let ticket = GossipTicket::new(topic_id, node_id);
        let topic = gossip.subscribe(topic_id, vec![]).await?;
        let (sender, receiver) = topic.split();
        let gossip_chan = GossipChannel {
            sender,
            receiver: Some(receiver),
        };

        Ok(Self {
            client: gossip,
            ticket,
            channel: Arc::new(RwLock::new(gossip_chan)),
        })
    }

    pub async fn take_receiver(&self) -> Result<GossipReceiver> {
        let mut chan = self.channel.write().await;
        chan.receiver
            .take()
            .ok_or(anyhow::anyhow!("Receiver already taken"))
    }

    pub async fn get_sender(&self) -> GossipSender {
        let chan = self.channel.read().await;
        chan.sender.clone()
    }

    pub fn ticket(&self) -> &GossipTicket {
        &self.ticket
    }
}

#[derive(Debug)]
pub struct GossipChannel {
    sender: GossipSender,
    receiver: Option<GossipReceiver>,
}

impl Deref for GossipChannel {
    type Target = GossipSender;

    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

#[derive(Debug, Clone)]
pub struct Iroh {
    #[allow(dead_code)]
    router: Router,
    pub blobs: Blobs,
    pub downloader: iroh_blobs::api::downloader::Downloader,
    #[allow(dead_code)] // Keep for potential future direct endpoint access
    pub endpoint: iroh::Endpoint,
    pub node_addr: EndpointAddr,
    pub gossip: GossipClient,
}

impl Iroh {
    pub async fn new(path: PathBuf) -> Result<Self> {
        // create dir if it doesn't already exist
        tokio::fs::create_dir_all(&path).await?;

        // create endpoint with relay servers for NAT traversal
        let endpoint = iroh::Endpoint::builder()
            .relay_mode(iroh::RelayMode::Default)
            .bind()
            .await?;

        // build the protocol router
        let mut builder = iroh::protocol::Router::builder(endpoint.clone());

        // add iroh blobs - use in-memory store
        use iroh_blobs::store::mem::MemStore;
        use std::sync::Arc;
        let store = MemStore::new();
        let blobs_protocol = Arc::new(iroh_blobs::BlobsProtocol::new(&store, None));

        builder = builder.accept(iroh_blobs::ALPN, blobs_protocol);

        // add iroh gossip
        let gossip = Gossip::builder().spawn(endpoint.clone());
        builder = builder.accept(iroh_gossip::ALPN, gossip.clone());

        let router = builder.spawn();

        // Get API interface and downloader from store
        let blobs = store.blobs().clone();
        let downloader = store.downloader(&endpoint);

        // Wait for relay connection to establish (longer timeout for mobile networks)
        tracing::info!("Waiting for relay connection...");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        // Get node address with relay info (endpoint.addr() includes relay URLs)
        let node_id = endpoint.id();
        let node_addr = endpoint.addr();

        let relay_urls: Vec<_> = node_addr.relay_urls().collect();
        if relay_urls.is_empty() {
            tracing::warn!("No relay URLs found in node address - NAT traversal may fail");
            tracing::warn!("Check network connectivity and relay server accessibility");
        } else {
            tracing::info!("Relay connection established: {:?}", relay_urls);
        }

        let gossip = GossipClient::new(gossip, node_id).await?;

        Ok(Self {
            node_addr,
            router,
            blobs,
            downloader,
            endpoint,
            gossip,
        })
    }

    #[allow(dead_code)]
    pub async fn shutdown(&self) -> Result<(), String> {
        self.router.shutdown().await.map_err(|e| e.to_string())
    }
}
