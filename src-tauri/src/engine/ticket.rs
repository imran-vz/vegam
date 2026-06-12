//! The Vegam Transfer Ticket: a bearer value carrying everything a Receiver
//! needs to start a Transfer (ADR 0003), plus the metadata the Receive UI
//! shows before downloading.
//!
//! String form: `vegam<base32>` via the `iroh_tickets::Ticket` trait — the
//! same mechanism that gives `BlobTicket` its `blob...` form. The embedded
//! `issued_at` is advisory for the Receiver (pre-flight "this looks
//! expired"); authoritative expiry is enforced by the Sender's provider gate
//! (ADR 0015).

use iroh_blobs::ticket::BlobTicket;
use iroh_tickets::{ParseError, Ticket};
use serde::{Deserialize, Serialize};

use crate::engine::types::{now_unix_ms, TicketPreview};

/// Transfer Tickets expire 24 hours after issuance by default (ADR 0015).
pub const TICKET_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq)]
pub struct VegamTicket {
    pub blob: BlobTicket,
    pub file_name: String,
    pub size: u64,
    /// Unix seconds at issuance.
    pub issued_at: u64,
}

/// Wire format: single-variant postcard enum so future revisions can add
/// variants without breaking old tickets. The blob ticket is embedded as its
/// own `encode_bytes` output, delegating wire compatibility upstream.
#[derive(Serialize, Deserialize)]
enum VegamTicketWireFormat {
    Variant0(Variant0VegamTicket),
}

#[derive(Serialize, Deserialize)]
struct Variant0VegamTicket {
    blob: Vec<u8>,
    file_name: String,
    size: u64,
    issued_at: u64,
}

impl Ticket for VegamTicket {
    const KIND: &'static str = "vegam";

    fn encode_bytes(&self) -> Vec<u8> {
        let data = VegamTicketWireFormat::Variant0(Variant0VegamTicket {
            blob: self.blob.encode_bytes(),
            file_name: self.file_name.clone(),
            size: self.size,
            issued_at: self.issued_at,
        });
        postcard::to_stdvec(&data).expect("postcard serialization failed")
    }

    fn decode_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let res: VegamTicketWireFormat = postcard::from_bytes(bytes)?;
        let VegamTicketWireFormat::Variant0(v) = res;
        let blob = BlobTicket::decode_bytes(&v.blob)?;
        Ok(Self {
            blob,
            file_name: v.file_name,
            size: v.size,
            issued_at: v.issued_at,
        })
    }
}

impl std::fmt::Display for VegamTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode_string())
    }
}

impl std::str::FromStr for VegamTicket {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode_string(s.trim())
    }
}

impl VegamTicket {
    pub fn expires_at(&self) -> u64 {
        self.issued_at.saturating_add(TICKET_TTL_SECS)
    }

    pub fn preview(&self) -> TicketPreview {
        TicketPreview {
            file_name: self.file_name.clone(),
            size: self.size,
            issued_at_ms: self.issued_at * 1000,
            expires_at_ms: self.expires_at() * 1000,
            is_probably_expired: now_unix_ms() > self.expires_at() * 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::EndpointAddr;
    use iroh_blobs::{BlobFormat, Hash};
    use std::str::FromStr;

    fn sample_ticket(issued_at: u64) -> VegamTicket {
        let id = iroh::SecretKey::generate().public();
        let addr = EndpointAddr::new(id);
        let hash = Hash::new(b"vegam test content");
        VegamTicket {
            blob: BlobTicket::new(addr, hash, BlobFormat::Raw),
            file_name: "movie.mkv".to_string(),
            size: 107_374_182_400,
            issued_at,
        }
    }

    #[test]
    fn roundtrip() {
        let t = sample_ticket(1_750_000_000);
        let s = t.to_string();
        assert!(s.starts_with("vegam"), "ticket is self-identifying: {s}");
        let back = VegamTicket::from_str(&s).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn rejects_garbage_and_foreign_kinds() {
        assert!(VegamTicket::from_str("garbage").is_err());
        assert!(VegamTicket::from_str("").is_err());
        // A blob ticket is not a vegam ticket.
        let blob = sample_ticket(0).blob;
        assert!(VegamTicket::from_str(&blob.to_string()).is_err());
        // Valid prefix, invalid payload.
        assert!(VegamTicket::from_str("vegamaaaaaaaa").is_err());
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        let t = sample_ticket(123);
        let s = format!("  {}\n", t);
        assert_eq!(VegamTicket::from_str(&s).unwrap(), t);
    }

    #[test]
    fn expiry_math() {
        let t = sample_ticket(1_000);
        assert_eq!(t.expires_at(), 1_000 + 24 * 60 * 60);
        // Far-future issuance does not overflow.
        let t = sample_ticket(u64::MAX);
        assert_eq!(t.expires_at(), u64::MAX);
    }
}
