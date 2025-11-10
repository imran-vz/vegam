// Legacy helper functions - kept for potential future use
// Currently unused as Iroh struct in mod.rs handles initialization

#[allow(dead_code)]
use anyhow::Result;
#[allow(dead_code)]
use iroh::Endpoint;
#[allow(dead_code)]
use iroh_base::EndpointAddr;
#[allow(dead_code)]
use tracing::info;

#[allow(dead_code)]
pub async fn initialize_endpoint() -> Result<Endpoint> {
    info!("Initializing Iroh endpoint");
    let endpoint = Endpoint::builder().bind().await?;
    let node_id = endpoint.id();
    let local_addrs = endpoint.bound_sockets();
    info!("Iroh node initialized");
    info!("Node ID: {}", node_id);
    info!("Local addresses: {:?}", local_addrs);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    info!("Iroh endpoint initialized and ready");
    Ok(endpoint)
}

#[allow(dead_code)]
pub fn get_node_addr(endpoint: &Endpoint) -> EndpointAddr {
    let node_id = endpoint.id();
    let direct_addresses = endpoint.bound_sockets();
    let mut addr = EndpointAddr::new(node_id);
    for socket_addr in direct_addresses {
        addr = addr.with_ip_addr(socket_addr);
    }
    addr
}

#[allow(dead_code)]
pub fn get_node_id(endpoint: &Endpoint) -> String {
    endpoint.id().to_string()
}
