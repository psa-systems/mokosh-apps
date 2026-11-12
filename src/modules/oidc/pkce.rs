//! PKCE primitives: code_verifier generation and S256 code_challenge.
//!
//! Per RFC 7636: the verifier is 43..=128 chars from
//! `[A-Z][a-z][0-9]-._~`. We use 32 bytes of CSPRNG output base64url-
//! encoded with no padding (43 chars), which sits comfortably inside the
//! allowed range.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
// rand 0.10 moved `fill_bytes` from `RngCore` onto `Rng`, and dropped
// `RngCore` from the crate root re-exports. Importing `Rng` here keeps
// the two `rand::rng().fill_bytes(...)` call sites below working with
// no change to their shape.
use rand::Rng;
use sha2::{Digest, Sha256};

/// 32 random bytes, base64url-encoded with no padding.
// SPEC-3.8.2: 32 random bytes, base64url without padding (43 characters).
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// `code_challenge = base64url(SHA-256(code_verifier))`.
// SPEC-3.8.2: base64url, unpadded, SHA-256 of the verifier.
pub fn s256_challenge(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

/// Random opaque value for the `state` and `nonce` query parameters.
///
/// MAPPS-338: 32 bytes (256 bits) instead of the prior 16 bytes. RFC 6819
/// §5.3.5 floors `state` at 128 bits; 256 bits gives ample headroom and
/// matches the entropy budget of the PKCE verifier so the weakest link
/// in the flow is no longer the nonce.
// SPEC-3.8.3: fresh 32 bytes per call, used separately for state and nonce.
pub fn random_opaque() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// MAPPS-1235 finding 9 is not a defect: the file has two call sites, as
/// the import comment says. This keeps the comment honest if one is added.
#[cfg(test)]
mod mapps1235_pkce_tests {
    #[test]
    fn the_comment_counts_the_call_sites_there_are() {
        let src = include_str!("pkce.rs");
        let head = &src[..src.find("mod mapps1235_pkce_tests").expect("this module")];
        assert!(head.contains("the two `rand::rng().fill_bytes(...)` call sites"));
        assert_eq!(head.matches("rand::rng().fill_bytes(&mut").count(), 2);
    }
}
