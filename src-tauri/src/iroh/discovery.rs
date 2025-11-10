// Local network discovery
//
// Iroh 0.20+ includes built-in local network discovery via swarm-discovery
// (similar to mDNS). It's enabled by default in Endpoint::builder().bind()
//
// This allows dialing peers by NodeID on local networks without internet access.
// Peers on the same LAN are automatically discovered and can connect directly.
//
// No additional setup required - just call endpoint.connect(node_addr) and
// Iroh will use local discovery + relay servers + direct addresses automatically.

/// Get device hostname for friendly peer naming
pub fn get_device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .unwrap_or_else(|| "Unknown Device".to_string())
}
