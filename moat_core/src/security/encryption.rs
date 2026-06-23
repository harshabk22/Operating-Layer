use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Encrypts credentials using XOR encryption and encodes in Base64.
pub fn encrypt_credential(plain_text: &str, key: &str) -> String {
    let plain_bytes = plain_text.as_bytes();
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() {
        return STANDARD.encode(plain_bytes);
    }
    let mut encrypted = Vec::with_capacity(plain_bytes.len());
    for (i, &byte) in plain_bytes.iter().enumerate() {
        let key_byte = key_bytes[i % key_bytes.len()];
        encrypted.push(byte ^ key_byte);
    }
    STANDARD.encode(&encrypted)
}

/// Decrypts credentials from Base64 and performs XOR decryption.
pub fn decrypt_credential(encrypted_base64: &str, key: &str) -> anyhow::Result<String> {
    let encrypted_bytes = STANDARD.decode(encrypted_base64)?;
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() {
        return String::from_utf8(encrypted_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in decrypted credentials: {}", e));
    }
    let mut decrypted = Vec::with_capacity(encrypted_bytes.len());
    for (i, &byte) in encrypted_bytes.iter().enumerate() {
        let key_byte = key_bytes[i % key_bytes.len()];
        decrypted.push(byte ^ key_byte);
    }
    String::from_utf8(decrypted)
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in decrypted credentials: {}", e))
}
