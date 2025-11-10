use anyhow::Result;
use iroh::net::endpoint::{Endpoint, NodeAddr};
use tracing::info;

pub async fn initialize_endpoint() -> Result<Endpoint> {
    info!("Initializing Iroh endpoint");

    // Create endpoint with default configuration
    // This includes:
    // - Public relay servers for NAT traversal
    // - Local network discovery (mDNS-like swarm-discovery) for LAN peers
    let endpoint = Endpoint::builder().bind().await?;

    let node_id = endpoint.node_id();
    let bound = endpoint.bound_sockets();
    let local_addrs = vec![bound.0];

    info!("Iroh node initialized with local network discovery");
    info!("Node ID: {}", node_id);
    info!("Local addresses: {:?}", local_addrs);

    Ok(endpoint)
}

pub fn get_node_addr(endpoint: &Endpoint) -> NodeAddr {
    let node_id = endpoint.node_id();
    let relay_url = endpoint.home_relay();
    let bound = endpoint.bound_sockets();
    let direct_addresses = vec![bound.0];

    let mut addr = NodeAddr::new(node_id);

    if let Some(relay) = relay_url {
        addr = addr.with_relay_url(relay);
    }

    addr.with_direct_addresses(direct_addresses)
}

pub fn get_node_id(endpoint: &Endpoint) -> String {
    endpoint.node_id().to_string()
}
