use anyhow::Result;
use iroh::net::endpoint::{Endpoint, NodeAddr};
use tracing::info;

pub async fn initialize_endpoint() -> Result<Endpoint> {
    info!("Initializing Iroh endpoint");

    // Create endpoint with default configuration
    // This includes public relay servers for NAT traversal
    let endpoint = Endpoint::builder().bind().await?;

    let node_id = endpoint.node_id();
    let bound = endpoint.bound_sockets();
    let local_addrs = vec![bound.0];

    info!("Iroh node initialized");
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

// pub fn parse_node_id(node_id_str: &str) -> Result<NodeId> {
//     node_id_str
//         .parse()
//         .map_err(|e| anyhow::anyhow!("Invalid node ID: {}", e))
// }
