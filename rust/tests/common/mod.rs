#![allow(dead_code)]

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use pipe_agent_storage::{PipeStorage, PipeStorageOptions};
use rand::RngCore;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_BASE_URL: &str = "https://us-west-01-firestarter.pipenetwork.com";

pub fn env_or(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Generate an ed25519 keypair and return (signing_key, base58_public_key).
pub fn generate_solana_keypair() -> (SigningKey, String) {
    let mut rng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut rng);
    let pubkey_bytes = signing_key.verifying_key().to_bytes();
    let pubkey_b58 = bs58_encode(&pubkey_bytes);
    (signing_key, pubkey_b58)
}

/// Path to the cached test keypair (in the target directory so it survives rebuilds
/// but stays out of source control).
fn keypair_cache_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push(".test-keypair");
    p
}

/// Load a persisted ed25519 keypair, or generate + save a new one.
/// Reusing the same keypair avoids creating a new SIWS account per test run,
/// which would hit the server's 1-account-per-IP-per-week rate limit.
pub fn load_or_create_keypair() -> (SigningKey, String) {
    let path = keypair_cache_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            let sk = SigningKey::from_bytes(&arr);
            let pubkey = bs58_encode(&sk.verifying_key().to_bytes());
            return (sk, pubkey);
        }
    }
    // Generate new keypair and persist
    let (sk, pubkey) = generate_solana_keypair();
    let _ = std::fs::write(&path, sk.to_bytes());
    (sk, pubkey)
}

/// Minimal base58 encoder (Bitcoin/Solana alphabet).
pub fn bs58_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let mut leading_zeros = 0;
    for &b in data {
        if b == 0 {
            leading_zeros += 1;
        } else {
            break;
        }
    }

    let mut digits: Vec<u8> = Vec::new();
    for &b in data {
        let mut carry = b as u32;
        for d in digits.iter_mut() {
            carry += (*d as u32) * 256;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }

    let mut result = String::with_capacity(leading_zeros + digits.len());
    for _ in 0..leading_zeros {
        result.push('1');
    }
    for &d in digits.iter().rev() {
        result.push(ALPHABET[d as usize] as char);
    }
    result
}

/// Sign a message with the ed25519 key and return base64-encoded signature.
pub fn sign_message(signing_key: &SigningKey, message: &str) -> String {
    let signature = signing_key.sign(message.as_bytes());
    STANDARD.encode(signature.to_bytes())
}

/// Generate random bytes of a given size.
pub fn random_payload(size: usize) -> Vec<u8> {
    let mut buf = vec![0u8; size];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

/// Create a PipeStorage client authenticated via SIWS.
/// Falls back to PIPE_API_KEY if set.
/// Returns (client, needs_logout) — caller should logout if needs_logout is true.
pub async fn authenticated_client() -> Result<(PipeStorage, bool), Box<dyn std::error::Error>> {
    let base_url = env_or("PIPE_BASE_URL", DEFAULT_BASE_URL);

    // If API key is provided, use it directly (no logout needed)
    if let Ok(api_key) = env::var("PIPE_API_KEY") {
        if !api_key.trim().is_empty() {
            let account = env::var("PIPE_ACCOUNT")
                .ok()
                .filter(|v| !v.trim().is_empty());
            let client = PipeStorage::new(PipeStorageOptions {
                api_key: Some(api_key),
                base_url: Some(base_url),
                account,
                ..Default::default()
            });
            return Ok((client, false));
        }
    }

    // Default: SIWS auth with a persistent keypair (reused across runs to
    // avoid the 1-account-per-IP rate limit on the production server).
    let (signing_key, wallet_pubkey) = load_or_create_keypair();
    let mut client = PipeStorage::new(PipeStorageOptions {
        base_url: Some(base_url),
        timeout: Some(Duration::from_secs(300)),
        poll_interval: Some(Duration::from_secs(1)),
        ..Default::default()
    });

    let challenge = client.auth_challenge(&wallet_pubkey).await?;
    let signature_b64 = sign_message(&signing_key, &challenge.message);
    client
        .auth_verify(
            &wallet_pubkey,
            &challenge.nonce,
            &challenge.message,
            &signature_b64,
        )
        .await?;

    Ok((client, true))
}
