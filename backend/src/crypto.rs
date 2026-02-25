use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

const NONCE_SIZE: usize = 12;
const ENCRYPTED_PREFIX: &str = "ENC:";

/// Derive a 32-byte key from the environment variable DB_ENCRYPTION_KEY.
/// Pads or truncates to exactly 32 bytes.
fn derive_key() -> Result<[u8; 32], String> {
    let raw = std::env::var("DB_ENCRYPTION_KEY")
        .map_err(|_| "环境变量 DB_ENCRYPTION_KEY 未设置".to_string())?;
    if raw.is_empty() {
        return Err("DB_ENCRYPTION_KEY 不能为空".to_string());
    }
    let mut key = [0u8; 32];
    let bytes = raw.as_bytes();
    let len = bytes.len().min(32);
    key[..len].copy_from_slice(&bytes[..len]);
    Ok(key)
}

/// Encrypt a plaintext field using AES-256-GCM.
/// Returns a string prefixed with "ENC:" followed by hex(nonce + ciphertext).
/// If the input is empty, returns it as-is.
pub fn encrypt_field(plaintext: &str) -> Result<String, String> {
    if plaintext.is_empty() || plaintext.starts_with(ENCRYPTED_PREFIX) {
        return Ok(plaintext.to_string());
    }
    let key_bytes = derive_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("加密初始化失败: {}", e))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("加密失败: {}", e))?;

    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENCRYPTED_PREFIX, hex::encode(combined)))
}

/// Decrypt a field encrypted by encrypt_field().
/// If the input doesn't start with "ENC:", returns it as-is (plaintext passthrough).
pub fn decrypt_field(encrypted: &str) -> Result<String, String> {
    if encrypted.is_empty() || !encrypted.starts_with(ENCRYPTED_PREFIX) {
        return Ok(encrypted.to_string());
    }
    let hex_data = &encrypted[ENCRYPTED_PREFIX.len()..];
    let combined = hex::decode(hex_data)
        .map_err(|e| format!("解密 hex 解码失败: {}", e))?;

    if combined.len() < NONCE_SIZE {
        return Err("加密数据格式错误: 数据太短".to_string());
    }

    let key_bytes = derive_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("解密初始化失败: {}", e))?;

    let nonce = Nonce::from_slice(&combined[..NONCE_SIZE]);
    let ciphertext = &combined[NONCE_SIZE..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("解密失败: {}", e))?;

    String::from_utf8(plaintext)
        .map_err(|e| format!("解密后 UTF-8 转换失败: {}", e))
}

/// Check if a field value is already encrypted.
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

/// Check if DB_ENCRYPTION_KEY is configured.
pub fn encryption_enabled() -> bool {
    std::env::var("DB_ENCRYPTION_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        std::env::set_var("DB_ENCRYPTION_KEY", "test-key-for-unit-tests-32bytes!");
        let original = "13800138000";
        let encrypted = encrypt_field(original).unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        assert_ne!(encrypted, original);
        let decrypted = decrypt_field(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_empty_passthrough() {
        std::env::set_var("DB_ENCRYPTION_KEY", "test-key-for-unit-tests-32bytes!");
        assert_eq!(encrypt_field("").unwrap(), "");
        assert_eq!(decrypt_field("").unwrap(), "");
    }

    #[test]
    fn test_already_encrypted_passthrough() {
        std::env::set_var("DB_ENCRYPTION_KEY", "test-key-for-unit-tests-32bytes!");
        let encrypted = encrypt_field("hello").unwrap();
        let double_encrypted = encrypt_field(&encrypted).unwrap();
        assert_eq!(encrypted, double_encrypted);
    }

    #[test]
    fn test_plaintext_passthrough_on_decrypt() {
        assert_eq!(decrypt_field("plaintext").unwrap(), "plaintext");
    }
}
