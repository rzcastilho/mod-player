# Contract: `SourceHost` — the additive extension of the `AudioSource` seam

**Crate**: `modplayer-audio-source` (trait crate, still dependency-free) |
**Implementors**: `SyntheticHost` and `ScriptedHost` (`modplayer-audio-source-synthetic`),
`ConnectSource` (`modplayer-audio-source-connect`) | **Consumer**: `modplayer-core::PlaybackController<B, H: SourceHost>`

001's `AudioSource` trait is **unchanged** (spec clarification: additive
only; every 001 test passes as-is). This file adds the off-real-time side of
the seam. Constitution IV: the host never names a concrete source; the
`modplayer` binary chooses the implementor.

## 1. Traits

```rust
/// Off-real-time handle to an audio source. Owned by the PlaybackController
/// on the UI thread; never touched from the audio callback.
pub trait SourceHost: Send + 'static {
    /// The real-time half handed to a `Processor`. One fresh value per
    /// stream (re)build; `position_frames` restores the read position.
    type Rt: AudioSource;

    /// Build (or rebuild) the real-time half. Called on every stream open;
    /// the previous `Rt` is dropped with the previous stream. Must not block
    /// on network; may allocate (off the real-time path).
    fn attach(&mut self, position_frames: u64) -> Self::Rt;

    /// Non-blocking. Commands are applied asynchronously; their effects
    /// arrive as events.
    fn command(&mut self, cmd: SourceCommand);

    /// Drain every event raised since the last call (FIFO). Non-blocking.
    fn poll(&mut self) -> Vec<SourceEvent>;

    /// Current buffer status (read from shared atomics; cheap).
    fn buffer_status(&self) -> BufferStatus;

    /// Current health (mirrors the last `SourceEvent::Health`).
    fn health(&self) -> SourceHealth;

    /// Shared real-time atomics for the position/underrun readouts.
    fn rt_shared(&self) -> Arc<SourceRtShared>;
}
```

`SourceRtShared` (in the trait crate so the controller can read it):
`underrun: AtomicBool` (set by `fill` on a short read; cleared by the
controller after observing it), `track_seq: AtomicU32`,
`track_len_frames: AtomicU64` (0 = unknown), `ring_fill_frames: AtomicU32`,
`consumed_frames: AtomicU64`. The synthetic host reports `underrun = false`,
`ring_fill_frames = u32::MAX` (always ready).

## 2. `SourceCommand` (host → source)

| Command | Semantics | Synthetic/Scripted behaviour |
|---|---|---|
| `Initialize { device_name, device_id }` | Register with the service; emits `Registered` or `Health(..)` | emits `Registered` immediately |
| `Deregister` | Stop output, deregister; emits `Deregistered` | emits `Deregistered` |
| `Shutdown` | `Deregister` + release all resources (app exit); synchronous best effort ≤ 2 s | no-op |
| `Retry` | Tear down and re-run `Initialize` after `Unavailable` | scripted |
| `SetDeviceName(DeviceName)` | Re-register under the new name within 5 s; playback state preserved (may pause/resume) | emits `Registered` |
| `LoadProgram(Program)` | Replace the play order (data-model.md §2.3). If `order[cursor_index]` is the currently loaded track, only `position_ms`/`start_playing` are applied (seek), otherwise the track at the cursor is loaded. Subsequent natural advancement follows `order`; wrap iff `repeat_all`; `repeat_one` restarts the current item on natural end only | synthetic: plays its fixed track regardless; scripted: plays fixture audio per program |
| `Play` / `Pause` / `Stop` | Transport intent; `Stop` = pause + seek 0 + `Stopped` event | mirrored as events |
| `Seek(ms)` | Seek within the current track; emits `Seeked` (and `Loading` first when the target is not buffered) | scripted throttle applies |
| `SkipNext` / `SkipPrev` | Only meaningful in source-driven mode (data-model.md §2.2 `QueueMode`); the source advances within its own context | scripted |
| `SetVolume(VolumePercent)` | Outbound volume report; no audio effect in the source | no-op |
| `RequestTransferHere` | Ask the service to make this device active (FR-018); result arrives as `BecameActive { context }` or nothing (host applies the 5 s timeout) | scripted |
| `ReportState { intent, position_ms, repeat }` | Outbound state report hint after host-only changes (FR-025); the Connect source coalesces reports (≤ 1 per 200 ms) | no-op |

## 3. `SourceEvent` (source → host)

| Event | Payload | Meaning |
|---|---|---|
| `Registered { device_name }` | | Device visible to controllers |
| `Deregistered` | | |
| `Health(SourceHealth)` | data-model.md §1.3 | Every change; also re-emitted on `Retry` |
| `TrackStarted { track, program: Option<(u64 /*generation*/, u32 /*index*/)>, position_ms, playing }` | `TrackRef` | A track is now the loaded/current one. `program = None` = the source chose it (source-driven or remote queue addition) |
| `Loading { position_ms }` | | Buffering started for the current track (start, seek, underrun) |
| `Playing { position_ms }` | | Audio flowing from `position_ms`; clears buffering; re-anchors position |
| `Paused { position_ms }` / `Stopped` | | |
| `Seeked { position_ms }` | | Seek applied |
| `EndOfTrack` | | Natural end of the current item; the source will itself continue per the program (host only mirrors) |
| `Unavailable { track }` | `TrackId` | The service refused the track; the source skips it per the program |
| `RemoteCommand(RemoteCommand)` | `Play \| Pause \| SkipNext \| SkipPrev \| Seek(ms) \| Volume(VolumePercent) \| Shuffle(bool) \| Repeat(Repeat)` | Already applied by the source where it can (play/pause/seek/skip); the host mirrors state and, for `Shuffle`/`Repeat`, applies its own mode and reloads the program |
| `BecameActive { context: Option<TransferContext> }` | data-model.md §6 | |
| `BecameInactive` | | |
| `TierRejected` | | Login refused for non-Premium (FR-027) |

Ordering: events are delivered in the order the source observed them;
`TrackStarted` always precedes the first `Loading/Playing` of that track.
Events carrying `program` with a generation older than the host's current
one are ignored by the host.

## 4. Real-time half contract additions (still `AudioSource`)

For a streaming implementor:

| Method | Rule |
|---|---|
| `fill(out)` | Pops up to `out.len()/2` frames from the ring; on a short read writes silence for the rest, sets `underrun`, and advances `position()` only by the frames actually delivered (position freezes while buffering, FR-006) |
| `position()` | `anchor + consumed_since_anchor` (markers, research R4) |
| `len_frames()` | `Some(track_len)` when a track is loaded, else `None`; **no wrapping** at `len` (the source moves to the next track via markers) |
| `seek(frame)` | Sets the anchor to `frame`, discards the readable ring content without allocating, bumps a discard generation; never blocks. The host also sends `SourceCommand::Seek/Stop` so the streaming half performs the real seek |
| Allocation | `attach` preallocates everything; `fill`/`seek` are verified with `assert_no_alloc` in the connect crate's tests |

## 5. Guarantees pinned by tests

- `modplayer-audio-source-synthetic`: `SyntheticHost::attach` returns a
  `SyntheticSource` at the requested position; `poll()` after `Initialize`
  yields exactly `[Registered]`; all 001 synthetic tests unchanged.
- `ScriptedHost`: a scripted sequence (`throttle(ms)`, `unavailable(id)`,
  `remote(cmd)`, `transfer_in(ctx)`, `transfer_out()`, `health(h)`) produces
  the documented events in order; its `Rt` produces deterministic fixture
  audio (a per-track DC offset + sine, so tests can assert which track is
  audible and detect gaps/clicks at transitions).
- `modplayer-audio-source-connect` (unit, no network): `ConnectRtSource`
  under `assert_no_alloc` for `fill`/`seek`; marker application at exact
  `at_written_frame` boundaries; underrun freezes `position()`; `seek`
  discards exactly the readable frames; `RingSink` back-pressure never
  loses samples.
