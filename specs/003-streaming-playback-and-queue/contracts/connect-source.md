# Contract: `modplayer-audio-source-connect` — the Connect receiver crate

**Crate**: `crates/modplayer-audio-source-connect`, version `0.1.0`, versioned
**independently** of the workspace (A-11, Constitution IV): its version is
bumped on its own cadence; the `modplayer` binary depends on it with a caret
range, and no other crate depends on it (`cargo tree -i` guard test in the
binary's build). `#![forbid(unsafe_code)]`. Only this crate imports
`librespot-*` (research R1).

## 1. Public API

```rust
pub struct ConnectConfig {
    pub device_name: DeviceName,
    pub device_id: String,               // 32 hex chars, from settings
    pub tmp_dir: PathBuf,                // research R6
    pub credentials: Arc<dyn ReceiverCredentials>,
    pub status_page_url: &'static str,   // passed through for logging only
}

/// Implemented by the `modplayer` binary over SecureStore + SessionCredential.
pub trait ReceiverCredentials: Send + Sync + 'static {
    /// The current OAuth access token (scope `streaming`). Called on every
    /// (re)connect. Errors map to SourceHealth::Transient.
    fn access_token(&self) -> Result<String, CredentialError>;
}

pub struct ConnectSource { /* impl SourceHost, Rt = ConnectRtSource */ }
impl ConnectSource {
    pub fn new(config: ConnectConfig) -> Self;   // no I/O; the worker thread starts on Initialize
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
}
```

`Debug` for `ConnectConfig`/`ConnectSource` never prints the token (002's
redaction rule; the credential-leak test scans it).

## 2. Command → librespot mapping

| `SourceCommand` | Worker action |
|---|---|
| `Initialize` | spawn worker thread if absent → tokio runtime → `AudioFetchParams::set` (once per process; ignore `Err` if already set) → `Session::new(SessionConfig { device_id, tmp_dir, client_id: KEYMASTER (default), .. }, None)` → `Player::new(PlayerConfig { gapless: true, position_update_interval: Some(250 ms), normalisation: false, .. }, session, Box::new(NoOpVolume), || Box::new(RingSink::new(producer)))` → `Spirc::new(ConnectConfig { name, device_type: Computer, initial_volume, disable_volume: false, volume_steps: 64, .. }, session, Credentials::with_access_token(token), player, host_mixer)` → spawn the Spirc future → `Registered` |
| `Deregister` | `spirc.disconnect(true)` then `spirc.shutdown()`; drop Player; `Deregistered` |
| `Shutdown` | as `Deregister` + runtime shutdown with a 2 s timeout; purge `tmp_dir` |
| `Retry` | drop everything, `Initialize` |
| `SetDeviceName` | `shutdown` current Spirc, re-`Spirc::new` with the new name on the same session and player; re-send the last program at the last known position if a track was loaded |
| `LoadProgram(p)` | store `ProgramMap { generation, order }`; `spirc.repeat(p.repeat_all)`; `spirc.repeat_track(p.repeat_one)`; `spirc.load(LoadRequest::from_tracks(order_uris, LoadRequestOptions { start_playing, seek_to: best_position_estimate_or(p.position_ms), playing_track: Some(Index(cursor_index)), context_options: Some(Options { shuffle: false, repeat: p.repeat_all, repeat_track: p.repeat_one }) }))`; if the current track is within the 30 s preload window and the next item changed, `player.preload(next)` |
| `Play`/`Pause`/`Stop` | `spirc.play()` / `spirc.pause()` / `spirc.pause()` + `spirc.set_position_ms(0)` (+ `Stopped` event synthesised) |
| `Seek(ms)` | `spirc.set_position_ms(ms)` |
| `SkipNext`/`SkipPrev` | `spirc.next()` / `spirc.prev()` |
| `SetVolume(pct)` | `spirc.set_volume(to_u16(pct))` (echo suppressed) |
| `RequestTransferHere` | `spirc.transfer(None)` when the cluster has another active device, else `spirc.activate()` (open verification 1) |
| `ReportState` | no-op beyond Spirc's own reporting; Spirc already notifies within ≤ 200 ms (`UPDATE_STATE_DELAY`) |

While `!active` (after `BecameInactive`), Spirc ignores play/pause/seek by
design ("Does nothing if we are not the active device"); the host does not
send them (FR-018).

## 3. `PlayerEvent` → `SourceEvent` mapping

| `PlayerEvent` | `SourceEvent` | Marker pushed to RT |
|---|---|---|
| `TrackChanged { audio_item }` | `TrackStarted { track: TrackRef::from(audio_item), program: map.index_of(uri, expected_next), position_ms: 0, playing }` | `TrackStart { seq+1, at: written_frames }` |
| `Loading { position_ms }` | `Loading` | — |
| `Playing { position_ms }` | `Playing` | `Reposition { position, at: written_frames }` |
| `Paused` / `Stopped` | `Paused` / `Stopped` | — |
| `Seeked` / `PositionCorrection` | `Seeked` | `Reposition` |
| `EndOfTrack` | `EndOfTrack` | `TrackEnd { at: written_frames }` |
| `Unavailable { track_id }` | `Unavailable` | — |
| `VolumeChanged { volume }` (not an echo) | `RemoteCommand(Volume(pct))` | — |
| `ShuffleChanged` / `RepeatChanged` | `RemoteCommand(Shuffle/Repeat)` | — |
| `SessionConnected` (after `SessionDisconnected` or first) | `BecameActive { context }` — `context` is `Some` when the activation was not requested by the host and a `TrackChanged` follows within 1 s (transfer-in); the worker waits for that `TrackChanged` before emitting | — |
| `SessionDisconnected` | `BecameInactive` | — |
| `PositionChanged` | none (updates the worker's position estimate only) | — |

Remote `play/pause/seek/next/prev` are observed only through the resulting
`Playing/Paused/Seeked/TrackChanged` events — that *is* the mirror (FR-017).

## 4. Health classification (research R7)

| Observation | Health |
|---|---|
| `ReceiverCredentials::access_token` error | Transient |
| `Session::connect` → `Unauthenticated` | Transient (token refresh pending) |
| `Session::connect` → `PermissionDenied` | event `TierRejected`, health unchanged |
| `Session::connect` → `FailedPrecondition` / `Unimplemented` / `InvalidArgument` | Unavailable { client_update_required: true } |
| `Spirc::new` → `Internal` / `DataLoss` / protobuf decode error | Unavailable { false } |
| Spirc future ends unexpectedly / dealer error / `Aborted` / `Unavailable` / `DeadlineExceeded` / I/O | Transient; reconnect with backoff 1, 2, 4 … 60 s |
| Player `Unavailable` for a track | not health — `Unavailable` event |

Backoff state machine: identical constants to 002's `RefreshScheduler`
(start 1 s, ×2, cap 60 s, reset on success).

## 5. Threads and channels

| Thread | Owner | Purpose |
|---|---|---|
| UI thread | controller | `command()`, `poll()` |
| `connect-worker` (std thread, tokio rt 2 workers) | `ConnectSource` | everything librespot |
| `player` (spawned by librespot `Player`) | librespot | fetch/decode; calls `RingSink::write` |
| audio callback | cpal | `ConnectRtSource::fill` |

Channels: `mpsc::Sender<SourceCommand>` (UI → worker, unbounded, tokio
`mpsc` inside the runtime), `mpsc::Receiver<SourceEvent>` (worker → UI,
std, drained by `poll`), `rtrb::RingBuffer<f32>` 16 384 slots (8 192 frames)
and `rtrb::RingBuffer<Marker>` 64 slots (player/worker → RT).

## 6. Temp directory (research R6)

`tmp_dir = <OS temp>/modplayer-stream-<pid>/`, created `0o700` on Unix,
purged on `Initialize` and `Shutdown`; stale `modplayer-stream-*` siblings
older than 24 h are removed at `Initialize`. Files are librespot's encrypted
`NamedTempFile`s (deleted on drop). Never contains decoded audio
(Constitution V).

## 7. Tests pinning this contract

- Unit (no network): `health::classify` table; `volume::{to_u16, to_pct}`
  round-trip within ±1 %; `ProgramMap::index_of` prefers the expected next
  index for duplicate tracks; `RingSink` writes N samples through a ring of
  capacity < N without loss (fake consumer thread); `ConnectRtSource` under
  `assert_no_alloc`; marker sequencing (`TrackStart` after `TrackEnd` at the
  same `at` frame advances `track_seq` exactly once).
- Live, `#[ignore = "manual"]`: `plays_five_seconds_of_a_real_track` — needs
  `MODPLAYER_TEST_ACCESS_TOKEN`; asserts `Registered`, `TrackStarted`,
  `Playing` and ≥ 220 500 non-silent frames consumed.
- Binary: `receiver_crate_has_single_dependent` — `cargo metadata` shows only
  `modplayer` depending on `modplayer-audio-source-connect`.
