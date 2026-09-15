# Phase 0 Research: First Launch Disclosure and Sign-In

**Feature**: 002-first-launch-and-sign-in | **Date**: 2026-09-15

Facts below were verified on 2026-09-15 against crates.io metadata, the librespot
repository (tag `v0.8.0` and `dev`), developer.spotify.com, and the delivered
001 codebase in this repository. Items that could not be verified are listed in
§ "Open verifications" and are carried into tasks as explicit spike steps.

## R1. A-2 spike — does a browser OAuth token work for the Connect receiver?

**Decision**: **PASS.** A token obtained through Spotify's authorization-code +
PKCE flow can be passed to `librespot_core::authentication::Credentials::with_access_token(token)`;
`Session::connect(creds, true)` performs the Access Point login and returns a
*reusable credentials blob* (`APWelcome.reusable_auth_credentials`) that does
not expire. librespot never uses the OAuth refresh token itself.

**Evidence**:
- librespot 0.8.0 (2025-11-10, MIT, MSRV 1.85): `core/src/authentication.rs`
  `Credentials::with_access_token` sets `auth_type = AUTHENTICATION_SPOTIFY_TOKEN`;
  `core/src/connection/mod.rs` `authenticate` returns the reusable credentials;
  `core/src/session.rs` `Session::connect(credentials, store_credentials)`.
- Maintainer (kingosticks, librespot PR #1309, 2024-10-02): "We don't refresh our
  access token. We use it to login and obtain reusable credentials… This blob
  does not expire and can be used instead of an access token next time."
- `dev` branch `examples/get_creds.rs`: OAuth with scope `["streaming"]` →
  `with_access_token` → `session.connect(creds, true)`.
- Non-Premium accounts fail AP login with `ErrorCode::PremiumAccountRequired`
  (`protocol/proto/keyexchange.proto`).

**Two caveats the design must respect**:
1. `SessionConfig::client_id` for the *receiver session* (feature 003) must be
   Spotify's own "Keymaster" client id (`65b708073fc0480ea92a077233ca87bd`,
   `core/src/config.rs`) — third-party ids cannot access the internal APIs.
   The *browser* token may be minted by any client id whose scopes include
   `streaming`.
2. Which client id mints the browser token affects Web API behaviour (R3/R4).

**Contract to 003**: this feature stores the OAuth token set (access + refresh)
in the OS secure store under one entry. 003 obtains the access token through
`AccountSession`'s service, calls `Credentials::with_access_token`, and may
store the resulting reusable blob as a *second* secure-store entry registered
as account-scoped (so sign-out deletes it). The AP `PremiumAccountRequired`
result is 003's authoritative tier signal and may set the tier to `free`.

**Alternatives rejected**: username/password (removed from librespot 0.8:
"Password authentication no longer supported, use OAuth"); Zeroconf/Connect
discovery credentials (different blob, must not be persisted per Spotify's
hardware docs); Device Authorization Grant (not available to third parties,
librespot #1501).

## R2. Browser → app handoff: loopback listener with dynamic port

**Decision**: loopback redirect `http://127.0.0.1:{port}/login` where `{port}`
is an ephemeral port bound at attempt start (RFC 8252 §7.3), with `state`
verification. Implemented with `std::net::TcpListener` only (R8). Custom URI
schemes rejected for this slice.

**Rationale**: Spotify's redirect-URI rules (developer.spotify.com, "Redirect
URIs", fetched 2026-09-15): HTTP is allowed only for loopback literals;
`localhost` is not allowed (`127.0.0.1` / `[::1]` are); a loopback URI may be
registered without a port and the request may add a dynamically assigned port.
librespot's default redirect is `http://127.0.0.1:5588/login` and maintainers
state the host must be `127.0.0.1` and the path `/login` with the Keymaster id.
A fixed port fails when busy (a known librespot user complaint). Custom
schemes need per-OS handler registration (Info.plist, registry, `.desktop`)
plus a single-instance IPC story, and Spotify documents them only for iOS.

**Honest consequence for FR-018 (app closed mid-flow)**: with loopback the
authorization code is delivered only to a *live* listener. If the app exits
before the browser redirects, the redirect fails in the browser and the code
is not received. "Attempt to complete" on relaunch therefore means: read the
pending attempt from the secure store; if younger than 5 minutes **and** the
recorded port can be re-bound, re-open the listener on that port and return to
the "Waiting for your browser…" state for the remainder of the window (a user
who reloads the browser's failed redirect tab, or presses **Open the browser
again**, completes sign-in — the PKCE verifier and `state` are unchanged, and
an unused code is still exchangeable within its validity). Otherwise the
pending state is deleted and the sign-in step shows "Your previous sign-in
didn't finish". This is recorded in the spec's assumption list as the
transport-level interpretation of EC-1.1.

## R3. Tier check — `GET https://api.spotify.com/v1/me`

**Decision**: tier = `product` field: `"premium"` → `premium`; `"free"` /
`"open"` → `free`; field absent or unparseable → `unknown`. `display_name`
(nullable → fall back to `id`) and `id` populate `AccountSession`. Scope
`user-read-private` is requested for this. HTTP status mapping: `401` →
credential invalid (try one refresh; if the refresh is definitively rejected →
revocation path FR-019); `403` → treated as `unknown` tier with "couldn't
verify" (dev-mode allowlist case); `429` → transient, honour `Retry-After` up
to the 30 s budget; transport error / timeout → transient → `unknown`.

**Rationale**: the reference marks `product` `deprecated: true` and the
February 2026 changelog removed it (with `country`, `email`, `followers`,
`explicit_content`) for *Development Mode* apps; Extended-Quota and first-party
apps keep it. The spec already defines the `unknown` tier precisely for "the
service did not tell us", so the design degrades gracefully whichever client
id is used. The authoritative Premium signal arrives with 003 (R1).

## R4. Token refresh and developer-app policy

**Decision**: `POST https://accounts.spotify.com/api/token` with form body
`grant_type=refresh_token&refresh_token=…&client_id=…` (PKCE: no
`Authorization` header). Persist the new `refresh_token` when the response
carries one, otherwise keep the old one. `error=invalid_grant` is definitive →
revocation path (FR-019). Transport errors, 5xx and 429 are transient →
backoff (FR-014). Store `authorized_at` alongside the tokens.

**Facts**: response `{access_token, token_type, expires_in (3600), scope,
refresh_token?}`; docs hedge on rotation ("when a refresh token is not
returned, continue using the existing token"). Refresh tokens issued to
Dashboard apps now live **6 months**; refreshing does not extend the lifetime;
after that the token endpoint answers `invalid_grant` and the user must
re-authorize — the FR-019 path covers this with the "revoked" wording (a
softer "sign-in expired" wording is a later string change, not a design
change).

**Client id decision**: new Developer-Dashboard apps are in *Development
Mode*: max 5 allow-listed users, owner must hold Premium, Extended Quota only
for organisations with ≥ 250k MAU (since 2025-05-15). Shipping our own
dev-mode client id is therefore unusable for an open-source desktop client.
**Decision**: ship librespot's Keymaster client id as the default
`ClientConfig { client_id, redirect_path: "/login", scopes: ["streaming",
"user-read-private", "user-read-email"] }` — the same posture librespot
itself takes and a prerequisite of 003 regardless — with an environment
override `MODPLAYER_OAUTH_CLIENT_ID` (developer/BYO-app use; no UI). This is a
governance-level fact (GOV-6, Q-4 legal review) recorded in plan.md
Complexity Tracking; all of it is one constant struct so a different legal
outcome changes one file.

## R5. OS secure store — `keyring` 4.2 behind an adapter crate

**Decision**: new adapter crate `modplayer-secure-store` wrapping
`keyring = "4.2"` (feature `v1`, default platform stores): macOS Keychain via
`apple-native-keyring-store` (`security-framework`), Windows Credential
Manager via `windows-native-keyring-store` (`windows-sys`), Linux Secret
Service via `zbus-secret-service-keyring-store` (`crypto-rust`, pure Rust —
no libdbus; zbus 5 is already in the lock file through AccessKit). One
`SecureStore` trait with two implementors: `KeyringSecureStore` (production)
and `MemorySecureStore` (tests; fault injection `set_unavailable(true)`).
Secrets are written with `Entry::set_secret(&[u8])` as compact JSON.

**Facts**: keyring 4.2.0 (2026-08-29), MIT OR Apache-2.0, MSRV 1.88 (our
toolchain is 1.95). API lives in `keyring-core` 1.0; `Entry::new(service,
user)`, `set_secret/get_secret/delete_credential`. Errors:
`NoStorageAccess` (locked), `PlatformFailure`, `NoEntry`, `NoDefaultStore`,
`TooLong(name, max)`. **Windows blob limit 2560 bytes** — the credential JSON
(access ≈ 300 chars, refresh ≈ 130 chars, two timestamps, scope) is ≈ 600
bytes; a unit test asserts the serialised credential stays < 2048 bytes.
`keyring-core`, the Apple and zbus stores contain no `unsafe`; the Windows
store wraps `CredReadW/CredWriteW` FFI. Our adapter crate keeps
`#![forbid(unsafe_code)]`; the constitution's "keychain" exception is only
needed transitively (recorded in the Constitution Check).

**CI**: unit/integration tests use `MemorySecureStore`. A `#[ignore =
"manual: needs unlocked OS store"]` round-trip test exists for the real store;
on Linux CI it can be enabled later with `gnome-keyring-daemon
--components=secrets --unlock` under `dbus-run-session` (keyring-rs's own CI
recipe) — not required for this slice's gates.

**Alternatives rejected**: `keyring-core` + store crates directly (more
wiring, same result); `secret-service`/`security-framework`/`windows-sys`
directly (three code paths to maintain); DPAPI/encrypted file (violates
Constitution VI: "OS credential store only").

## R6. HTTP client — `ureq` 3.4 on worker threads, no async runtime

**Decision**: `ureq = "3.4"` (default `rustls` + `gzip`, plus `json`) called
from dedicated `std::thread`s; results flow back over `std::sync::mpsc` and
are drained in `AccountService::tick()` on the UI thread (the same pattern as
001's `DeviceWatcher` → `PlaybackController::tick`). No tokio in this slice.

**Facts**: ureq 3.4.2 (2026-09-13), MIT OR Apache-2.0, MSRV 1.85,
`#![forbid(unsafe_code)]`, rustls 0.23 + `webpki-roots`. librespot-core 0.8
uses `hyper-rustls 0.27` on rustls ^0.23 → one rustls copy when 003 lands.
`AuthorizationService` is a plain synchronous trait, so a tokio task in 003
can call it via `spawn_blocking` without change.

**Licence impact** (`deny.toml`): `webpki-roots` is `CDLA-Permissive-2.0`
and `ring` is `Apache-2.0 AND ISC` — both must be added to the allow-list
with a comment naming the crate (the existing convention). `cargo deny check`
is the gate.

**Alternatives rejected**: `reqwest` 0.13 (drags tokio + hyper now; librespot
pins reqwest 0.12 → duplicate stacks); `oauth2` 5 crate (defaults to reqwest;
the PKCE flow is ~100 lines with `sha2` + `base64`).

## R7. Opening the system browser — `egui::Context::open_url`

**Decision**: no new dependency. The UI opens URLs with
`ctx.open_url(egui::OpenUrl::new_tab(url))`, which eframe/egui-winit forwards
to the `webbrowser` crate already in `Cargo.lock`. The account service only
*produces* the authorization URL; opening it (and "Open the browser again")
is a UI action, which keeps the service headless-testable.

**Alternatives rejected**: `open` 5.4 (adds a crate; one `unsafe` fork on
unix); `webbrowser` as a direct dependency (already reachable through egui).

## R8. Loopback listener — std only, non-blocking accept with deadline

**Decision**: `TcpListener::bind(("127.0.0.1", 0))`, read the port from
`local_addr()`, `set_nonblocking(true)`, poll `accept()` every 50 ms while
checking a cancel flag and the 5-minute deadline; parse only the request line;
accept the first request whose query has `state` equal to the attempt's and
either `code` or `error`; answer other paths (`/favicon.ico`, probes) with
`404` without consuming the attempt; reply `200 text/html` "You can close this
tab and return to ModPlayer" (`Connection: close`, `Content-Length`); drop the
listener. IPv4 only, matching the registered redirect literal.

**Rationale**: `tiny_http` last released 2022-10; librespot-oauth uses a raw
blocking `TcpListener` with no timeout or cancel — exactly what FR-016 needs
fixed. A non-blocking poll loop is portable, cancelable, and testable by
connecting with `TcpStream` in the test.

## R9. Disclosure snapshot guard — SHA-256 pin, no snapshot framework

**Decision**: `disclosure.rs` exposes `DISCLOSURE_BUNDLE_VERSION: u32` and
`DISCLOSURE_EN_US_SHA256: &str`; a test hashes the `include_str!`-ed
`locales/en-US/disclosure.ftl` source with `sha2` and fails with the message
"disclosure/privacy text changed: bump DISCLOSURE_BUNDLE_VERSION if the
substance changed, then update DISCLOSURE_EN_US_SHA256" when the hash differs.
This fails CI on *any* text change until a human decides whether it is
substantive (typo → hash only; substance → version + hash), satisfying SC-007.
`sha2` is needed for PKCE `S256` anyway.

**Alternatives rejected**: `insta` (needs the `cargo insta` workflow and
`.snap` files for a one-assert requirement).

## R10. Timestamps and clocks — `time` 0.3 + injectable `Clock`

**Decision**: `time = "0.3"` with `formatting`, `parsing`, `serde-well-known`
for `acknowledged_at`, `expires_at`, `authorized_at`, `last_validated_at`
stored as RFC 3339 UTC. A `Clock` trait (`now_utc()`, `now_instant()`) with
`SystemClock` and a test `FakeClock` (`Arc<Mutex<…>>`, `advance(d)`) is
injected into the refresh scheduler and the pending-attempt age check so the
5-minute / 30-second / backoff rules are unit-tested without sleeping.
Monotonic `Instant`s drive in-process deadlines; only wall-clock values are
persisted.

**Facts**: time 0.3.55 (2026-08-01), MIT OR Apache-2.0, MSRV 1.88; already a
dependency of librespot-core 0.8 (no extra crate when 003 lands). `jiff`
(pre-1.0) and `chrono` (larger) rejected.

## R11. Where the account service lives — new `modplayer-account` crate

**Decision**: a new library crate `modplayer-account` (component: INT-1
account/session service) holding `AccountService` (state machine + worker
threads), the PKCE flow, the loopback listener, the `AuthorizationService`
trait with `SpotifyAuthorizationService` (ureq) and `FakeAuthorizationService`,
the account-state file store, the account-scoped store registry, the refresh
scheduler, and the disclosure bundle constants. It depends on
`modplayer-secure-store` and **not** on `modplayer-core`; the UI crate
depends on both and maps `AccountEvent`s to `NotificationCenter` calls. The
device-scoped `DisclosureAcknowledgement` is persisted in 001's
`settings.toml` (core's settings model), because it is a non-account setting.

**Rationale**: Constitution VII wants one crate per component and IV wants
service-specific protocol knowledge confined; putting Spotify endpoint
knowledge into `modplayer-core` would make every 001 crate's dependency graph
carry ureq/rustls/keyring. Keeping `account` free of `core` avoids a cycle
(core's controller never needs the account) and lets 003's receiver crate
depend on `modplayer-account` alone for its credential.

**Alternatives rejected**: modules inside `modplayer-core` (dependency bloat,
IV spirit); a single crate for account + secure store (platform adapter must
be its own crate per Constitution X).

## R12. Non-secret session data — `account.toml` next to `settings.toml`

**Decision**: `AccountSession`'s non-secret fields are persisted in
`account.toml` in the same config directory (`MODPLAYER_CONFIG_DIR` override
honoured), written with the same tmp + `sync_all` + `rename` atomic-replace
routine as 001. The routine is a 12-line function re-implemented in
`modplayer-account::state_store` because `account` must not depend on
`core`; sharing it would require a third utility crate for one function. The
file is registered as an account-scoped store, so sign-out/revocation =
delete file. Only `credential_ref = "session-credential"` (the secure-store
entry name) is written, never the credential.

**Rationale**: a separate file makes "delete all account-scoped state" a
single unlink and keeps the device-scoped `settings.toml` untouched by
sign-out (FR-015); mixing both into one file would require partial rewrites.

## R13. Randomness for PKCE verifier and `state` — `getrandom` 0.3

**Decision**: `getrandom = "0.3"` fills 32 random bytes for the verifier and
16 for `state`; `base64 = "0.22"` (URL-safe, no padding) encodes them and the
`S256` challenge. Both crates are MIT OR Apache-2.0 and pure Rust wrappers over
OS entropy (getrandom has platform `unsafe` internally, is already transitive
via `ring`, and is the standard choice).

**Alternatives rejected**: `rand` (bigger API than needed); hand-rolled base64
(30 lines of avoidable code with an existing well-audited crate).

## Open verifications (spike steps carried into tasks)

1. Whether `GET /v1/me` returns `product` for tokens minted with the Keymaster
   client id (design tolerates absence via `unknown`).
2. Whether Spotify's Keymaster id accepts an ephemeral-port redirect
   `http://127.0.0.1:{port}/login` (librespot uses both `:5588` and port-less
   variants, strongly implying port-less registration). Fallback if rejected:
   fixed default port 5588 with a retry over a small range.
3. PKCE refresh-token rotation semantics (code handles both present/absent).
4. A live-store round-trip on each OS (manual quickstart scenario).

## Dependency additions summary (Constitution X justification)

| Crate | Version | Licence | Why std / existing deps are insufficient |
|---|---|---|---|
| keyring (+ platform stores) | 4.2 | MIT/Apache-2.0 | OS credential stores are FFI on three platforms; Constitution VI mandates them |
| ureq | 3.4 | MIT/Apache-2.0 | HTTPS to token/profile endpoints; std has no TLS or HTTP |
| sha2 | 0.10 | MIT/Apache-2.0 | PKCE S256 and the disclosure hash pin |
| base64 | 0.22 | MIT/Apache-2.0 | URL-safe base64 for PKCE |
| getrandom | 0.3 | MIT/Apache-2.0 | CSPRNG for verifier/state |
| time | 0.3 | MIT/Apache-2.0 | RFC 3339 timestamps; shared with librespot later |
| serde_json | 1 | MIT/Apache-2.0 | Token/profile JSON and the secure-store payload (serde + toml already present) |
| proptest (dev-only) | 1 | MIT/Apache-2.0 | Constitution VIII names proptest for state-serialization round-trips (`account.toml`, credential payload); std has no property-testing framework |
