use anyhow::Result;
use iroh::net::endpoint::{Endpoint, NodeAddr};
use iroh::net::relay::RelayMode;
use tracing::info;

pub async fn initialize_endpoint() -> Result<Endpoint> {
    info!("Initializing Iroh endpoint");

    // Create endpoint with relay mode enabled
    // This uses the default Iroh relay servers for NAT traversal
    // RelayMode::Default uses production Iroh relay servers
    let endpoint = Endpoint::builder()
        .relay_mode(RelayMode::Default)
        .bind()
        .await?;

    let node_id = endpoint.node_id();
    let bound = endpoint.bound_sockets();
    let local_addrs = vec![bound.0];

    info!("Iroh node initialized");
    info!("Node ID: {}", node_id);
    info!("Local addresses: {:?}", local_addrs);

    // Wait for relay connection with timeout
    let mut relay_connected = false;
    for attempt in 1..=10 {
        if let Some(relay) = endpoint.home_relay() {
            info!("Home relay connected: {}", relay);
            relay_connected = true;
            break;
        }
        info!("Waiting for relay connection (attempt {})", attempt);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    if !relay_connected {
        info!("Warning: No relay server connected after 5 seconds. Direct connections only.");
        info!("Current home relay: {:?}", endpoint.home_relay());
    }

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
