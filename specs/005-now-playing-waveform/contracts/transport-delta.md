# Contract: Transport delta (extends 003 contracts/transport-and-queue.md)

**Crate**: `crates/modplayer-core` (`transport.rs`, `controller.rs`).
Implements FR-009, FR-010; research R6, R8; data-model.md §4.

## 1. Sample-accurate seek

| Item | Before (003) | After (005) |
|---|---|---|
| `Input::Seek` | `{ position_ms: u32, buffer_ready: bool }` | `{ position_ms: u32, position_frames: Option<u64>, buffer_ready: bool }` |
| `Effect::SeekTo` | `{ position_ms }` | `{ position_ms, position_frames: Option<u64> }` |
| `PlaybackController::seek(Duration)` | → `Input::Seek { ms, .. }` | unchanged signature; `position_frames: None` |
| `PlaybackController::seek_frames(u64)` | — | new; `position_ms = frames × 1000 / rate`, `position_frames = Some(frame)` |
| controller on `SeekTo` | `Command::Seek(ms_to_frames(ms))` + `SourceCommand::Seek(ms)` | `Command::Seek(position_frames.unwrap_or_else(ms_to_frames))` + `SourceCommand::Seek(ms)` |

Reducer rules T6–T8 are unchanged; the end-of-track clamp applies to
both representations (`position_frames` clamped to
`track_len_ms × rate / 1000`). Seeking while `Paused` stays `Paused`;
while `Stopped` with a current track enters `Paused` at the position
(003 US-1 AS13 — already the reducer's behaviour, now also exercised by
the waveform).

Tests: `transport_reducer::seek_frames_carries_exact_frame_to_engine`,
`::seek_frames_clamps_at_track_end`,
`controller_streaming::seek_frames_pushes_exact_engine_command_and_ms_to_source`.

## 2. Analysis wiring in the controller

| Trigger | Controller action |
|---|---|
| `SourceEvent::TrackStarted { track, .. }` accepted by the reducer (generation check passed) | `analysis.attach(track.id, source_rate, duration_ms → frames)` |
| `SourceEvent::BecameActive { context: Some(ctx) }` | same with `ctx.current` |
| `SourceEvent::DecodedStore { track, store }` | `analysis.attach_store(track, store)` (ignored if `track` is not the current attachment) |
| current track changes (any path: skip, end of track, queue replace, unavailable skip, transfer-out) | `analysis.detach()` before the new `attach` |
| `stop()` | keeps the attachment (the track is still current; FR-017 "playing or paused" — stopped-with-current-track keeps its waveform) |
| `clear_for_sign_out()` | `analysis.detach()` |
| `shutdown()` | `analysis.shutdown()` after the source `Shutdown` |
| `tick()` | `analysis.drain()`; `PlaybackController::analysis()` exposes `latest()` |

`AnalysisService::new(AnalysisPaths::resolve())` in
`PlaybackController::new`; a `None` path (unresolvable data dir) disables
the cache only — analysis still runs in memory.

Tests: `controller_streaming::track_change_detaches_then_attaches_analysis`,
`::decoded_store_event_reaches_analysis_for_current_track_only`,
`::sign_out_detaches_analysis`.

## 3. Unchanged

`Command::Seek(u64)` (engine), `SourceCommand::Seek(u32 ms)`, position
reporting (`PositionClock`), `buffering` derivation, queue rules. No
engine-crate change in this slice.
