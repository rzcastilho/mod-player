# Contract: Account read integration delta (extends 002 contracts/authorization-service.md)

**Crate**: `modplayer-account` | **Implementors**: `SpotifyAuthorizationService` (ureq), `FakeAuthorizationService` (scripted)

## 1. `AuthorizationService` additions

```rust
/// Read-only catalogue/player lookups used by "Play from account" (FR-022)
/// and the transfer banner / launch-state query (FR-016, FR-019; research R3, R9).
fn fetch_recently_played(&self, access_token: &str, limit: u8) -> Result<Vec<TrackRef>, AuthError>;
fn fetch_saved_tracks(&self, access_token: &str, limit: u8) -> Result<Vec<TrackRef>, AuthError>;
fn fetch_playback_state(&self, access_token: &str) -> Result<Option<PlaybackStateSummary>, AuthError>;
```

| Method | Endpoint | Notes |
|---|---|---|
| `fetch_recently_played` | `GET /v1/me/player/recently-played?limit={limit}` | `limit` clamped 1..=50; de-duplicated by track id preserving first occurrence; local-file/podcast items skipped |
| `fetch_saved_tracks` | `GET /v1/me/tracks?limit={limit}` | same mapping |
| `fetch_playback_state` | `GET /v1/me/player` | `204` → `Ok(None)`; `200` → `Some(PlaybackStateSummary { device_name: Option<String>, device_id: Option<String>, is_playing: bool })` |

`PlaybackStateSummary` and `TrackRef` mapping: `id ← "uri"` (fallback
`spotify:track:{id}`), `title ← "name"`, `artists ← artists[].name`,
`album ← album.name`, `artwork_url ← album.images[0].url`,
`duration_ms ← duration_ms`, `availability ← is_playable == false ? Unavailable : Available`
(missing `is_playable` = Available).

`AuthError` gains `Forbidden` (HTTP 403 — scope not granted) distinct from
`Unauthorized` (401 → 002's revocation path is **not** triggered by these
read calls; they surface as `Forbidden`/`Transport` to the caller only).
Budget: 10 s timeout each, worker thread, result delivered through
`AccountService::tick()` as `AccountEvent::ReadResult { request_id, result }`
so the UI never blocks.

## 2. `ClientConfig.scopes`

Adds `user-read-recently-played`, `user-library-read`,
`user-read-playback-state` to 002's list. Existing sessions keep working;
the new calls return `Forbidden` until the user signs in again. The
sign-in screen's scope list text (if shown) and the disclosure text are
**not** changed (the disclosure hash pin stays valid); Settings › Account
gains no new control.

## 3. `AccountService` additions

| Method | Behaviour |
|---|---|
| `request_recent_tracks(limit) -> RequestId` | worker: `fetch_recently_played`; on empty result falls back to `fetch_saved_tracks`; event `ReadResult { RecentTracks(Vec<TrackRef>) }` |
| `request_playback_state() -> RequestId` | event `ReadResult { PlaybackState(Option<PlaybackStateSummary>) }` |
| `access_token() -> Option<String>` | the current credential's access token, read from the secure store on demand (used by the binary's `ReceiverCredentials` impl); never logged |

Requests carry the 002 `attempt_id`; a result for a stale attempt (sign-out
in between) is dropped.

## 4. Tests

- `FakeAuthorizationService` scripts: `RecentTracks(Ok(vec))`,
  `RecentTracks(Err(Forbidden))`, `SavedTracks(..)`, `PlaybackState(..)`.
- `tests/reads.rs`: recently-played parse fixture (real response shape with
  a local-file item and a duplicate); 204 handling; 403 → `Forbidden`;
  fallback to saved tracks on empty; stale attempt dropped.
- Credential-leak test extended: `Debug` of the new request/response types
  never contains the token.
