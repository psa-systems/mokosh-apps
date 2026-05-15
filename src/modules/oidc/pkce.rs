//! PKCE primitives: code_verifier generation and S256 code_challenge.
//!
//! Per RFC 7636: the verifier is 43..=128 chars from
//! `[A-Z][a-z][0-9]-._~`. We use 32 bytes of CSPRNG output base64url-
//! encoded with no padding (43 chars), which sits comfortably inside the
//! allowed range.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// 32 random bytes, base64url-encoded with no padding.
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// `code_challenge = base64url(SHA-256(code_verifier))`.
pub fn s256_challenge(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

/// Random opaque value for the `state` and `nonce` query parameters.
pub fn random_opaque() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
