// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SpotifyAuthorizationService`: the concrete `AuthorizationService` over
//! Spotify's Accounts/Web API (contracts/authorization-service.md,
//! research R1/R3/R4/R6). PKCE URL building, `ureq` calls, and the 429
//! retry/timeout policy (T059).

use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use ureq::Agent;
use ureq::http::Response;

use modplayer_audio_source::{Availability, TrackId, TrackRef};

use crate::auth_service::{
    AuthError, AuthorizationService, ClientConfig, PlaybackStateSummary, Profile, REDIRECT_PATH,
    TokenSet,
};
use crate::pending::PendingAuthorization;
use crate::pkce;
use crate::session::Tier;

/// Environment override for the resolved client id (research R4; plan.md
/// Complexity Tracking — same legal posture as librespot).
pub const CLIENT_ID_ENV: &str = "MODPLAYER_OAUTH_CLIENT_ID";

/// librespot's public Keymaster client id — the default `client_id`
/// (research R4). Spotify's Development-Mode apps are capped at 5
/// allow-listed users, so an open-source client cannot ship its own
/// dashboard app; 003's receiver session must use this id regardless.
const DEFAULT_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

/// Every network call (token exchange, refresh, profile) budgets ≤ 30 s,
/// including one 429 retry (contracts/authorization-service.md "Timeouts
/// and budgets").
const BUDGET: Duration = Duration::from_secs(30);

/// The default `ClientConfig` (contracts/authorization-service.md).
/// `client_id` here is the fallback; the effective id is resolved by
/// [`resolved_client_id`], which honours [`CLIENT_ID_ENV`].
pub const DEFAULT_CONFIG: ClientConfig = ClientConfig {
    client_id: DEFAULT_CLIENT_ID,
    redirect_path: REDIRECT_PATH,
    scopes: &[
        "streaming",
        "user-read-private",
        "user-read-email",
        // 003 (contracts/account-read-delta.md §2): "Play from account", the
        // transfer banner's device name, and the launch-time active-
        // elsewhere check. Existing sessions keep working; these calls
        // return `Forbidden` until the user signs in again.
        "user-read-recently-played",
        "user-library-read",
        "user-read-playback-state",
    ],
    authorize_url: "https://accounts.spotify.com/authorize",
    token_url: "https://accounts.spotify.com/api/token",
    profile_url: "https://api.spotify.com/v1/me",
};

/// Base URL for the Web API read endpoints added in 003 (contracts/
/// account-read-delta.md §1). Not part of `ClientConfig` — these three
/// calls are specific to this implementation, the way `profile_url` covers
/// only 002's single read.
const WEB_API_BASE: &str = "https://api.spotify.com/v1";

/// Resolve the effective client id: [`CLIENT_ID_ENV`] if set and
/// non-empty, else [`DEFAULT_CLIENT_ID`].
pub fn resolved_client_id() -> String {
    resolve_client_id(std::env::var(CLIENT_ID_ENV).ok())
}

/// Pure core of [`resolved_client_id`], testable without touching the
/// process environment (this crate forbids `unsafe`, so tests cannot use
/// `std::env::set_var`).
fn resolve_client_id(env_value: Option<String>) -> String {
    env_value
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

static USER_AGENT: LazyLock<String> =
    LazyLock::new(|| format!("ModPlayer/{}", env!("CARGO_PKG_VERSION")));

/// `AuthorizationService` over Spotify's Accounts/Web API via `ureq`
/// (contracts/authorization-service.md).
pub struct SpotifyAuthorizationService {
    config: ClientConfig,
    client_id: String,
    agent: Agent,
}

impl SpotifyAuthorizationService {
    /// Build a service using [`DEFAULT_CONFIG`] with the client id
    /// resolved from [`resolved_client_id`].
    pub fn new() -> Self {
        Self::with_config(DEFAULT_CONFIG, resolved_client_id())
    }

    /// Build a service with an explicit config/client id (tests, or a
    /// future non-default deployment).
    pub fn with_config(config: ClientConfig, client_id: String) -> Self {
        let agent_config = Agent::config_builder()
            .timeout_global(Some(BUDGET))
            // Handle every status code ourselves so a 429/4xx/5xx still
            // hands back a `Response` (with headers/body) rather than an
            // opaque `Error::StatusCode` (contracts/authorization-
            // service.md's per-status mapping table).
            .http_status_as_error(false)
            .user_agent(USER_AGENT.as_str())
            .build();
        Self {
            config,
            client_id,
            agent: agent_config.into(),
        }
    }

    /// The endpoint/scope configuration this service was built with.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// The resolved OAuth client id this service authenticates as.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// POST `self.config.token_url` with `form`, following the 429-retry
    /// policy, and parse the result into a [`TokenSet`]
    /// (contracts/authorization-service.md).
    fn post_token(&self, form: &[(&str, &str)]) -> Result<TokenSet, AuthError> {
        let response = self.execute(|| {
            self.agent
                .post(self.config.token_url)
                .send_form(form.iter().copied())
        })?;
        let status = response.status().as_u16();
        let body = read_body(response)?;
        match status {
            200 => parse_token_response(&body),
            429 => Err(AuthError::Transient),
            400..=499 => Err(map_token_error_body(&body)),
            500..=599 => Err(AuthError::Transient),
            other => Err(AuthError::Service(format!("http-{other}"))),
        }
    }

    /// Run `request`, retrying exactly once on a `429` whose `Retry-After`
    /// fits within the remaining [`BUDGET`] (contracts/authorization-
    /// service.md: "429 with `Retry-After ≤ remaining budget` is retried
    /// once inside the budget, else `Transient`").
    fn execute(
        &self,
        request: impl Fn() -> Result<Response<ureq::Body>, ureq::Error>,
    ) -> Result<Response<ureq::Body>, AuthError> {
        let start = Instant::now();
        let response = request().map_err(map_transport_error)?;
        if response.status().as_u16() != 429 {
            return Ok(response);
        }
        let retry_after = retry_after_seconds(&response).unwrap_or(1);
        let remaining = BUDGET.saturating_sub(start.elapsed());
        if Duration::from_secs(retry_after) > remaining {
            return Ok(response);
        }
        thread::sleep(Duration::from_secs(retry_after));
        request().map_err(map_transport_error)
    }
}

impl Default for SpotifyAuthorizationService {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthorizationService for SpotifyAuthorizationService {
    fn authorization_url(&self, attempt: &PendingAuthorization) -> String {
        let redirect_uri = self.config.redirect_uri(attempt.port);
        let scope = self.config.scopes.join(" ");
        let challenge = pkce::challenge_for(&attempt.pkce_verifier);
        let query = [
            ("response_type", "code"),
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", scope.as_str()),
            ("state", attempt.state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ];
        let query_string = query
            .iter()
            .map(|(key, value)| format!("{key}={}", url_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        format!("{}?{query_string}", self.config.authorize_url)
    }

    fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, AuthError> {
        self.post_token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.client_id),
            ("code_verifier", verifier),
        ])
    }

    fn refresh(&self, refresh_token: &str) -> Result<TokenSet, AuthError> {
        self.post_token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
        ])
    }

    fn fetch_profile(&self, access_token: &str) -> Result<Profile, AuthError> {
        let response = self.execute(|| {
            self.agent
                .get(self.config.profile_url)
                .header("Authorization", format!("Bearer {access_token}"))
                .call()
        })?;
        let status = response.status().as_u16();
        match status {
            200 => parse_profile_response(&read_body(response)?),
            401 => Err(AuthError::Rejected),
            403 => Err(AuthError::Forbidden),
            429 => Err(AuthError::Transient),
            500..=599 => Err(AuthError::Transient),
            other => Err(AuthError::Service(format!("http-{other}"))),
        }
    }

    fn fetch_recently_played(
        &self,
        access_token: &str,
        limit: u8,
    ) -> Result<Vec<TrackRef>, AuthError> {
        let limit = limit.clamp(1, 50);
        let url = format!("{WEB_API_BASE}/me/player/recently-played?limit={limit}");
        let body = self.get_web_api(&url, access_token)?;
        Ok(parse_track_items(&body))
    }

    fn fetch_saved_tracks(
        &self,
        access_token: &str,
        limit: u8,
    ) -> Result<Vec<TrackRef>, AuthError> {
        let limit = limit.clamp(1, 50);
        let url = format!("{WEB_API_BASE}/me/tracks?limit={limit}");
        let body = self.get_web_api(&url, access_token)?;
        Ok(parse_track_items(&body))
    }

    fn fetch_playback_state(
        &self,
        access_token: &str,
    ) -> Result<Option<PlaybackStateSummary>, AuthError> {
        let url = format!("{WEB_API_BASE}/me/player");
        let response = self.execute(|| {
            self.agent
                .get(&url)
                .header("Authorization", format!("Bearer {access_token}"))
                .call()
        })?;
        let status = response.status().as_u16();
        match status {
            204 => Ok(None),
            200 => {
                let body = read_body(response)?;
                Ok(Some(parse_playback_state_response(&body)))
            }
            401 => Err(AuthError::Rejected),
            403 => Err(AuthError::Forbidden),
            429 => Err(AuthError::Transient),
            500..=599 => Err(AuthError::Transient),
            other => Err(AuthError::Service(format!("http-{other}"))),
        }
    }
}

impl SpotifyAuthorizationService {
    /// GET `url` with a bearer token, mapping the read-endpoint status
    /// table shared by `fetch_recently_played`/`fetch_saved_tracks`
    /// (contracts/account-read-delta.md §1). `fetch_playback_state` has its
    /// own caller since it also needs the `204` branch.
    fn get_web_api(&self, url: &str, access_token: &str) -> Result<String, AuthError> {
        let response = self.execute(|| {
            self.agent
                .get(url)
                .header("Authorization", format!("Bearer {access_token}"))
                .call()
        })?;
        let status = response.status().as_u16();
        match status {
            200 => read_body(response),
            401 => Err(AuthError::Rejected),
            403 => Err(AuthError::Forbidden),
            429 => Err(AuthError::Transient),
            500..=599 => Err(AuthError::Transient),
            other => Err(AuthError::Service(format!("http-{other}"))),
        }
    }
}

/// A transport-level failure (DNS/TCP/TLS/timeout — never a status code,
/// since `http_status_as_error(false)` is set) is always retryable
/// (contracts/authorization-service.md).
fn map_transport_error(_error: ureq::Error) -> AuthError {
    AuthError::Transient
}

fn read_body(mut response: Response<ureq::Body>) -> Result<String, AuthError> {
    response
        .body_mut()
        .read_to_string()
        .map_err(|_| AuthError::Transient)
}

fn retry_after_seconds(response: &Response<ureq::Body>) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "default_expires_in")]
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

fn default_expires_in() -> i64 {
    3600
}

/// Parse a `200` token-endpoint body into a [`TokenSet`] (research R4;
/// data-model.md §4: "`expires_in` missing → default 3600 s; `expires_in
/// <= 0` → treated as already expired"). A malformed body is treated as a
/// transient failure — the same fail-safe data-model.md §4 applies to
/// `/v1/me`.
fn parse_token_response(body: &str) -> Result<TokenSet, AuthError> {
    let parsed: TokenResponse = serde_json::from_str(body).map_err(|_| AuthError::Transient)?;
    Ok(TokenSet {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expires_in: Duration::from_secs(parsed.expires_in.max(0) as u64),
        scope: parsed.scope,
    })
}

#[derive(Deserialize)]
struct TokenErrorResponse {
    error: String,
}

/// Map a non-`200` token-endpoint body to the right [`AuthError`]:
/// `invalid_grant` is a definitive rejection (research R4); every other
/// OAuth `error` code is carried verbatim, never the description
/// (contracts/authorization-service.md).
fn map_token_error_body(body: &str) -> AuthError {
    match serde_json::from_str::<TokenErrorResponse>(body) {
        Ok(parsed) if parsed.error == "invalid_grant" => AuthError::Rejected,
        Ok(parsed) => AuthError::Service(parsed.error),
        Err(_) => AuthError::Service("malformed_error_response".to_string()),
    }
}

#[derive(Deserialize)]
struct ProfileResponse {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    product: Option<String>,
}

/// Parse a `200` `/v1/me` body into a [`Profile`] (research R3). A missing
/// or empty `id` is treated as a transport-level failure, per data-model.md
/// §4.
fn parse_profile_response(body: &str) -> Result<Profile, AuthError> {
    let parsed: ProfileResponse = serde_json::from_str(body).map_err(|_| AuthError::Transient)?;
    if parsed.id.is_empty() {
        return Err(AuthError::Transient);
    }
    Ok(Profile {
        id: parsed.id,
        display_name: parsed.display_name,
        tier: tier_from_product(parsed.product.as_deref()),
    })
}

/// `/v1/me` `product` mapping (research R3, data-model.md §1.7):
/// `premium → Premium`; `free | open → Free`; anything else / missing →
/// `Unknown`.
fn tier_from_product(product: Option<&str>) -> Tier {
    match product {
        Some("premium") => Tier::Premium,
        Some("free") | Some("open") => Tier::Free,
        _ => Tier::Unknown,
    }
}

#[derive(Deserialize)]
struct ArtistObject {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ImageObject {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Deserialize)]
struct AlbumObject {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    images: Vec<ImageObject>,
}

/// The Spotify "track object" shape shared by the recently-played and
/// saved-tracks item wrappers (contracts/account-read-delta.md §1 mapping
/// table).
#[derive(Deserialize)]
struct TrackObject {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    duration_ms: Option<i64>,
    #[serde(default)]
    is_playable: Option<bool>,
    #[serde(default)]
    is_local: Option<bool>,
    #[serde(default)]
    artists: Vec<ArtistObject>,
    #[serde(default)]
    album: Option<AlbumObject>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
}

/// A recently-played item (`PlayHistoryObject`) or a saved-track item
/// (`SavedTrackObject`) — both wrap the track under a `track` key.
#[derive(Deserialize)]
struct TrackItemWrapper {
    track: Option<TrackObject>,
}

#[derive(Deserialize)]
struct TrackItemsResponse {
    #[serde(default)]
    items: Vec<TrackItemWrapper>,
}

/// Map one Web API track object to a [`TrackRef`] (contracts/
/// account-read-delta.md §1: "`id ← uri` (fallback
/// `spotify:track:{id}`) ... `availability ← is_playable == false ?
/// Unavailable : Available` (missing `is_playable` = Available)").
/// `None` for a local-file/podcast item, or one whose id cannot be
/// represented as a [`TrackId`] (empty/non-ASCII/too long).
fn track_ref_from_object(track: TrackObject) -> Option<TrackRef> {
    if track.is_local == Some(true) {
        return None;
    }
    if matches!(track.kind.as_deref(), Some("episode")) {
        return None;
    }
    let uri = track
        .uri
        .filter(|uri| !uri.is_empty())
        .or_else(|| track.id.as_ref().map(|id| format!("spotify:track:{id}")))?;
    let id = TrackId::new(uri).ok()?;
    let artists = track
        .artists
        .into_iter()
        .filter_map(|artist| artist.name)
        .collect();
    let album = track.album.as_ref().and_then(|album| album.name.clone());
    let artwork_url = track
        .album
        .and_then(|album| album.images.into_iter().next())
        .and_then(|image| image.url);
    let duration_ms = track.duration_ms.unwrap_or(0).max(0) as u32;
    let availability = if track.is_playable == Some(false) {
        Availability::UnavailableRegion
    } else {
        Availability::Available
    };
    Some(TrackRef::new(
        id,
        track.name.unwrap_or_default(),
        artists,
        album,
        artwork_url,
        duration_ms,
        availability,
    ))
}

/// Parse a recently-played or saved-tracks `200` body into a de-duplicated
/// (by track id, first occurrence kept), local-file/podcast-filtered
/// `Vec<TrackRef>` (contracts/account-read-delta.md §1/§4). A malformed
/// body parses as an empty list rather than failing the whole call — the
/// caller (`AccountService`) already falls back to saved tracks on an
/// empty recently-played result.
fn parse_track_items(body: &str) -> Vec<TrackRef> {
    let Ok(parsed) = serde_json::from_str::<TrackItemsResponse>(body) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut tracks = Vec::new();
    for item in parsed.items {
        let Some(track) = item.track.and_then(track_ref_from_object) else {
            continue;
        };
        if seen.insert(track.id.clone()) {
            tracks.push(track);
        }
    }
    tracks
}

#[derive(Deserialize)]
struct DeviceObject {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct PlaybackStateResponse {
    #[serde(default)]
    device: Option<DeviceObject>,
    #[serde(default)]
    is_playing: bool,
}

/// Parse a `200` `/v1/me/player` body into a [`PlaybackStateSummary`]
/// (contracts/account-read-delta.md §1). A malformed body maps to an
/// "unknown device, not playing" summary rather than failing the call —
/// there is no reasonable retry for a body Spotify itself sent as `200`.
fn parse_playback_state_response(body: &str) -> PlaybackStateSummary {
    let parsed =
        serde_json::from_str::<PlaybackStateResponse>(body).unwrap_or(PlaybackStateResponse {
            device: None,
            is_playing: false,
        });
    PlaybackStateSummary {
        device_name: parsed
            .device
            .as_ref()
            .and_then(|device| device.name.clone()),
        device_id: parsed.device.and_then(|device| device.id),
        is_playing: parsed.is_playing,
    }
}

/// RFC 3986 percent-encoding for a single query value (unreserved:
/// `A-Za-z0-9-_.~`). `ureq`'s own encoder is private, and pulling in a
/// dedicated URL crate for one query-string builder would be the kind of
/// dependency research.md's R6/R13 rejected elsewhere in this crate for the
/// same reason (a well-scoped ~10-line function versus a new crate).
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_the_contract() {
        assert_eq!(DEFAULT_CONFIG.redirect_path, "/login");
        assert_eq!(
            DEFAULT_CONFIG.scopes,
            &[
                "streaming",
                "user-read-private",
                "user-read-email",
                "user-read-recently-played",
                "user-library-read",
                "user-read-playback-state",
            ]
        );
        assert_eq!(
            DEFAULT_CONFIG.authorize_url,
            "https://accounts.spotify.com/authorize"
        );
        assert_eq!(
            DEFAULT_CONFIG.token_url,
            "https://accounts.spotify.com/api/token"
        );
        assert_eq!(DEFAULT_CONFIG.profile_url, "https://api.spotify.com/v1/me");
    }

    #[test]
    fn resolve_client_id_falls_back_to_the_default_when_absent_or_empty() {
        assert_eq!(resolve_client_id(None), DEFAULT_CLIENT_ID);
        assert_eq!(resolve_client_id(Some(String::new())), DEFAULT_CLIENT_ID);
    }

    #[test]
    fn resolve_client_id_honours_a_non_empty_override() {
        assert_eq!(
            resolve_client_id(Some("custom-client-id".to_string())),
            "custom-client-id"
        );
    }

    #[test]
    fn new_service_exposes_its_config_and_client_id() {
        let service = SpotifyAuthorizationService::new();
        assert_eq!(service.config().redirect_path, "/login");
        assert!(!service.client_id().is_empty());
    }

    #[test]
    fn authorization_url_carries_every_pkce_parameter() {
        let service = SpotifyAuthorizationService::with_config(DEFAULT_CONFIG, "cid".to_string());
        let attempt = PendingAuthorization {
            attempt_id: 1,
            pkce_verifier: "verifier-value".to_string(),
            state: "state-value".to_string(),
            port: 4321,
            started_at: time::OffsetDateTime::UNIX_EPOCH,
        };
        let url = service.authorization_url(&attempt);
        assert!(url.starts_with("https://accounts.spotify.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A4321%2Flogin"));
        assert!(url.contains("state=state-value"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&format!(
            "code_challenge={}",
            pkce::challenge_for("verifier-value")
        )));
    }

    #[test]
    fn url_encode_leaves_unreserved_characters_alone() {
        assert_eq!(url_encode("abcXYZ019-_.~"), "abcXYZ019-_.~");
    }

    #[test]
    fn url_encode_percent_encodes_everything_else() {
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(
            url_encode("http://127.0.0.1:1/login"),
            "http%3A%2F%2F127.0.0.1%3A1%2Flogin"
        );
    }

    #[test]
    fn parse_token_response_defaults_missing_expires_in() {
        let token = parse_token_response(r#"{"access_token":"a","scope":"s"}"#).expect("parse");
        assert_eq!(token.access_token, "a");
        assert_eq!(token.expires_in, Duration::from_secs(3600));
        assert_eq!(token.refresh_token, None);
    }

    #[test]
    fn parse_token_response_clamps_non_positive_expires_in_to_zero() {
        let token = parse_token_response(r#"{"access_token":"a","expires_in":-5,"scope":"s"}"#)
            .expect("parse");
        assert_eq!(token.expires_in, Duration::ZERO);
    }

    #[test]
    fn parse_token_response_carries_a_present_refresh_token() {
        let token = parse_token_response(
            r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"scope":"s"}"#,
        )
        .expect("parse");
        assert_eq!(token.refresh_token, Some("r".to_string()));
    }

    #[test]
    fn malformed_token_response_is_transient() {
        assert!(matches!(
            parse_token_response("not json"),
            Err(AuthError::Transient)
        ));
    }

    #[test]
    fn invalid_grant_error_body_is_rejected() {
        assert_eq!(
            map_token_error_body(
                r#"{"error":"invalid_grant","error_description":"secret detail"}"#
            ),
            AuthError::Rejected
        );
    }

    #[test]
    fn other_error_codes_carry_only_the_code_never_the_description() {
        let error = map_token_error_body(
            r#"{"error":"invalid_request","error_description":"leaked secret"}"#,
        );
        assert_eq!(error, AuthError::Service("invalid_request".to_string()));
        assert_eq!(
            format!("{error}"),
            "authorization service error: invalid_request"
        );
        assert!(!format!("{error:?}").contains("leaked secret"));
    }

    #[test]
    fn tier_mapping_matches_the_product_field() {
        assert_eq!(tier_from_product(Some("premium")), Tier::Premium);
        assert_eq!(tier_from_product(Some("free")), Tier::Free);
        assert_eq!(tier_from_product(Some("open")), Tier::Free);
        assert_eq!(tier_from_product(Some("family")), Tier::Unknown);
        assert_eq!(tier_from_product(None), Tier::Unknown);
    }

    #[test]
    fn parse_profile_response_maps_every_field() {
        let profile =
            parse_profile_response(r#"{"id":"user-1","display_name":"Alex","product":"premium"}"#)
                .expect("parse");
        assert_eq!(profile.id, "user-1");
        assert_eq!(profile.display_name, Some("Alex".to_string()));
        assert_eq!(profile.tier, Tier::Premium);
    }

    #[test]
    fn parse_profile_response_rejects_an_empty_id_as_transient() {
        assert!(matches!(
            parse_profile_response(r#"{"id":"","product":"premium"}"#),
            Err(AuthError::Transient)
        ));
    }

    #[test]
    fn parse_profile_response_falls_back_to_unknown_tier_when_product_is_absent() {
        let profile = parse_profile_response(r#"{"id":"user-1"}"#).expect("parse");
        assert_eq!(profile.tier, Tier::Unknown);
        assert_eq!(profile.display_name, None);
    }

    /// contracts/account-read-delta.md §1/§4: real recently-played response
    /// shape with a local-file item (skipped) and a duplicate track id
    /// (de-duplicated, first occurrence kept).
    #[test]
    fn parse_track_items_skips_local_files_and_dedupes_by_id() {
        let body = r#"{
            "items": [
                { "track": { "uri": "spotify:track:aaa", "name": "First", "duration_ms": 1000,
                              "artists": [{"name": "Artist A"}], "album": {"name": "Album",
                              "images": [{"url": "https://img/a.jpg"}]}, "type": "track" } },
                { "track": { "uri": "spotify:local:x:y:Local+Song:120", "name": "Local Song",
                              "is_local": true, "duration_ms": 5000, "artists": [], "type": "track" } },
                { "track": { "uri": "spotify:track:aaa", "name": "First (dup)", "duration_ms": 1000,
                              "artists": [{"name": "Artist A"}], "type": "track" } },
                { "track": { "uri": "spotify:episode:zzz", "name": "A Podcast", "duration_ms": 2000,
                              "type": "episode" } }
            ]
        }"#;

        let tracks = parse_track_items(body);

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id.as_str(), "spotify:track:aaa");
        assert_eq!(tracks[0].title, "First");
        assert_eq!(tracks[0].artists, vec!["Artist A".to_string()]);
        assert_eq!(tracks[0].album, Some("Album".to_string()));
        assert_eq!(tracks[0].artwork_url, Some("https://img/a.jpg".to_string()));
    }

    #[test]
    fn parse_track_items_marks_non_playable_as_unavailable() {
        let body = r#"{"items":[{"track":{"uri":"spotify:track:bbb","name":"T",
            "is_playable": false, "duration_ms": 1000, "type": "track"}}]}"#;
        let tracks = parse_track_items(body);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].availability, Availability::UnavailableRegion);
    }

    #[test]
    fn parse_track_items_missing_uri_falls_back_to_id() {
        let body = r#"{"items":[{"track":{"id":"ccc","name":"T","duration_ms":1000,
            "type":"track"}}]}"#;
        let tracks = parse_track_items(body);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id.as_str(), "spotify:track:ccc");
    }

    #[test]
    fn parse_track_items_malformed_body_is_empty() {
        assert!(parse_track_items("not json").is_empty());
    }

    #[test]
    fn parse_playback_state_response_maps_device_fields() {
        let summary = parse_playback_state_response(
            r#"{"device":{"id":"dev-1","name":"Kitchen Speaker"},"is_playing":true}"#,
        );
        assert_eq!(summary.device_id, Some("dev-1".to_string()));
        assert_eq!(summary.device_name, Some("Kitchen Speaker".to_string()));
        assert!(summary.is_playing);
    }

    #[test]
    fn parse_playback_state_response_malformed_body_defaults_to_not_playing() {
        let summary = parse_playback_state_response("not json");
        assert_eq!(summary.device_id, None);
        assert!(!summary.is_playing);
    }
}
