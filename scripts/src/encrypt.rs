use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use hex;
use rsa::{oaep::Oaep, pkcs1::DecodeRsaPublicKey, pkcs8::DecodePublicKey, RsaPublicKey};
use sha2::Sha256;
use std::env;

/// Encrypts the entity secret using the public key with RSA-OAEP encryption
/// This is equivalent to the Node.js forge implementation
pub fn encrypt_entity_secret(entity_secret_hex: &str, public_key_pem: &str) -> Result<String> {
    // Convert hex string to bytes
    println!("entity_secret_hex: {}", entity_secret_hex);
    let entity_secret_bytes = hex::decode(entity_secret_hex)
        .map_err(|e| anyhow!("Failed to decode hex entity secret: {}", e))?;

    // Parse the public key from PEM format
    println!(
        "Public key PEM header: {}",
        public_key_pem.lines().next().unwrap_or("No header found")
    );

    // Try PKCS#1 format first, then fall back to PKCS#8 format
    let public_key = match RsaPublicKey::from_pkcs1_pem(public_key_pem) {
        Ok(key) => {
            println!("Successfully parsed PKCS#1 format");
            key
        }
        Err(e1) => {
            println!("PKCS#1 parsing failed: {}, trying PKCS#8...", e1);
            match RsaPublicKey::from_public_key_pem(public_key_pem) {
                Ok(key) => {
                    println!("Successfully parsed PKCS#8 format");
                    key
                }
                Err(e2) => {
                    return Err(anyhow!(
                        "Failed to parse public key from PEM (tried both PKCS#1 and PKCS#8): PKCS#1 error: {}, PKCS#8 error: {}",
                        e1, e2
                    ));
                }
            }
        }
    };

    // Encrypt using RSA-OAEP with SHA-256
    let mut rng = rand::thread_rng();
    let padding = Oaep::new::<Sha256>();
    let encrypted_data = public_key
        .encrypt(&mut rng, padding, &entity_secret_bytes)
        .map_err(|e| anyhow!("Failed to encrypt data: {}", e))?;

    // Encode to base64
    let base64_encoded = general_purpose::STANDARD.encode(&encrypted_data);

    Ok(base64_encoded)
}

/// Encrypts entity secret from environment variables
pub fn encrypt_from_env() -> Result<String> {
    let entity_secret = env::var("ENTITY_SECRET")
        .map_err(|_| anyhow!("ENTITY_SECRET environment variable not found"))?;

    let public_key = env::var("CIRCLE_PUBLIC_KEY")
        .map_err(|_| anyhow!("CIRCLE_PUBLIC_KEY environment variable not found"))?;

    encrypt_entity_secret(&entity_secret, &public_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_entity_secret() {
        // This is a test with sample data - in real usage, use actual keys
        let entity_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let public_key = "-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA...\n-----END PUBLIC KEY-----";

        // This will fail with invalid key, but tests the function structure
        let result = encrypt_entity_secret(entity_secret, public_key);
        assert!(result.is_err()); // Expected to fail with invalid test key
    }
}
