use ed25519_dalek::{VerifyingKey, Signature, Verifier};
use sha2::{Sha256, Digest};
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // username
    pub role: String, // "admin" | "operator" | "viewer"
    pub exp: usize, // expiration timestamp
}

pub fn hash_sha256(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn verify_ed25519(
    pub_key_hex: &str,
    signature_hex: &str,
    checksum: &str,
) -> Result<(), &'static str> {
    let pub_key_bytes = hex::decode(pub_key_hex).map_err(|_| "Invalid public key hex")?;
    let pub_key_bytes: [u8; 32] = pub_key_bytes.try_into().map_err(|_| "Public key must be 32 bytes")?;
    
    let sig_bytes = hex::decode(signature_hex).map_err(|_| "Invalid signature hex")?;
    let sig_bytes: [u8; 64] = sig_bytes.try_into().map_err(|_| "Signature must be 64 bytes")?;
    
    let verifying_key = VerifyingKey::from_bytes(&pub_key_bytes).map_err(|_| "Invalid Ed25519 public key bytes")?;
    let signature = Signature::from_bytes(&sig_bytes);

    // Try string bytes first
    if verifying_key.verify(checksum.as_bytes(), &signature).is_ok() {
        return Ok(());
    }

    // Try decoded hex bytes if checksum is a hex string
    if let Ok(decoded_checksum) = hex::decode(checksum) {
        if verifying_key.verify(&decoded_checksum, &signature).is_ok() {
            return Ok(());
        }
    }

    Err("Ed25519 signature verification failed")
}

pub fn generate_jwt(username: &str, role: &str, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: username.to_string(),
        role: role.to_string(),
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.required_spec_claims.insert("sub".to_string());
    
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    
    Ok(token_data.claims)
}
