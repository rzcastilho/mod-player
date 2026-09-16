# Data Model: Streaming Playback, Transport, and Queue

**Feature**: 003-streaming-playback-and-queue | **Date**: 2026-09-15

Entities from spec.md "Key Entities", made concrete. Where a type lives is
stated because the crate boundaries are the point (Constitution IV, VII).
`~` marks a type that already exists (001/002) and is extended.

## 1. Value types (`modplayer-audio-source`, trait crate — no dependencies)

### 1.1 `TrackId`

Newtype over the service's stable track identifier in URI form
(`spotify:track:<base62>`). Invariant: non-empty, ASCII, ≤ 64 bytes.
`Display` prints the URI. Used as the primary key everywhere.

### 1.2 `TrackRef` (DM-2, narrowed to what this slice needs)

| Field | Type | Notes |
|---|---|---|
| id | `TrackId` | primary key |
| title | `String` | ≥ 1 char after trim; falls back to "Unknown title" when the service omits it |
| artists | `Vec<String>` | may be empty (shown as "Unknown artist") |
| album | `Option<String>` | |
| artwork_url | `Option<String>` | reference only; not fetched in this slice |
| duration_ms | `u32` | > 0 |
| availability | `Availability` | `Available` / `Unavailable` (region-locked, removed, or refused by the service) |

Constructed by the account read integration (Web API JSON), by the Connect
source from `AudioItem` (transfer-in tracks), and by test fixtures.

### 1.3 `SourceHealth`

```
Ok
Transient { since: Instant, next_retry_in: Duration }
Unavailable { client_update_required: bool }
```
Transitions: `Ok → Transient` on any retryable error; `Transient → Ok` on
recovery; `{Ok, Transient} → Unavailable` only on an unrecoverable
classification (research R7); `Unavailable → Ok` only via `Retry`
re-initialisation. Duration alone never moves `Transient → Unavailable`.

### 1.4 `BufferStatus`

| Field | Type | Notes |
|---|---|---|
| ring_fill_frames | `u32` | decoded frames waiting in the RT ring |
| ready | `bool` | source's readiness for the current position (research R5) |
| current_prefetched | `bool` | current track fully fetched (best effort; `false` when unknown) |
| next | `Option<TrackId>` | the item currently preloaded, if any |

### 1.5 `SourceCommand` / `SourceEvent`

Defined in contracts/audio-source-host.md §2–§3. Both are plain data
(`Clone + Debug`, no secrets), carried over `std::sync::mpsc`.

## 2. Queue (`modplayer-core::queue`, pure, in-memory)

### 2.1 `QueueItem`

| Field | Type | Notes |
|---|---|---|
| uid | `QueueItemId` (u64, monotonically assigned) | identity for the UI and reorders; stable across reorders |
| track | `TrackRef` | |
| origin | `Origin` = `Context` \| `PlayNext` \| `Revealed` | `Revealed` = appended from a source-driven transfer context (research R3); displayed like `Context` |
| unavailable | `bool` | set when the source reported `Unavailable`/`availability == Unavailable`; visible "Unavailable" mark; skipped by advancement |

### 2.2 `Queue`

| Field | Type | Notes |
|---|---|---|
| context | `Vec<QueueItem>` | original context order (all items ever added as `Context`/`Revealed`, minus removals) |
| play_next | `VecDeque<QueueItem>` | the play-next block, FIFO |
| cursor | `Option<Cursor>` | `Cursor::Context(idx)` or `Cursor::PlayNext` (the current item is the head of the block, popped into `current`) |
| current | `Option<QueueItem>` | the item at the cursor (owned separately so the play-next head can be consumed) |
| shuffle | `Option<ShuffleOrder>` | `Some` while shuffle is on: a permutation of the *remaining* context indices after the cursor, seeded by an injected RNG (deterministic in tests) |
| repeat | `Repeat` = `Off` \| `One` \| `All` | |
| history | `VecDeque<QueueItemId>` | items actually played this session, oldest first, bounded 100 |
| mode | `QueueMode` = `HostDriven` \| `SourceDriven` | research R3 |

Invariants:
- `current` is `Some` ⇔ `cursor` is `Some`.
- `history.len() ≤ 100`; an entry is pushed when an item *starts* playing (not when it is loaded while paused).
- Effective order = `[current] ++ play_next ++ upcoming_context` where `upcoming_context` = remaining context items in shuffled order when `shuffle.is_some()`, else in context order after the cursor.
- Items are never removed by being played; `remove(uid)` is the only removal.

Operations (each returns a `QueueChange` describing whether the program must be re-sent and whether playback must restart):

| Operation | Rule (FR) |
|---|---|
| `replace_context(tracks)` | clears everything except `repeat`; cursor → first item; `mode = HostDriven` |
| `add_context(tracks)` / `reveal(track)` | append to `context` (reveal also moves the cursor onto the revealed item when the source reports it started) |
| `play_next(uid)` / `play_next_track(track)` | move/append into `play_next` tail (FR-011 FIFO) |
| `move_up(uid)` / `move_down(uid)` / `reorder(uid, to_effective_index)` | edits the effective order; crossing into/out of the play-next block changes `origin` (FR-012, clarifications) |
| `remove(uid)` | non-current: drop; current: `advance(Skip)` (ignores repeat-one), or empty → `cursor = None` (FR-012, FR-014) |
| `set_shuffle(on, rng)` | on: permute remaining context after the cursor, keep current + play-next; off: restore context order after the block (FR-013) |
| `set_repeat(mode)` | |
| `advance(reason)` | `reason ∈ {TrackEnd, Skip, Removed, SeekPastEnd}`; order: (1) repeat-one only for `TrackEnd`/`SeekPastEnd`; (2) play-next head; (3) next upcoming context; (4) repeat-all → wrap (re-shuffle when shuffled) / repeat-off → `Stopped` with cursor on last item (FR-014). Items marked `unavailable` are skipped with a `Skipped(uid)` change; if none playable remain → end-of-queue (FR-026) |
| `skip_back(position_ms)` | > 3 000 ms or empty history → `RestartCurrent`; else pop history tail → `MoveTo(uid)` (FR-004) |
| `mark_unavailable(track_id)` | sets the flag on every item with that id |
| `program()` | `Program { order: Vec<TrackId>, cursor_index: usize }` = effective order with the current item first… see contracts/transport-and-queue.md §3 for the exact shape sent to the source |
| `next_prefetch_target()` | the item `advance(TrackEnd)` would choose, without mutating (drives the "pre-fetch target recomputed within 1 s" rule; the source receives it implicitly through `program()`) |

Property-tested (SC-011): for any sequence of operations, `advance` never
returns a removed or unavailable item, every context item plays once per
shuffle cycle, the play-next block is always contiguous immediately after
the current item, and `history.len() ≤ 100`.

### 2.3 `Program` (the host → source handoff)

| Field | Type | Notes |
|---|---|---|
| order | `Vec<TrackId>` | full effective order, current item included at `cursor_index` |
| cursor_index | `u32` | |
| position_ms | `u32` | where to (re)start the current item |
| start_playing | `bool` | transport intent |
| repeat_all | `bool` | Spirc `repeat` |
| repeat_one | `bool` | Spirc `repeat_track` |
| generation | `u64` | monotonically increasing; events carry the generation they refer to, so stale `TrackStarted` events are ignored |

## 3. Transport (`modplayer-core::transport`)

### 3.1 `TransportState`

`Stopped | Playing | Paused | Buffering` where `Buffering` is a sub-state of
the `Playing` intent (FR-003). Derived fields kept by the controller:

| Field | Type | Notes |
|---|---|---|
| intent | `Intent` = `Stopped` \| `Playing` \| `Paused` | mirrors engine `Transport` |
| buffering | `bool` | research R5 |
| position | via `PositionClock` (engine `RtShared` anchor) | frozen while `Paused`/`Stopped`/`Buffering` |
| track_len_ms | `Option<u32>` | from the current `QueueItem` |
| active | `ActiveState` = `NotRegistered { reason }` \| `Inactive { other_device: Option<String> }` \| `Active` \| `TransferRequested { since }` | Connect device status (FR-016/018/019/027) |
| health | `SourceHealth` | mirrored from the source |
| reconnect_warning_raised | `bool` | the 30 s warning (FR-021) |

Transition table (excerpt; full table in contracts/transport-and-queue.md §2):

| From | Command | To | Side effects |
|---|---|---|---|
| Stopped | play | Playing (Buffering until ready) | `Program{start_playing:true}` if not loaded; engine `Play` |
| Playing/Buffering | pause | Paused | engine `Pause`; source `Pause` |
| Paused | play | Playing | engine `Play`; source `Play`; no re-buffer when ring/decoder already at position |
| any | stop | Stopped, position 0 | engine `Stop`; source `Stop`; queue/current retained; stays `Active` |
| Stopped | seek(p) | Paused at p | engine `Seek`; source `Seek` |
| Playing | seek(p ≥ len) | advance(SeekPastEnd) | FR-005 clamp |
| Buffering | play | no-op | |
| Inactive | play/skip/seek/Play here | TransferRequested | source `RequestTransferHere`; 5 s timer |
| TransferRequested | `BecameActive` | Active + apply the pending command | |
| TransferRequested | 5 s elapsed | Inactive | warning "Couldn't take over playback" |
| Active | `BecameInactive{device}` | Inactive; intent frozen as Paused semantics | engine `Pause`; banner |
| any | `Health(Unavailable)` | transport disabled | critical notification |

### 3.2 `PendingTransferCommand`

`Play | SkipForward | SkipBack | Seek(ms) | PlayHere` — the command that
requested the transfer and is applied once `BecameActive` arrives.

## 4. Connect Receiver Device (`modplayer-audio-source-connect` + settings)

| Field | Type | Where | Notes |
|---|---|---|---|
| device_name | `DeviceName` (trimmed, 1–64 chars) | `settings.toml [playback] device_name`; `None` = default | default `"ModPlayer on <hostname>"` / `"ModPlayer"` (FR-001) |
| device_id | `String` (32 hex chars) | `settings.toml [playback] connect_device_id` | generated once with `getrandom` |
| registered | `bool` | source state | true between `Registered` and `Deregistered` events |
| active | `bool` | source state (Spirc `SessionConnected/Disconnected`) | |

`DeviceName::parse(input) -> Result<Option<DeviceName>, DeviceNameError>`:
trims; empty → `Ok(None)` (restore default); > 64 chars → `Err(TooLong)`.

## 5. Connect Audio Source internals (`modplayer-audio-source-connect`)

| Struct | Thread | Role |
|---|---|---|
| `ConnectSource` (impl `SourceHost`) | UI thread (owned by the controller) | command sender, event receiver, `SourceRtShared` handle, config (device name/id, tmp dir, status of `Retry`) |
| `ConnectWorker` | dedicated `std::thread` running a tokio multi-thread runtime (2 workers) | builds `Session`, `Player`, `Spirc`; owns the Spirc future; translates `PlayerEvent` → `SourceEvent`, `SourceCommand` → Spirc/Player calls; backoff/reconnect; program bookkeeping (`ProgramMap { generation, order }` to resolve `TrackChanged` → `program_index`) |
| `RingSink` (impl librespot `Sink`) | Player thread | f64→f32, push into `rtrb::Producer<f32>`, maintains `written_frames: AtomicU64`, blocks with 500 µs parks when full |
| `HostMixer` (impl librespot `Mixer`) | Spirc task | stores the volume `u16`; forwards `RemoteVolume` to the worker's event channel; `get_soft_volume` = `NoOpVolume` |
| `ConnectRtSource` (impl `AudioSource`) | audio callback | ring consumer, marker consumer, anchor arithmetic, underrun flag (research R4) |
| `SourceRtShared` | shared `Arc` | atomics: `underrun`, `track_seq`, `track_len_frames`, `ring_fill_frames`, `written_frames`, `consumed_frames` |
| `Marker` | SPSC queue streaming → RT | `{ at_written_frame: u64, track_seq: u32, position_frames: u64, kind: TrackStart \| Reposition \| TrackEnd }` |

Volume scale: `VolumePercent(p)` ↔ Spirc `u16` = `round(p × 65 535 / 100)`;
inbound `u16 v` → `round(v × 100 / 65 535)`. The worker suppresses echo
(`set_volume` → `VolumeChanged` with the value it just sent).

## 6. Remote Transfer Event (`SourceEvent` payloads)

| Event | Payload | Host effect |
|---|---|---|
| `BecameActive { context }` | `context: Option<TransferContext { current: TrackRef, position_ms, playing: bool, shuffle: Option<bool>, repeat: Option<Repeat> }>` | `Some` (transfer-to): queue ← `[current]`, mode `SourceDriven`, modes from context when supplied; in-flight local command discarded; transport ← `Playing`/`Paused` per `playing`. `None` (own activation after `RequestTransferHere`/`activate`): apply the pending command |
| `BecameInactive` | none (other device name fetched via R3) | freeze (Paused semantics), banner "Playing on <name>" with **Play here** |

## 7. Account-session interactions (from 002, unchanged types)

| Trigger (002 event / state) | Playback effect (FR-027) |
|---|---|
| `playback_permitted() == false` (tier Free/Unknown) | `ActiveState::NotRegistered { reason: PremiumRequired \| SubscriptionNotVerified }`; source `Deregister`; "Play from account" hidden |
| `SignedOut` / `SessionRevoked` | controller `clear_for_sign_out()`: stop, clear queue + buffers, source `Deregister` (synchronous, before 002's report) |
| `SessionExpired` | nothing new; next load fails `Unauthenticated` → source `Transient` → transport `Buffering` |
| `TierRejected` from source, or `TierChecked(Free)` | let current track finish (advance blocked), then disable + deregister + warning with **Open upgrade page** |
| `Authorized/TierChecked(Premium)` | source `Initialize` (register) |

## 8. Settings file delta (`settings.toml`, device-scoped)

```toml
[playback]
device_name = "Studio Mac"          # optional; absent = default name
connect_device_id = "3f9c…"         # 32 hex chars; generated on first need
```
Unknown/invalid `device_name` (over-length, non-UTF-8) → treated as absent
with the existing `InvalidValue` warning path; invalid `connect_device_id` →
regenerated. Schema version unchanged (additive keys).

## 9. Notifications introduced (keys in contracts/ui-surface.md)

| Key | Severity | Action | Trigger |
|---|---|---|---|
| `stream-reconnect-warning` | Warning | — (auto-cleared) | transient failure ≥ 30 s |
| `stream-source-unavailable` | Critical | `OpenStatusPage` + `RetrySource` | health `Unavailable{false}` |
| `stream-source-update-required` | Critical | `OpenStatusPage` | health `Unavailable{true}` |
| `transfer-request-failed` | Warning | — | 5 s transfer timeout |
| `queue-item-skipped-unavailable` | Info (`{ $title }`) | — | FR-026 |
| `subscription-downgraded` | Warning | `OpenUpgradePage` | FR-027 |
| `play-from-account-failed` | Warning (`{ $reason }`) | — | developer action failure |

`NotificationAction` (~, core) gains `OpenStatusPage`, `RetrySource`,
`OpenUpgradePage` alongside 002's `SignIn`.
