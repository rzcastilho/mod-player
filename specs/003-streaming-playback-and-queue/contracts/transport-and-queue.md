# Contract: Transport state machine, Queue, and `PlaybackController` additions

**Crate**: `modplayer-core` (`transport.rs`, `queue.rs`, `controller.rs`,
`settings/model.rs`, `links.rs`) | **Consumers**: `modplayer-ui`, tests.

## 1. `PlaybackController<B: OutputBackend, H: SourceHost>` — new public surface

Existing 001/002 methods keep their signatures. `open_stream_on` now builds
the processor from `self.source_host.attach(position)` instead of
`SyntheticSource::default()`.

| Method | Behaviour |
|---|---|
| `new(backend, source_host, settings_store)` | as before plus the host; `Initialize` is **not** sent here |
| `set_playback_permitted(permitted: bool, reason: Option<NotRegisteredReason>)` | called by `App` from 002 state: `true` → `Initialize {device_name, device_id}` once; `false` → `Deregister`, transport disabled with the inline reason |
| `transport_state() -> TransportState` | `Stopped/Playing/Paused/Buffering` (data-model.md §3.1) |
| `active_state() -> &ActiveState` | |
| `transport_enabled() -> bool` | `active_device().is_some() && !matches!(health, Unavailable) && registered_or_inactive` |
| `disabled_reason() -> Option<&'static str>` | Fluent key for the inline reason (no device / Premium required / not verified / source unavailable) |
| `position() -> Duration` | `PositionClock::now(shared)` (engine contract §3) |
| `current_track() -> Option<&QueueItem>` | |
| `queue() -> &Queue` / `queue_view() -> QueueView` | `QueueView { items: Vec<QueueRow { uid, title, artist, origin, unavailable, is_current }> , shuffle: bool, repeat: Repeat }` in effective order, current first, history excluded |
| `play()` / `pause()` / `stop()` / `seek(Duration)` / `skip_forward()` / `skip_back()` | per §2; when `Inactive`, play/skip/seek become a transfer request (FR-018) |
| `set_master_volume(v)` | as 001 + `SourceCommand::SetVolume(v)` when registered |
| `queue_replace(tracks)` / `queue_play_next(uid)` / `queue_play_next_track(TrackRef)` / `queue_move_up(uid)` / `queue_move_down(uid)` / `queue_reorder(uid, index)` / `queue_remove(uid)` / `set_shuffle(bool)` / `set_repeat(Repeat)` | apply to `Queue`, then `sync_program()` (§3) |
| `play_here()` | `RequestTransferHere` with pending `PlayHere` |
| `retry_source()` | `SourceCommand::Retry` |
| `set_device_name(input: &str) -> Result<(), DeviceNameError>` | validates, persists `[playback] device_name`, `SetDeviceName` when registered |
| `device_name() -> String` | effective name (custom or default) |
| `clear_for_sign_out()` | stop; queue cleared; `Deregister`; `active = NotRegistered{SignedOut}`; synchronous |
| `on_tier_rejected()` / `on_tier_free()` | FR-027 downgrade path |
| `tick()` | 001's flush + backend events **+** `poll()` the source and reduce every event (§2), read `SourceRtShared` for underrun/ring fill, run timers (transfer 5 s, transient 30 s, prefetch recompute) |
| `shutdown()` | called from `App::on_exit`: `stop()`, `SourceCommand::Shutdown`, drop stream |

## 2. Transport reducer (`transport::reduce(state, input) -> (state, Vec<Effect>)`)

Pure function; `Effect ∈ { Engine(Command), Source(SourceCommand), Notify(key, severity, action), DismissNotify(key), Queue(QueueOp), Timer(Start/Cancel) }`.
Inputs are user commands, source events, timer expiries and RT readouts.

Rules (spec clarifications made executable):

| # | Rule |
|---|---|
| T1 | `play` in `Stopped` with a current item → intent `Playing`; if the item is not loaded in the source, `LoadProgram{start_playing:true, position_ms:0}`; else `Source(Play)`; `Engine(Play)`; `buffering = !buffer_status.ready` |
| T2 | `play` in `Paused` → intent `Playing`, `Source(Play)`, `Engine(Play)`; no re-buffer when `ready` |
| T3 | `play` while `Buffering` → no-op |
| T4 | `pause` in `Playing/Buffering` → `Paused`, `Source(Pause)`, `Engine(Pause)`; position retained |
| T5 | `stop` → `Stopped`, position 0, `Engine(Stop)`, `Source(Stop)`; current item, queue and active status retained |
| T6 | `seek(p)` in `Stopped` → `Paused` at `p`; `Engine(Seek)`, `Source(Seek)` |
| T7 | `seek(p)` with `p ≥ len` → clamp to `len` and `Queue(Advance(SeekPastEnd))` (repeat-one restarts the same item) |
| T8 | `seek(p)` otherwise → `Engine(Seek(frames))`, `Source(Seek(ms))`; `buffering = !ready` until `Playing` arrives |
| T9 | `skip_forward` → `Queue(Advance(Skip))` (ignores repeat-one); result `MoveTo(item)` → `LoadProgram{cursor=item, position 0, start_playing = intent==Playing}`; `EndOfQueue` → `Stopped` on the last item at 0 |
| T10 | `skip_back` → `Queue(SkipBack(position))`: `RestartCurrent` → `Seek(0)`; `MoveTo(uid)` → program at that item |
| T11 | `EndOfTrack` event (host-driven) → `Queue(Advance(TrackEnd))` for the *mirror* only — the source already continues; the host verifies the next `TrackStarted.program` index equals its expectation, otherwise it re-sends the program (self-healing) |
| T12 | `TrackStarted{program: Some((gen, idx))}` with `gen == current` → cursor := idx, push history when `playing`, `track_len := duration`; `program: None` → `Queue(Reveal(track))` and cursor := revealed |
| T13 | `Loading` → `buffering = true` (if intent `Playing`); `Playing` → `buffering = false`, dismiss nothing; underrun flag observed → `buffering = true` until `ring_fill ≥ 50 %` or `Playing` |
| T14 | `Unavailable{track}` → `Queue(MarkUnavailable)`, `Notify(queue-item-skipped-unavailable, Info)`; if no playable candidate remains → `Stopped` |
| T15 | `RemoteCommand(Volume(v))` → `set_master_volume(v)` without echoing `SetVolume` back; `Shuffle(b)`/`Repeat(r)` → `set_shuffle`/`set_repeat` (program re-sent) |
| T16 | `BecameInactive` → `active = Inactive{other: None}`, `Engine(Pause)`, intent frozen as `Paused`, `Source(Deregister)` **not** sent (still registered); request the other device's name (R3) |
| T17 | `play/skip/seek/play_here` while `Inactive` → `active = TransferRequested{since, pending}`, `Source(RequestTransferHere)`, `Timer(Transfer 5 s)`; `pause`/`stop` → no-op; volume → local only |
| T18 | `BecameActive{context: None}` in `TransferRequested` → `Active`, apply `pending`; `BecameActive{context: Some(ctx)}` → queue := `[ctx.current]`, mode `SourceDriven`, modes from ctx when given, discard pending, intent := ctx.playing ? Playing : Paused, `Engine(Play/Pause)`; in any state |
| T19 | `Timer(Transfer)` expiry in `TransferRequested` → `Inactive`, `Notify(transfer-request-failed, Warning)` |
| T20 | `Health(Transient{since})` → inline "Reconnecting…"; `Timer(Reconnect 30 s)`; expiry → `Notify(stream-reconnect-warning)`; `Health(Ok)` → `DismissNotify(stream-reconnect-warning)` |
| T21 | `Health(Unavailable{cur})` → transport disabled; `Notify(stream-source-unavailable or -update-required, Critical, actions)`; queue view/navigation untouched |
| T22 | `TierRejected` / tier `Free` → `downgrade_pending = true`: the current item finishes (`EndOfTrack` → no advance), then `Deregister`, `active = NotRegistered{PremiumRequired}`, `Notify(subscription-downgraded, Warning, OpenUpgradePage)` |
| T23 | `clear_for_sign_out` → `Engine(Stop)`, `Source(Deregister)`, queue cleared, `active = NotRegistered{SignedOut}` |
| T24 | Every input that changes intent, position, volume, repeat or track → `Source(ReportState)` (coalesced by the source) |

Position publication (FR-006) is not a reducer concern: the UI reads
`PositionClock` each frame at ≥ 60 Hz while `Playing`.

## 3. Program synchronisation (`sync_program`)

Called after every queue mutation and mode change. Computes
`Program { order: effective order, cursor_index, position_ms: current position, start_playing: intent == Playing, repeat_all, repeat_one, generation: next }`
and sends `LoadProgram` when `mode == HostDriven` and any of `order`,
`cursor_index`, `repeat_all`, `repeat_one` differ from the last sent program;
a mutation in `SourceDriven` mode switches to `HostDriven` first. Debounced
to at most one send per 250 ms (the last wins) so a drag-reorder does not
spam the source; the ≤ 1 s prefetch-recompute requirement (FR-009) is met
by the send itself.

Wrap under repeat-all + shuffle: when `TrackStarted` lands on the last item
of the effective order, the queue pre-computes the re-shuffle for the next
cycle and `sync_program` sends `[last, ...reshuffled]` so the source preloads
the correct first item of the next cycle (FR-013 "re-shuffled at each wrap").

## 4. `Queue` API (pure, `modplayer-core::queue`)

See data-model.md §2.2 for fields and operations. Additional contract points:

- Randomness is injected: `set_shuffle(true, &mut impl RngCore)`; the
  controller uses a `getrandom`-seeded `SmallRng`-style xorshift implemented
  in-crate (no `rand` dependency); tests pass a fixed seed.
- `QueueChange` returned by every mutating call:
  `{ order_changed: bool, cursor_changed: bool, playback: PlaybackChange }` with
  `PlaybackChange ∈ { None, Restart, MoveTo(uid), EndOfQueue, Empty, Skipped(uid) }`.
- Proptest strategy (SC-011): random sequences of up to 64 operations over
  fixtures of up to 12 tracks (with duplicates and unavailable flags);
  invariants in data-model.md §2.2.

## 5. Settings delta (`settings.toml`, 001 contracts/settings-file.md extended)

```toml
[playback]
device_name = "Studio Mac"    # optional string; absent/empty = default
connect_device_id = "…32 hex…"  # generated on first Initialize when absent
```
`RawPlayback` is an optional table (older files load unchanged). Invalid
`device_name` (> 64 chars after trim) → `InvalidField::DeviceName` warning
and default used; invalid id → regenerated and saved.

## 6. Links (`modplayer-core::links`)

`pub const STATUS_PAGE_URL: &str = match option_env!("MODPLAYER_STATUS_PAGE_URL") { Some(u) => u, None => "https://github.com/rzcastilho/modplayer/issues" };`
Opened only from the UI via `egui::Context::open_url` (002 design note 10).

## 7. Tests pinning this contract (`modplayer-core`)

- `transport_reducer.rs`: one test per rule T1–T24 (table-driven).
- `queue.rs` (unit) + `queue_proptest.rs`: FR-013/FR-014 branches; history
  bound; play-next contiguity; shuffle cycle completeness; unavailable skip.
- `controller_streaming.rs` with `FakeBackend` + `ScriptedHost`:
  buffered play → audible within 50 ms of `Play` (frames of fixture audio in
  the rendered output); throttled first play → `Buffering` then `Playing`
  exactly when the script releases ≥ 2 s; gapless transition (no zero-run
  and no discontinuity > fixture bound across the boundary); seek-past-end
  advances; transfer-in/away/back; 5 s timeout notification; 30 s transient
  warning and its auto-dismiss; 5-minute transient never becomes
  unavailable; sign-out clears; downgrade lets the track finish.
- `settings.rs`: `[playback]` round-trip and defaults.
