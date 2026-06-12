# Use Default Public Iroh Relays for V1

Production v1 relies on the default public Iroh relay infrastructure instead of running Vegam-owned relays. This avoids relay operations and cost before demand is proven, while still allowing Iroh to fall back to relayed connectivity when direct peer-to-peer connection is unavailable. The active Transfer detail should show a subtle per-Transfer status such as "Direct" or "Relayed"; deeper network-path details belong in diagnostics.
