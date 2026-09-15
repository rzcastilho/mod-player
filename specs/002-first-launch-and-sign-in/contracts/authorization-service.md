# Contract: Authorization Service and Loopback Callback (`modplayer-account`)

Covers INT-1 transport: the PKCE flow, token exchange/refresh, the profile
call used as tier check, and the loopback HTTP callback. Everything
service-specific (endpoints, client id, scopes, response shapes) lives in
`crates/modplayer-account/src/spotify.rs`; the rest of the crate uses the
trait below.

## ClientConfig

```rust
pub struct ClientConfig {
    pub client_id: &'static str,      // default: librespot Keymaster id (research R4);
                                       // overridden by env MODPLAYER_OAUTH_CLIENT_ID at startup
    pub redirect_path: &'static str,  // "/login"
    pub scopes: &'static [&'static str], // ["streaming", "user-read-private", "user-read-email"]
    pub authorize_url: &'static str,  // "https://accounts.spotify.com/authorize"
    pub token_url: &'static str,      // "https://accounts.spotify.com/api/token"
    pub profile_url: &'static str,    // "https://api.spotify.com/v1/me"
}
```

`redirect_uri(port) = format!("http://127.0.0.1:{port}{redirect_path}")`.

## Trait

```rust
pub trait AuthorizationService: Send + Sync + 'static {
    /// Pure. Builds the browser URL for `attempt` (response_type=code, client_id,
    /// redirect_uri, scope, state, code_challenge, code_challenge_method=S256).
    fn authorization_url(&self, attempt: &PendingAuthorization) -> String;
    /// POST token_url grant_type=authorization_code. Blocking; ≤ 30 s.
    fn exchange_code(&self, code: &str, verifier: &str, redirect_uri: &str) -> Result<TokenSet, AuthError>;
    /// POST token_url grant_type=refresh_token. Blocking; ≤ 30 s.
    fn refresh(&self, refresh_token: &str) -> Result<TokenSet, AuthError>;
    /// GET profile_url with Bearer token. Blocking; ≤ 30 s (FR-008 budget).
    fn fetch_profile(&self, access_token: &str) -> Result<Profile, AuthError>;
}

pub struct TokenSet { pub access_token: String, pub refresh_token: Option<String>,
                      pub expires_in: Duration, pub scope: String }
pub struct Profile  { pub id: String, pub display_name: Option<String>, pub tier: Tier }

pub enum AuthError {
    /// DNS/TCP/TLS/timeout/5xx/429-exhausted: retryable.
    Transient,
    /// Service says the grant/token is invalid or revoked (token endpoint
    /// `invalid_grant`, profile 401 after one refresh): NOT retryable.
    Rejected,
    /// Token valid but call forbidden (403): keep session, tier Unknown.
    Forbidden,
    /// User cancelled in the browser (`error=access_denied` on the callback).
    Cancelled,
    /// Any other definitive error from the service (`error=<other>`, 400 malformed).
    Service(String /* error code only, never the description body */),
}
```

`TokenSet` and `Profile` implement redacting `Debug`. `AuthError::Service`
carries only the OAuth `error` code (e.g. `invalid_request`), never response
bodies.

## Implementors

| Type | Notes |
|---|---|
| `SpotifyAuthorizationService` | `ureq 3.4` agent with `timeout_global(30 s)`, `User-Agent: ModPlayer/<version>`. Form-encoded POSTs; JSON responses parsed with `serde_json` into private wire structs. 429 with `Retry-After ≤ remaining budget` is retried once inside the budget, else `Transient`. |
| `FakeAuthorizationService` | Scripted: `Arc<Mutex<Script>>` with per-call queues of `Result`s, call log, optional per-call `Duration` delay (to exercise the 30 s timeout with `FakeClock`), and a `browser: Option<Box<dyn Fn(&str) + Send>>` hook that, when set, simulates the user's browser by connecting to the loopback callback URL with a scripted `code`/`error` and the attempt's `state`. |

## PKCE flow (owned by `modplayer-account::pkce` + `listener`)

1. `getrandom` 32 bytes → `verifier` (base64url, no pad); `challenge = base64url(sha256(verifier))`; 16 bytes → `state`.
2. Bind `TcpListener` `127.0.0.1:0` → `port`; write `PendingAuthorization` to the secure store (after the probe succeeded); spawn the listener thread with the cancel flag and deadline `started_at + 5 min`.
3. Return the authorization URL to the UI, which opens it (`egui::Context::open_url`). "Open the browser again" re-opens the same URL.
4. Listener resolves with one of: `Callback { code }`, `Callback { error }`, `Cancelled`, `TimedOut`, `BindLost`.
5. On `code`: a worker thread calls `exchange_code` → on `Ok`, write `SessionCredential` to the secure store **first**, then write `account.toml` (tier `Unknown`), delete `pending-authorization`, emit `Authorized`; then `fetch_profile` → emit `TierChecked`.
6. On any other outcome: delete `pending-authorization`, emit `SignInFailed(note)`.

## Loopback callback HTTP contract

| Request | Response |
|---|---|
| `GET {redirect_path}?code=…&state=<match>` | `200 OK`, `Content-Type: text/html; charset=utf-8`, body = externalised "You can close this tab and return to ModPlayer." page; listener stops. |
| `GET {redirect_path}?error=<e>&state=<match>` | `200 OK`, same page with "Sign-in was not completed." line; listener stops with `Callback { error }`. |
| `GET {redirect_path}?…&state=<mismatch>` or missing `state` | `400 Bad Request`, plain text; listener **keeps waiting** (a stray request must not consume the attempt). |
| Any other path (e.g. `/favicon.ico`) | `404 Not Found`; listener keeps waiting. |
| Anything that is not `GET` / unparsable request line | `400`; keeps waiting. |

Only the request line (first line, ≤ 8 KiB) is parsed; headers and body are
ignored. `Connection: close` and `Content-Length` are always sent. The HTML
contains no scripts and no external resources.

Resumed attempts (relaunch within 5 minutes) re-bind the **recorded** port;
if the bind fails the attempt is discarded with note `PreviousDidNotFinish`.

## Timeouts and budgets

| Operation | Budget | On expiry |
|---|---|---|
| Waiting for browser | 5 min from `started_at` | `TimedOut` → sign-in step, note, pending state deleted |
| `exchange_code` | 30 s | `Transient` → treated as `ServiceError` note (no session created) |
| `fetch_profile` (tier check) | 30 s | tier `Unknown`, credential kept |
| `refresh` | 30 s | `Transient` → backoff |
