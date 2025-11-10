use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Derive a 32-byte encryption key from the node ID
/// This ensures each device has a unique encryption key
fn derive_key(node_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"vegam-ticket-key-");
    hasher.update(node_id.as_bytes());
    let result = hasher.finalize();
    result.into()
}

/// Encrypt a ticket string using AES-256-GCM
/// Format: vegam://node_id:base64(nonce || ciphertext)
/// The node_id is included so the receiver can derive the same key
pub fn encrypt_ticket(ticket: &str, node_id: &str) -> Result<String> {
    let key_bytes = derive_key(node_id);
    let cipher = Aes256Gcm::new(&key_bytes.into());

    // Generate random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);

    // Encrypt the ticket
    let ciphertext = cipher
        .encrypt(&nonce, ticket.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Combine nonce + ciphertext
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    // Encode to base64 and include node_id in the ticket
    let encoded = URL_SAFE_NO_PAD.encode(&combined);
    Ok(format!("vegam://{}:{}", node_id, encoded))
}

/// Decrypt a ticket string using AES-256-GCM
/// Supports encrypted format: vegam://node_id:base64(nonce || ciphertext)
/// The node_id parameter is ignored - we use the node_id from the ticket
pub fn decrypt_ticket(ticket: &str, _receiver_node_id: &str) -> Result<String> {
    // Check if it's an encrypted ticket
    let without_prefix = ticket
        .strip_prefix("vegam://")
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket format: missing 'vegam:// prefix"))?;

    // Split to get sender's node_id and encrypted data
    let parts: Vec<&str> = without_prefix.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid ticket format: missing node_id"));
    }

    let sender_node_id = parts[0];
    let encoded = parts[1];

    // Decode from base64
    let combined = URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|e| anyhow::anyhow!("Invalid ticket encoding: {}", e))?;

    // Split nonce and ciphertext
    if combined.len() < 12 {
        return Err(anyhow::anyhow!("Invalid ticket: too short"));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce_array: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid nonce size"))?;
    let nonce = Nonce::from(nonce_array);

    // Derive key using sender's node_id (not receiver's)
    let key_bytes = derive_key(sender_node_id);
    let cipher = Aes256Gcm::new(&key_bytes.into());

    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("Invalid ticket format: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let original = "test.txt|1234|blobhash123";
        let node_id = "test-node-id";

        let encrypted = encrypt_ticket(original, node_id).unwrap();
        assert!(encrypted.starts_with("vegam://"));

        let decrypted = decrypt_ticket(&encrypted, node_id).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_different_node_ids_produce_different_keys() {
        let ticket = "test.txt|1234|blobhash123";
        let node1 = "node-1";
        let node2 = "node-2";

        let encrypted1 = encrypt_ticket(ticket, node1).unwrap();
        let encrypted2 = encrypt_ticket(ticket, node2).unwrap();

        // Different node IDs should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // Encrypted tickets include the sender's node_id
        assert!(encrypted1.contains("node-1"));
        assert!(encrypted2.contains("node-2"));

        // Any receiver can decrypt because sender's node_id is in the ticket
        assert_eq!(decrypt_ticket(&encrypted1, "any-receiver").unwrap(), ticket);
        assert_eq!(decrypt_ticket(&encrypted2, "any-receiver").unwrap(), ticket);
    }

    #[test]
    fn test_invalid_format_fails() {
        let node_id = "test-node";

        // Missing prefix
        assert!(decrypt_ticket("invalid", node_id).is_err());

        // Invalid base64
        assert!(decrypt_ticket("vegam://!!!", node_id).is_err());

        // Too short
        assert!(decrypt_ticket("vegam://AA", node_id).is_err());
    }

    #[test]
    fn test_encrypted_format_is_url_safe() {
        let ticket = "test.txt|1234|blobhash123";
        let node_id = "test-node";
        let encrypted = encrypt_ticket(ticket, node_id).unwrap();

        // Should be URL-safe (no special chars that need escaping)
        assert!(!encrypted.contains('='));
        assert!(!encrypted.contains('+'));
        assert!(!encrypted.contains('/'));
    }
}
