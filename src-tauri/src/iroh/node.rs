use anyhow::Result;
use iroh::endpoint::{Endpoint, RelayMode};
use iroh::EndpointAddr;
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

    let endpoint_id = endpoint.id();
    let bound_sockets = endpoint.bound_sockets();

    info!("Iroh node initialized");
    info!("Endpoint ID: {}", endpoint_id);
    info!("Bound sockets: {:?}", bound_sockets);

    // Wait for relay connection with timeout
    let mut relay_connected = false;
    for attempt in 1..=10 {
        let addr = endpoint.addr();
        if let Some(relay) = addr.relay_urls().next() {
            info!("Home relay connected: {}", relay);
            relay_connected = true;
            break;
        }
        info!("Waiting for relay connection (attempt {})", attempt);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    if !relay_connected {
        info!("Warning: No relay server connected after 5 seconds. Direct connections only.");
        let addr = endpoint.addr();
        info!(
            "Current relays: {:?}",
            addr.relay_urls().collect::<Vec<_>>()
        );
    }

    Ok(endpoint)
}

pub fn get_node_addr(endpoint: &Endpoint) -> EndpointAddr {
    let endpoint_id = endpoint.id();
    let mut addr = EndpointAddr::new(endpoint_id);

    // Add relays from current endpoint address
    let endpoint_addr = endpoint.addr();
    for relay in endpoint_addr.relay_urls() {
        addr = addr.with_relay_url(relay.clone());
    }

    // Add bound socket addresses
    for socket_addr in endpoint.bound_sockets() {
        addr = addr.with_ip_addr(socket_addr);
    }

    addr
}

pub fn get_node_id(endpoint: &Endpoint) -> String {
    endpoint.id().to_string()
}
