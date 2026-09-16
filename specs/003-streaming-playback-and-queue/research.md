# Research: Streaming Playback, Transport, and Queue

**Feature**: 003-streaming-playback-and-queue | **Date**: 2026-09-15

Facts below were verified on 2026-09-15 against the librespot 0.8.0 sources
downloaded from crates.io (`librespot-core`, `librespot-audio`,
`librespot-playback`, `librespot-connect`, all MIT, MSRV 1.85, edition 2024)
and against the code shipped by 001/002 in this repository. Every
"NEEDS CLARIFICATION" from the Technical Context is resolved here; the
numbered decisions are referenced from plan.md, data-model.md and the
contracts.

## R1. The receiver protocol implementation — librespot 0.8 `Spirc` + `Player` + custom `Sink`

**Decision**: the Connect Audio Source (`crates/modplayer-audio-source-connect`)
is built on librespot 0.8: `librespot-core::Session` (AP login with the OAuth
access token from 002, research R1 of 002), `librespot-connect::Spirc` (device
registration, dealer websocket, remote commands, transfer, outbound state
reports), `librespot-playback::Player` (fetch → decrypt → Vorbis decode via
Symphonia, gapless preload) driving a **custom `Sink`** (`RingSink`) that
writes decoded PCM into a lock-free `rtrb` ring buffer read by the real-time
half. librespot's audio backends and mixers are disabled
(`default-features = false`, TLS feature `rustls-tls-webpki-roots`, matching
002's rustls choice); volume is handled by a `HostMixer` that forwards remote
volume to the host and never attenuates samples (`NoOpVolume`).

**Evidence** (librespot 0.8.0 sources):
- `Spirc::new(ConnectConfig, Session, Credentials, Arc<Player>, Arc<dyn Mixer>) -> (Spirc, impl Future)`;
  `ConnectConfig { name, device_type, initial_volume, disable_volume, volume_steps, .. }`.
- `Spirc` commands: `play/pause/prev/next/shuffle/repeat/repeat_track/set_volume/set_position_ms/load(LoadRequest)/disconnect(pause)/activate()/transfer(Option<TransferRequest>)/shutdown()`.
- `LoadRequest::from_tracks(Vec<String /*uri*/>, LoadRequestOptions { start_playing, seek_to, playing_track: Option<PlayingTrack::Index(u32)>, context_options })`.
- `Player::new(PlayerConfig, Session, Box<dyn VolumeGetter + Send>, sink_builder: FnOnce() -> Box<dyn Sink>) -> Arc<Player>`; `Sink { start, stop, write(AudioPacket::Samples(Vec<f64>), &mut Converter) }`.
- `Player` spawns its own thread with its own tokio runtime ("It must be run by using block_on() in a dedicated thread"); `Spirc::new` and the returned future need a tokio runtime supplied by the caller.
- `PlayerEvent` carries everything the host needs: `Loading/Playing/Paused/Stopped/Seeked/PositionCorrection/EndOfTrack/Unavailable/TrackChanged{AudioItem}/SessionConnected/SessionDisconnected/VolumeChanged/ShuffleChanged/RepeatChanged/TimeToPreloadNextTrack`.
- `Player::load` of the track already playing reuses the decoder and only seeks (`handle_command_load`, "Check if we are already playing the track. If so, just do a seek"). `PlayerConfig { gapless: true, position_update_interval: Option<Duration>, .. }`.
- Non-Premium accounts fail AP login with `PremiumAccountRequired` (002 R1) — the authoritative tier signal for FR-027.

**Rationale**: Spirc is the only maintained open implementation of the
Connect receiver dealer/connect-state protocol; re-implementing it on
`librespot-core` alone (dealer subscriptions, `PUT connect-state`, transfer
state decoding, context resolution) is several thousand lines of
protocol-tracking code — exactly the risk Constitution IV isolates. Keeping
it inside one crate behind the `AudioSource`/`SourceHost` seam satisfies
FR-002 and A-11.

**Alternatives rejected**: (a) own Connect implementation on `librespot-core`
— too large, highest drift risk; (b) `librespot-playback` with a stock
backend (rodio/cpal) — audio would bypass the 001 engine, violating
Constitution I/V and the "renders through the audio engine" requirement;
(c) a native-tls build — pulls OpenSSL on Linux; 002 already standardised on
rustls.

## R2. Who owns the queue — host-owned `Queue`, Spirc executes a host-computed *program*

**Decision**: transport and queue rules (FR-003/004/011–014) live in
`modplayer-core` (`queue.rs`, `transport.rs`) as pure, proptest-able code.
The host hands the source a **program** = the full effective play order
(TrackRef ids) plus the cursor index, position and play intent
(`SourceCommand::LoadProgram`). The Connect source maps it to
`spirc.load(LoadRequest::from_tracks(order, {playing_track: Index(cursor), seek_to, start_playing}))`
with Spirc shuffle **off** (host already shuffled), `repeat` = host repeat-all
(so Spirc wraps within the host's order and preloads across the wrap) and
`repeat_track` = host repeat-one. Spirc then advances and preloads within
that order, giving gapless transitions (`TimeToPreloadNextTrack` → preload of
the host's actual next item). The host mirrors advancement from
`TrackStarted{program_index}` events and re-sends a program whenever its
effective order, cursor, or modes change (reorder, remove, play-next,
shuffle/repeat toggle, wrap re-shuffle, unavailable skip).

**Evidence**: `ConnectState` (queue, shuffle, repeat, next/prev tracks) is
private to `SpircTask`; there is no read API, so a queue view (FR-012) cannot
be built from Spirc. `Spirc::prev` implements its own "<3 s → previous track"
rule without a shuffle-aware history; `Spirc::repeat_track` is cleared by a
skip. Both differ from FR-004/FR-014, so host ownership is the only way to
meet the spec's rules and SC-011's proptests.

**Consequences (recorded deviations, all bounded)**:
1. A program reload while the current track is unchanged makes `Player` seek
   the running decoder to `seek_to`; the source passes its best position
   estimate (last `Playing/PositionChanged` event + elapsed, refreshed every
   250 ms via `position_update_interval`), so the discontinuity is ≤ the
   estimate error (target < 50 ms) and only occurs on queue edits, never on
   natural transitions.
2. Outbound shuffle state (FR-025) is reported as **off** to other
   controllers because Spirc's shuffle flag must stay off for the host's
   order to be honoured; repeat-all and repeat-one are reported correctly.
   A remote "shuffle on/off" command is honoured as a host shuffle toggle
   (mirrored from `ShuffleChanged`) and the host immediately reloads its own
   order, so the remote flag reverts to off.
3. Remote `next`/`prev` are executed by Spirc within the host's order
   (`prev` = previous item in effective order at <3 s, restart otherwise).
   Local skip-back uses the host's history (FR-004); the remote variant uses
   list order — identical except after reorders/removals.
4. `Player::load` on the same track drops the pending preload
   (`self.preload = PlayerPreload::None`); the next track is re-preloaded at
   the usual 30 s-before-end point.

**Alternatives rejected**: mirroring Spirc's queue (no read API); giving Spirc
one track at a time (Spirc stops at end-of-list → audible gap on every
transition, no gapless).

## R3. Transfer-in context, other device's name, launch-time Connect state — Web API `/v1/me/player`

**Decision**: Spirc does not expose the transferred context's upcoming items
nor the cluster's other devices. Therefore: (a) on transfer-to, the local
queue becomes `[current]` (spec-sanctioned single-item case, FR-015) in
**source-driven** mode: as Spirc reveals further tracks (`TrackChanged` with
a uri not in the program), the host appends them as context items and moves
the cursor; local skip forward/back delegate to `spirc.next()/prev()` while
source-driven; any local queue edit or "Play from account" switches back to
**host-driven** (program) mode. (b) The other device's display name
(FR-016 banner) and the launch-time active device (FR-019) come from the
account read integration: `AuthorizationService::fetch_playback_state`
(`GET /v1/me/player`, scope `user-read-playback-state`), called once on
`BecameInactive` and once at launch/reconnect. When the call fails or the
scope was not granted, the banner reads "Playing on another device" and
FR-019 falls back to "not active" (a subsequent cluster update corrects it).

**Rationale**: keeps the source crate the only protocol speaker while using
the public Web API (already used by 002 for `/v1/me`) for read-only
metadata. **Alternatives rejected**: forking `librespot-connect` to expose
cluster/context state — maintenance burden contrary to A-11's goal.

## R4. Real-time half — `rtrb` ring, markers, anchored position, underrun flag

**Decision**: `ConnectRtSource` implements 001's `AudioSource` unchanged:
- `sample_rate() = 44_100` (Spotify Vorbis is 44.1 kHz stereo at every tier;
  the 001 `OutputStage` resamples to the device rate).
- `fill` pops interleaved `f32` frames from an `rtrb::Consumer<f32>`; if fewer
  than requested are available it writes silence for the remainder, sets the
  `underrun` atomic flag and does **not** advance the reported position for
  the silent frames (FR-006/FR-010).
- Position = `anchor_frames + frames_consumed_since_anchor`. Anchors are
  **markers** (`Marker { at_written_frame, track_seq, position_frames, kind }`)
  pushed by the streaming half through a second SPSC queue when
  `Playing/Seeked/PositionCorrection/TrackChanged/EndOfTrack` fire, stamped
  with the ring's total-written-frames counter at that instant; the RT half
  applies a marker once its total-consumed counter passes `at_written_frame`.
  Boundary accuracy is ± one decoded packet (≤ 50 ms); relative accuracy is
  sample-exact.
- `len_frames()` = current track duration in frames from an atomic set by the
  streaming half (`None` when no track); `seek(frame)` (called by the engine
  on Stop/Seek) resets the anchor and discards readable ring content
  (`read_chunk(slots).commit_all()`, allocation-free); the streaming half
  performs the real seek (`Player::seek`).
- Ring capacity 8 192 frames (≈ 186 ms): small enough that stale audio after
  a seek/skip is inaudible-short and that seeks land within FR-005's 50 ms
  once the flush happens, large enough to ride out scheduler jitter.
  `RingSink::write` blocks the Player thread (which is what every librespot
  sink does against a device) with 500 µs parks while the ring is full.
- `SourceRtShared` atomics (`underrun`, `current_track_seq`, `track_len_frames`,
  `ring_fill_frames`) are read by the controller in `tick()`.

**Rationale**: Constitution I — no lock, allocation or I/O in `fill`/`seek`;
position derived from frames actually consumed by the audio clock.

**Alternatives rejected**: a multi-second decoded ring (stale audio on
seek/skip, and buffering state would lag); using `PlayerEvent::PositionChanged`
as the position source (UI-timer-like, violates FR-006).

## R5. Readiness, buffering and the "≥ 2 s" threshold

**Decision**: the readiness threshold is implemented in two layers:
`AudioFetchParams::read_ahead_before_playback = 2 s` (librespot blocks the
start of playback until ≥ 2 s of *encoded* data at nominal bitrate is
present — `Loading` → `Playing` events delimit this) and the host's
`buffering` sub-state = transport intent `Playing` ∧ (source reported
`Loading` since the last `Playing` ∨ RT `underrun` flag observed and the ring
has not since refilled to ≥ 50 %). On underrun librespot's `Player` blocks on
the fetch, the ring drains, the RT half writes silence and freezes position;
when data returns `Player` emits `Playing{position_ms}` "after a
buffer-underrun" — the host clears `buffering` and re-anchors. The track is
never skipped (FR-010). `read_ahead_during_playback = 1 h` makes the fetcher
request the whole file ahead of the read position — the "as fast as the
connection allows" pre-buffer (FR-009); `AudioFetchParams::set` is a
process-global `OnceLock`, set once at source construction.

**Evidence**: `AudioFetchParams { read_ahead_before_playback (default 1 s),
read_ahead_during_playback (default 5 s), minimum_download_size 64 KiB, .. }`
with doc "this many seconds of data ahead of the current read position are
requested".

## R6. Pre-fetch of the next item, and what is held where

**Decision**: the next item is `Player::preload`ed when librespot raises
`TimeToPreloadNextTrack` (30 s before the end of the current track,
`PRELOAD_NEXT_TRACK_BEFORE_END_DURATION_MS = 30000`) — Spirc does this
automatically for the next entry of the program; the source additionally
calls `player.preload(next)` immediately when a new program arrives while the
current track is inside that window. Only the current and the preloaded next
track exist (`PlayerPreload` holds exactly one). **Deviation from FR-009's
"once the current track is complete, fetch the next"**: librespot exposes no
fetch-complete signal; the prefetch therefore starts at the 30 s mark rather
than the moment the current fetch completes. The spec's observable outcomes
(gapless transition when both are buffered, ≤ 2 tracks held, stale prefetch
cancelled — a program change replaces `PlayerPreload`) hold.

**Storage deviation (recorded in plan.md Complexity Tracking)**:
`librespot-audio` streams the **encrypted** file into a `NamedTempFile` in
`SessionConfig.tmp_dir` and deletes it on drop. FR-009 says "in memory only;
nothing written to disk". Constitution V (no *decoded* audio on disk, no
decryption except for playback) is fully respected: the file holds the
AES-CTR-encrypted stream only. Mitigation: `tmp_dir` = a ModPlayer-private
subdirectory of the OS temp dir (`modplayer-stream-<pid>`), created with
owner-only permissions where the OS supports it, purged at source start and
exit. An in-memory backing store would require a fork of `librespot-audio`
(`unknown-git = "deny"` in `deny.toml`) and is deferred; recorded as a
follow-up.

## R7. Failure classification — transient vs. unavailable

**Decision** (FR-020/021): the source classifies every librespot
`Error` by `ErrorKind`:
- **Transient** (retry with backoff 1 s → 60 s cap, 002's `RefreshScheduler`
  convention): `Unavailable`, `DeadlineExceeded`, `ResourceExhausted`,
  `Cancelled`, `Aborted` (dealer/session dropped), `Unauthenticated` (token
  rejected — wait for 002's refresh, re-read the token on every retry), plus
  any I/O/TLS error while a session exists. Reported as
  `SourceHealth::Transient { since, next_retry_in }`.
- **Unavailable** (no retry; `Retry` action re-initialises):
  `Unimplemented`, `FailedPrecondition`, `InvalidArgument`, `DataLoss`,
  `Internal` during handshake/registration, and protobuf parse failures.
  `client_update_required = true` when the AP rejects the client/protocol
  version (`ErrorKind::FailedPrecondition` or `Unimplemented` at login).
- **Tier**: `PermissionDenied` at login (`PremiumAccountRequired`) →
  `SourceEvent::TierRejected` (FR-027 downgrade path; not a health state).
Transient never escalates by duration (SC-009); the 30 s warning is a host
timer over `Transient.since`.

**Session recovery**: on session loss the existing `Player` keeps rendering
what it already fetched; a new `Session` is built in the background with
backoff; on success a new `Player` + `Spirc` are created and the host's
last program is re-sent at the current position (an audible gap only if the
buffer ran out first, bounded by `buffering`).

## R8. Credentials — OAuth access token each time; reusable blob deferred

**Decision**: the source logs in with
`Credentials::with_access_token(token)` where `token` comes from a
`ReceiverCredentials` trait object (`fn access_token(&self) -> Result<String, CredentialError>`)
implemented in the `modplayer` binary over `SecureStore` +
`SessionCredential::from_payload`. The token is re-read on every (re)connect,
so 002's refresh keeps it valid. The reusable AP credential blob
(`EntryName::ReceiverCredential`, reserved in 002) is **not** stored in this
slice (one secret fewer; login latency with a fresh token measured in the
002 spike is < 1 s). The `SessionConfig.client_id` stays librespot's
Keymaster id (002 R1 caveat 1); `device_id` is a per-install random id
persisted in `settings.toml` (`[playback] connect_device_id`) so other
controllers see one stable device across launches.

**Alternatives rejected**: storing the blob now — adds a secure-store write
path and a registry entry for no user-visible gain in this slice.

**⚠️ Amendment (2026-09-16, post-implementation, T098 manual run)**: the
premise that **one** browser OAuth token serves both the receiver session
(this R8) and the Web API reads (R9) is **false** as of 2026. Verified on a
real Premium account by driving M1 end-to-end:

| Token minted by | Web API (`/v1/me`, `/me/tracks`, recently-played) | librespot `Spirc::new` session login |
|---|---|---|
| Keymaster (`65b7080…`, 002 R1 default) | **HTTP 429** on every call, ignoring `Retry-After`, independent of volume | **works — verified**: session auth succeeds, device activates, real audio plays (T044, see below) |
| Own Developer-Dashboard app (via `MODPLAYER_OAUTH_CLIENT_ID`) | **works** — fetched real recently-played tracks | **`FaultyRequest(INVALID_CREDENTIALS)`** → `ErrorKind::FailedPrecondition` |

Spotify now (a) rejects Web API requests bearing tokens minted by the
Keymaster desktop client id with a blanket 429 (confirmed independently:
go-librespot #282, librespot-python #328 — both label it Spotify-side and
removed/replaced their Web API passthrough), and (b) rejects a
Dashboard-app-minted token at the receiver session's AP login, because the
session authenticates as Keymaster (`SessionConfig.client_id`) while the
token was issued to a different client. **No single token satisfies both.**

The Keymaster→session cell is now **verified** (2026-09-16): the live test
T044 was run with a Keymaster-minted `streaming` token (obtained by signing
into ModPlayer with the *default* client id and reading the stored
credential). The session authenticated with no `INVALID_CREDENTIALS`, the
device activated (`BecameActive`), the requested track loaded, and ~4.9 s of
non-silent audio (216 404 frames) flowed through the RT ring. Two follow-on
observations from the same run:
- Reaching this required a **third fix** (below): a host-initiated
  `LoadProgram` did not activate the Connect device, so librespot silently
  dropped the `Load` ("will be ignored while Not Active").
- Rapidly re-running T044 against the same track eventually stalls at
  `Loading` (no `Playing`) — Spotify audio-key throttling after many rapid
  loads (cf. librespot #1649), a test-hygiene caveat, not a code fault; the
  first clean runs played audio.

**Consequence**: real streaming (US1's core promise, SC-001/002) cannot pass
end-to-end under the single-credential design. The fix is a **two-credential
architecture** — a Keymaster/session credential for the Connect receiver and
a separate Dashboard-app token for Web API reads — which spans 002 (OAuth,
SecureStore) and 003 (worker credential source, `AuthorizationService`
transport). This is out of 003's current scope and is recorded in plan.md
"Post-Implementation Findings (T098)". Two bugs found in the same run and
fixed in place: (1) the connect worker's `Session::new` ran outside its tokio
runtime (panic "there is no reactor running", latent since T038 as no
non-`#[ignore]` test exercises the real `ConnectSource`); (2) worker
thread-spawn / tier-check failures were swallowed silently; and (3) a
host-initiated `LoadProgram` never called `spirc.activate()`, so librespot
ignored the `Load` on the still-inactive device and playback never started —
fixed by activating when a `device_active` flag shows the device is inactive
(guarded so it never arms `transfer_requested` for an already-active device
and corrupts a later US3 remote transfer).

**Resolution (2026-09-16, supersedes the "Consequence" above)**: the
two-credential architecture was not built. See spec.md's **Amendment
(2026-09-16): Session-sourced reads, not the public Web API** and plan.md's
"Post-Implementation Findings (T098) → Resolution" — the shipped design stays
single-credential and sources tier, the other device's name, and "Play from
account" from the receiver session (`SpClient`, Connect cluster state)
instead of the public Web API. T098 M1 passed live on this design.

## R9. "Play from account" and the other device name — extend `AuthorizationService`

**⚠️ Superseded (2026-09-16)**: this section describes the approach tried and
found infeasible (see R8's amendment above and spec.md's Amendment) — the
public Web API returns 429 for every read under the single Keymaster
credential this slice uses. It is kept for the record of what was attempted;
the shipped "Play from account" and other-device-name sourcing use the
session's `SpClient` / Connect cluster state, not these `AuthorizationService`
Web API methods.

**Decision**: `modplayer-account`'s `AuthorizationService` gains three
read-only methods, implemented by `SpotifyAuthorizationService` (ureq) and
scripted by `FakeAuthorizationService`:
`fetch_recently_played(token, limit) -> Vec<TrackRef>` (`GET /v1/me/player/recently-played`),
`fetch_saved_tracks(token, limit) -> Vec<TrackRef>` (`GET /v1/me/tracks`), and
`fetch_playback_state(token) -> Option<PlaybackStateSummary>` (`GET /v1/me/player`).
`ClientConfig.scopes` gains `user-read-recently-played`, `user-library-read`,
`user-read-playback-state`. Sessions authorised before this change lack the
scopes: a 403 is surfaced as `AuthError::Forbidden`, "Play from account"
shows an inline "Sign in again to grant access to your library" hint, and
the device-name lookups fall back silently.

**Rationale**: reuses 002's HTTP transport, fake and credential handling;
keeps Web API knowledge in the INT-1/INT-3 crate, not in the receiver crate.

## R10. Extending the `AudioSource` seam additively — the `SourceHost` trait

**Decision**: `modplayer-audio-source` keeps `AudioSource` byte-for-byte and
adds `SourceHost` (off-real-time handle), `SourceCommand`, `SourceEvent`,
`SourceHealth`, `BufferStatus`, `TrackRef`, `TrackId`
(contracts/audio-source-host.md). Three implementors: `SyntheticHost`
(001's synthetic source, no-op commands), `ScriptedHost` (test double with
throttling, unavailable tracks, remote/transfer scripts, health scripts —
public in `modplayer-audio-source-synthetic` for cross-crate tests, as 002
did with `FakeAuthorizationService`), and `ConnectSource`. The
`PlaybackController` becomes generic over `H: SourceHost` and calls
`host.attach(position)` to obtain a fresh RT half for every stream
(re)build, so the engine stays generic (`Processor<H::Rt>`) with no trait
object on the real-time path (001 decision preserved).

**Alternatives rejected**: `Box<dyn AudioSource>` in the processor (reverses
001's documented decision for no gain); a second abstraction (forbidden by
the spec's clarification and Constitution X).

## R11. Engine deltas — `Command::Seek`, leftover carry, `PositionClock`

**Decision** (engine crate, maintainer sign-off + real-time note required):
1. `Command::Seek(u64)` (16 bytes, `Copy`) → `source.seek(frame)` at the next
   buffer boundary; needed by the synthetic source and by the RT flush of the
   Connect source.
2. The resampler guard-frame **rewind by `seek`** is replaced by a
   preallocated leftover carry (`OutputStage` keeps unconsumed source frames
   and prepends them on the next `render`); the published position is
   `source.position() − carried`. A ring-buffer source cannot un-consume
   frames, and this removes a `seek` from the hot path for every source.
3. `RtShared` gains a `position_anchor` (position frames + `Instant` nanos +
   generation, written once per render) and `PositionClock::now(&RtShared) ->
   Duration` extrapolates between renders at the source rate while
   `Playing`, returning the frozen anchor otherwise. The UI samples it at
   ≥ 60 Hz (`request_repaint_after(16 ms)` while playing); a test samples it
   at 60 Hz for 10 s against `FakeBackend` and asserts monotonicity and
   ≤ 5 ms jitter (SC-003). Rationale: at the Safe preset (1 024 frames) the
   render rate is ≈ 43 Hz, below FR-006's 60 Hz, so per-render publication
   alone cannot satisfy the requirement; `Instant::now()` is a non-blocking
   vDSO/`mach_absolute_time` read and is real-time acceptable.
4. Engine `Transport` stays `Stopped|Playing|Paused`; `buffering` is a host
   sub-state (`TransportState` in core) derived per R5.

## R12. Keeping the host ticking while the window is hidden

**Decision**: audio advancement never depends on UI ticks — the source
(Spirc/Player) advances and preloads within the program on its own (R2).
Host bookkeeping (cursor mirror, wrap re-shuffle program, notifications)
runs in `PlaybackController::tick()` from the egui frame. To keep frames
flowing while minimized/hidden/on another desktop, `App` starts a
`Ticker` thread that calls `egui::Context::request_repaint()` every 33 ms
whenever the transport intent is `Playing` (winit's event-loop proxy wakes
the loop for occluded windows on all three platforms). Manual quickstart
scenario M4 verifies 10 minutes minimized (SC-010).

## R13. Settings, strings, URLs

- `settings.toml` (device-scoped) gains `[playback] device_name`
  (optional, absent = default), `connect_device_id` (generated once).
  Same atomic-replace store; contracts/settings-file.md of 001 extended in
  contracts/transport-and-queue.md §5.
- `STATUS_PAGE_URL` is a build-time constant in `modplayer-core::links`
  overridable via `MODPLAYER_STATUS_PAGE_URL` at compile time
  (`option_env!`), default `https://github.com/rzcastilho/modplayer/issues`
  until a status page exists (spec assumption). `UPGRADE_URL` is reused from
  002 (`modplayer-account::UPGRADE_URL`).
- New Fluent file `locales/en-US/playback.ftl` (transport, queue, banner,
  notifications, device name); en-US only.

## R14. Test strategy

- `modplayer-core`: proptest over queue operation sequences
  (SC-011; `proptest` moves to core's `[dev-dependencies]`), transport
  reducer table tests, controller tests with `FakeBackend` + `ScriptedHost`
  for every FR-014 branch, buffering, unavailable skip, transfer-in/away,
  transfer timeout (5 s with `FakeClock`-style injected `Instant` source),
  30 s transient warning, sign-out/revocation clearing, downgrade.
- `modplayer-engine`: `Command::Seek` boundary test, leftover-carry position
  test, no-alloc `render` under `assert_no_alloc` unchanged, position-clock
  jitter test.
- `modplayer-audio-source-connect`: pure-function unit tests (health
  classification table, marker arithmetic, `RingSink` back-pressure with a
  fake consumer, program/index mapping, volume scale mapping); a live
  `#[ignore = "manual"]` test that logs in and plays 5 s of a track.
- `modplayer-ui`: fluent-key coverage test extended to `playback.ftl`,
  accessible-name test for every new control (FR-023), credential-leak test
  unchanged but now scanning the new crate's `Debug` outputs.
- CI matrix unchanged (fmt, clippy `-D warnings`, test, deny, SPDX headers).

## Dependency additions summary (Constitution X justification)

| Crate | Version | Licence | Why std / existing deps are insufficient |
|---|---|---|---|
| librespot-core | 0.8 | MIT | AP session, dealer, spclient — the Connect protocol (only in the receiver crate) |
| librespot-connect | 0.8 | MIT | Spirc: registration, remote commands, transfer, state reports |
| librespot-playback | 0.8 (no default features) | MIT | Fetch/decrypt/decode pipeline and gapless preload; custom `Sink`/`Mixer` traits |
| librespot-audio | 0.8 | MIT | `AudioFetchParams` (read-ahead configuration) |
| tokio | 1 (rt-multi-thread, sync) | MIT | Required runtime for `Spirc`; confined to the receiver crate's worker thread |
| tempfile (transitive) | 3 | MIT/Apache-2.0 | librespot-audio's encrypted temp file (R6) |
| symphonia (transitive) | 0.5 | MPL-2.0 | Vorbis decoder used by librespot-playback; MPL-2.0 already allowed in `deny.toml` |
| protobuf, hyper, tokio-tungstenite, rustls (transitive) | — | MIT/Apache-2.0/ISC | librespot wire stack; licences already allowed |
| proptest (dev, now also in core) | 1 | MIT/Apache-2.0 | SC-011 queue proptests |

`cargo deny check` runs as part of the first task of the receiver crate; any
newly surfaced licence is added to `deny.toml` with a crate comment, as 002
did. No new `unsafe`: every new crate is `#![forbid(unsafe_code)]`.

## Open verifications (carried into tasks as spike steps)

1. `Spirc::transfer(None)` vs `activate()` semantics when another device is
   active (FR-018 "request transfer here") — verify against a second
   controller; fallback is `activate()` followed by a program reload.
2. Whether `PlayerEvent::TrackChanged` precedes or follows `Playing` for a
   preloaded gapless transition (affects marker ordering only; both orders
   are handled).
3. Whether Keymaster-minted tokens are accepted by
   `/v1/me/player/recently-played` (R9 fallback covers a 403).
4. Behaviour of `Spirc::shutdown()` on device rename while playing (R2
   consequence: the host re-sends the program at the current position after
   re-registration; brief pause expected, ≤ 5 s to re-register).
