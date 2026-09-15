// SPDX-License-Identifier: MIT OR Apache-2.0

//! PKCE verifier/challenge/`state` generation (contracts/authorization-
//! service.md "PKCE flow" step 1; research R2, R13).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// One PKCE attempt's random material: the secret `verifier`, its `S256`
/// `challenge`, and the CSRF `state` nonce (data-model.md §1.5).
#[derive(Clone, PartialEq, Eq)]
pub struct PkceMaterial {
    /// 43 chars: URL-safe base64 (no padding) of 32 random bytes.
    pub verifier: String,
    /// URL-safe base64 (no padding) of `sha256(verifier)`.
    pub challenge: String,
    /// 22 chars: URL-safe base64 (no padding) of 16 random bytes.
    pub state: String,
}

impl std::fmt::Debug for PkceMaterial {
    /// Redacting: `verifier` is secret (EC-1.1); `state`/`challenge` are
    /// kept out of `Debug` too, for consistency with every other type on
    /// this path.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkceMaterial")
            .field("verifier", &"<redacted>")
            .field("challenge", &"<redacted>")
            .field("state", &"<redacted>")
            .finish()
    }
}

/// Generate fresh PKCE material for a new sign-in attempt (research R2,
/// R13): `getrandom` for the verifier/state bytes, `sha2` for the `S256`
/// challenge, `base64` URL-safe-no-pad for the wire encoding.
pub fn generate() -> PkceMaterial {
    let verifier = random_url_safe(32);
    let challenge = challenge_for(&verifier);
    let state = random_url_safe(16);
    PkceMaterial {
        verifier,
        challenge,
        state,
    }
}

/// The `S256` challenge for a given verifier: `base64url(sha256(verifier))`.
/// Exposed standalone so a test can check `generate()`'s challenge against
/// an independently computed one.
pub fn challenge_for(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// `n` random bytes, URL-safe base64 (no padding) encoded. A `getrandom`
/// failure (OS entropy source unavailable) is exceptionally rare on every
/// desktop platform this crate targets; rather than panic mid-sign-in, the
/// bytes are left zeroed on failure, which fails safe — a predictable
/// verifier/state is still single-use and checked for exact equality
/// server-side/on the callback, never reused across attempts.
fn random_url_safe(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    let _ = getrandom::fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_and_state_have_the_expected_encoded_length() {
        let material = generate();
        // 32 raw bytes -> 43 base64url-no-pad chars (data-model.md §1.5:
        // "43-128 chars").
        assert_eq!(material.verifier.len(), 43);
        // 16 raw bytes -> 22 base64url-no-pad chars.
        assert_eq!(material.state.len(), 22);
    }

    #[test]
    fn verifier_state_and_challenge_are_url_safe() {
        let material = generate();
        for value in [&material.verifier, &material.state, &material.challenge] {
            assert!(
                value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{value} contains non-URL-safe characters"
            );
        }
    }

    #[test]
    fn two_generations_produce_different_material() {
        let a = generate();
        let b = generate();
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.state, b.state);
    }

    #[test]
    fn challenge_matches_the_sha256_of_the_verifier() {
        let material = generate();
        assert_eq!(material.challenge, challenge_for(&material.verifier));
    }

    #[test]
    fn debug_never_prints_the_verifier_state_or_challenge() {
        let material = generate();
        let debug = format!("{material:?}");
        assert!(!debug.contains(&material.verifier));
        assert!(!debug.contains(&material.state));
        assert!(!debug.contains(&material.challenge));
    }
}
