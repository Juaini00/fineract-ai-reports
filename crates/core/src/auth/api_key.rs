use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

pub fn hash_api_key(raw_key: &str) -> String {
    let digest = Sha256::digest(raw_key.as_bytes());
    hex::encode(digest)
}

pub fn generate_api_key(prefix: &str) -> String {
    let mut secret = [0_u8; 32];
    OsRng.fill_bytes(&mut secret);
    format!("{}_{}", prefix, hex::encode(secret))
}

pub fn key_display_prefix(raw_key: &str) -> String {
    raw_key.chars().take(18).collect()
}
