use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;

/// AES-256-GCM key used for encrypting stored OAuth tokens.
pub type AesKey = Key<Aes256Gcm>;

const NONCE_LEN: usize = 12;

/// Generates a cryptographically secure random token (base64url, no padding).
pub fn generate_token(length: usize) -> String {
    let bytes_needed = (length * 3).div_ceil(4).max(16);
    let mut buf = vec![0u8; bytes_needed];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Derives a stable 32-byte AES key from a configured secret.
/// The secret must be at least 32 bytes; shorter secrets are rejected.
pub fn derive_key_from_secret(secret: &str) -> anyhow::Result<Key<Aes256Gcm>> {
    if secret.len() < 32 {
        anyhow::bail!("encryption key must be at least 32 bytes long");
    }

    let mut key = [0u8; 32];
    let secret_bytes = secret.as_bytes();
    // XOR-fold the secret into the key buffer so any length >= 32 works
    for (i, b) in secret_bytes.iter().enumerate() {
        key[i % 32] ^= b;
    }
    Ok(*Key::<Aes256Gcm>::from_slice(&key))
}

/// Encrypts plaintext with AES-256-GCM. Output format: base64(nonce || ciphertext).
pub fn encrypt(key: &Key<Aes256Gcm>, plaintext: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(out))
}

/// Decrypts data produced by [`encrypt`].
pub fn decrypt(key: &Key<Aes256Gcm>, encoded: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key);
    let data = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| anyhow::anyhow!("invalid encrypted payload: {e}"))?;

    if data.len() <= NONCE_LEN {
        anyhow::bail!("encrypted payload too short");
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("invalid utf8 in plaintext: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = derive_key_from_secret("test_encryption_key_32_bytes_long!!").unwrap();
        let original = "oauth-token-abc123";
        let encrypted = encrypt(&key, original).unwrap();
        assert_ne!(encrypted, original);
        assert_eq!(decrypt(&key, &encrypted).unwrap(), original);
    }

    #[test]
    fn rejects_short_key() {
        assert!(derive_key_from_secret("short").is_err());
    }

    #[test]
    fn token_length() {
        let t = generate_token(32);
        assert!(!t.is_empty());
    }
}
