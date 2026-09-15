// SPDX-License-Identifier: MIT OR Apache-2.0

//! `AuthorizationService` trait, `ClientConfig`, `TokenSet`, `Profile`,
//! `AuthError` (contracts/authorization-service.md). Service-specific
//! detail (endpoints, client id, scopes, response shapes) lives in
//! `spotify.rs`; everything else in the crate uses this trait.

use std::time::Duration;

use crate::pending::PendingAuthorization;
use crate::session::Tier;

/// The loopback callback path (contracts/authorization-service.md
/// `ClientConfig::redirect_path`, research R2: "the path `/login`"). Fixed
/// — unlike `client_id`, this is not configurable — so `service.rs`'s
/// listener and `spotify.rs`'s `ClientConfig` share one source of truth
/// rather than routing it through the `AuthorizationService` trait.
pub const REDIRECT_PATH: &str = "/login";

/// OAuth client/endpoint configuration, service-agnostic at the trait
/// boundary (contracts/authorization-service.md).
#[derive(Debug, Clone, Copy)]
pub struct ClientConfig {
    /// Default: the service's Keymaster client id (research R4);
    /// overridden by env `MODPLAYER_OAUTH_CLIENT_ID` at startup.
    pub client_id: &'static str,
    /// `"/login"`.
    pub redirect_path: &'static str,
    pub scopes: &'static [&'static str],
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub profile_url: &'static str,
}

impl ClientConfig {
    /// `http://127.0.0.1:{port}{redirect_path}`
    /// (contracts/authorization-service.md).
    pub fn redirect_uri(&self, port: u16) -> String {
        format!("http://127.0.0.1:{port}{}", self.redirect_path)
    }
}

/// A fresh access/refresh token pair from the token endpoint.
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Duration,
    pub scope: String,
}

impl std::fmt::Debug for TokenSet {
    /// Redacting: never prints token bytes (FR-020).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .finish()
    }
}

/// The service account profile used for the tier check.
#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub display_name: Option<String>,
    pub tier: Tier,
}

impl std::fmt::Debug for Profile {
    /// Redacting, for consistency with every other type on this path —
    /// the account id/display name are never logged either way.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("id", &"<redacted>")
            .field("display_name", &"<redacted>")
            .field("tier", &self.tier)
            .finish()
    }
}

/// Failure modes from any `AuthorizationService` call
/// (contracts/authorization-service.md).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// DNS/TCP/TLS/timeout/5xx/429-exhausted: retryable.
    #[error("transient authorization failure")]
    Transient,
    /// The service says the grant/token is invalid or revoked: NOT
    /// retryable (token endpoint `invalid_grant`, profile 401 after one
    /// refresh).
    #[error("authorization rejected")]
    Rejected,
    /// Token valid but call forbidden (403): keep session, tier `Unknown`.
    #[error("authorization forbidden")]
    Forbidden,
    /// User cancelled in the browser (`error=access_denied` on the
    /// callback).
    #[error("authorization cancelled")]
    Cancelled,
    /// Any other definitive error from the service. Carries only the OAuth
    /// `error` code (e.g. `invalid_request`), never the response body.
    #[error("authorization service error: {0}")]
    Service(String),
}

/// Blocking authorization transport: PKCE URL building, code exchange,
/// refresh, and the profile call used as the tier check
/// (contracts/authorization-service.md). Implementors:
/// `SpotifyAuthorizationService` (`spotify.rs`), `FakeAuthorizationService`
/// (`fake_auth.rs`).
pub trait AuthorizationService: Send + Sync + 'static {
    /// Pure. Builds the browser URL for `attempt` (`response_type=code`,
    /// `client_id`, `redirect_uri`, `scope`, `state`, `code_challenge`,
    /// `code_challenge_method=S256`).
    fn authorization_url(&self, attempt: &PendingAuthorization) -> String;
    /// POST the token endpoint, `grant_type=authorization_code`. Blocking;
    /// ≤ 30 s.
    fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, AuthError>;
    /// POST the token endpoint, `grant_type=refresh_token`. Blocking;
    /// ≤ 30 s.
    fn refresh(&self, refresh_token: &str) -> Result<TokenSet, AuthError>;
    /// GET the profile endpoint with a bearer token. Blocking; ≤ 30 s
    /// (FR-008 budget).
    fn fetch_profile(&self, access_token: &str) -> Result<Profile, AuthError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_set_debug_never_prints_token_bytes() {
        let tokens = TokenSet {
            access_token: "super-secret-access".to_string(),
            refresh_token: Some("super-secret-refresh".to_string()),
            expires_in: Duration::from_secs(3600),
            scope: "streaming".to_string(),
        };
        let debug = format!("{tokens:?}");
        assert!(!debug.contains("super-secret-access"));
        assert!(!debug.contains("super-secret-refresh"));
    }

    #[test]
    fn profile_debug_never_prints_id_or_display_name() {
        let profile = Profile {
            id: "user-12345".to_string(),
            display_name: Some("Alex Example".to_string()),
            tier: Tier::Premium,
        };
        let debug = format!("{profile:?}");
        assert!(!debug.contains("user-12345"));
        assert!(!debug.contains("Alex Example"));
    }

    #[test]
    fn redirect_uri_formats_loopback_with_port() {
        let config = ClientConfig {
            client_id: "id",
            redirect_path: "/login",
            scopes: &[],
            authorize_url: "",
            token_url: "",
            profile_url: "",
        };
        assert_eq!(config.redirect_uri(12345), "http://127.0.0.1:12345/login");
    }
}
