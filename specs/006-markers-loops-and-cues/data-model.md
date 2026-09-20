# Data Model: Markers, Loop Regions, and Cue Points

**Feature**: 006-markers-loops-and-cues | **Date**: 2026-09-17 |
**Spec**: [spec.md](spec.md) (Key Entities, FR-001–FR-028) | **Research**: [research.md](research.md)

Four layers, one direction of truth: the persisted file (§4) is loaded
into the core model (§1), the core model is the shadow state the
controller derives the engine's real-time copy (§2) from, and the UI
state (§3) is per-session view state that never outlives the view.

Positions are **frames at the source sample rate** everywhere except the
`m:ss.mmm` display and `SourceCommand::Seek(ms)`.

---

## 1. Core model — `modplayer_core::markers::model`

### 1.1 `MarkerId`

`#[derive(Copy, Eq, Hash, Ord)] pub struct MarkerId(u32)` — allocated by
`TrackMarkers::next_id` (monotonic per loaded track state, persisted so
ids are stable across sessions and a saved file never reuses an id).

### 1.2 `MarkerKind`

```rust
pub enum MarkerKind {
    Point,
    RegionStart { region: RegionId },
    RegionEnd   { region: RegionId },
    Cue         { slot: CueSlot },       // CueSlot(u8) invariant 1..=8
}
```

### 1.3 `Marker` (DM-6)

| Field | Type | Rules |
|---|---|---|
| `id` | `MarkerId` | unique per track |
| `kind` | `MarkerKind` | region markers reference an existing `LoopRegion`; cue slots unique per track (FR-013) |
| `position` | `u64` frames | `0 ..= len_frames` (FR-019 clamp on every set) |
| `name` | `String` | trimmed, ≤ 64 chars (`MAX_NAME_CHARS`); `Point` default `"Marker n"` (`n` = point markers on the track + 1 at creation); region/cue default empty (FR-001) |
| `color` | `PaletteIndex(u8)` | `0..=7`; defaults by kind: region `0`, point `1`, cue `2` |
| `owner` | `Owner` | `Owner::Host` only in this slice (FR-024); the enum exists so the file format never changes when plugins land |
| `transient` | `bool` | always `false` here; persisted for forward compatibility |
| `visible` | `bool` | always `true` here |
| `clamped` | `bool` | **session-only** (not persisted): set by `load` when the saved position exceeded the current track length (FR-018 warning glyph); cleared the next time the marker is moved |

### 1.4 `LoopRegion` (DM-7)

| Field | Type | Rules |
|---|---|---|
| `id` | `RegionId(u32)` | unique per track |
| `a` | `Option<MarkerId>` | the `RegionStart` marker, `None` while incomplete |
| `b` | `Option<MarkerId>` | the `RegionEnd` marker, `None` while incomplete |
| `crossfade_ms` | `u8` | `0..=50`, default `5` (FR-010) |
| `repeat` | `RepeatCount` | `Infinite` (default) or `Times(u16)` with `1..=1000` (FR-011) |
| `armed` | `bool` | **session-only**: never serialized; every region loads disarmed (FR-016) |
| `wraps` | `u32` | session-only mirror of `RtShared::loop_wraps`, reset on arm |

Derived (methods, not fields): `is_complete()` (`a` and `b` both set),
`span(&TrackMarkers) -> Option<(u64, u64)>`, `length_frames()`,
`is_armable(rate)` = complete ∧ `length ≥ min_region_frames(rate)`
where `min_region_frames = max(1, rate / 1000)` (1 ms, rounded down to
whole frames), `effective_crossfade_frames(rate)` =
`loop_math::effective_crossfade(configured_frames, a, b)` =
`min(configured, b − a, a)`.

### 1.5 `TrackMarkers` — the per-track aggregate

```rust
pub struct TrackMarkers {
    track: TrackId,
    len_frames: u64,                 // current track length (clamp bound)
    sample_rate: u32,
    markers: Vec<Marker>,            // kept sorted by (position, id)
    regions: Vec<LoopRegion>,
    current_region: Option<RegionId>,
    next_marker_id: u32,
    next_region_id: u32,
    dirty: bool,                     // set by every mutation; cleared by save
}
```

Invariants (all enforced inside the mutation methods and pinned by the
`markers_model` proptests):

- **I1 Limit**: `markers.len() ≤ MAX_MARKERS = 64` counting every kind,
  including the lone endpoint of an incomplete region (FR-002). Every
  creation path returns `Err(MarkerError::LimitReached)` at 64; move-only
  paths (`I`/`O` on an existing endpoint, `Shift+n` on an occupied slot)
  never check the limit.
- **I2 Order**: after every mutation `markers` is sorted by
  `(position, id)`; equal positions are allowed (spec edge case).
- **I3 A ≤ B**: after any position change to a region endpoint, if
  `pos(a) > pos(b)` the two markers' *kinds* are swapped
  (`RegionStart` ↔ `RegionEnd`); names and colours stay with their
  markers (FR-007). Equal positions do not swap.
- **I4 Clamp**: every set position is `min(position, len_frames)`
  (FR-019).
- **I5 Cue uniqueness**: at most one `Cue { slot }` per slot (FR-013);
  `set_cue(slot, pos)` moves the existing one.
- **I6 One armed**: at most one region has `armed == true` (FR-008);
  `arm(region)` disarms any other.
- **I7 Armable**: `arm` returns `Err(RegionIncomplete)` or
  `Err(RegionTooShort)` per §1.4's `is_armable` (FR-007/FR-008).
- **I8 Current region**: `current_region` is `Some` iff `regions` is
  non-empty, and always names an existing region; it is set by
  `new_loop_region`, by `set_loop_a/b` creating the first region, and by
  `select_marker` on a region endpoint (FR-006).
- **I9 Endpoint deletion**: deleting a region endpoint clears that side
  (`a`/`b` = `None`), disarms the region; deleting the last endpoint
  removes the region (and `current_region` falls back to the most recently
  created remaining region or `None`) (FR-006).

Mutation API (all `Result<_, MarkerError>`; every `Ok` sets `dirty`):

| Method | Behaviour |
|---|---|
| `add_point(pos) -> MarkerId` | I1, default name/colour, I2 |
| `set_loop_a(pos) / set_loop_b(pos) -> (RegionId, MarkerId)` | on `current_region` (creating a region if none: I8); creates the endpoint (I1) or moves it; I3, I4 |
| `new_loop_region() -> RegionId` | empty incomplete region becomes current; no marker yet (no limit check until `I`/`O`) |
| `set_cue(slot, pos) -> MarkerId` | I5, I1 only when creating |
| `move_marker(id, pos)` | I3, I4, clears `clamped` |
| `rename(id, name)` | trim, truncate to 64 chars; empty on a `Point` keeps the old name |
| `recolor(id, PaletteIndex)` / `cycle_color(id)` | |
| `delete(id)` | I9; a cue frees its slot |
| `select_marker(id)` | I8 (region endpoints set `current_region`) |
| `arm(region) / disarm()` / `toggle_current()` | I6, I7; `arm` resets `wraps = 0` |
| `set_crossfade_ms(region, u8)` / `set_repeat(region, RepeatCount)` | clamp `0..=50` / `1..=1000` |
| `clear_all()` | empties everything, `current_region = None` |
| `set_len_frames(len)` | re-clamps and sets `clamped` on any marker beyond `len` (FR-018) |
| Queries | `markers()`, `regions()`, `region(id)`, `marker(id)`, `cue(slot)`, `current_region()`, `armed_region()`, `count()`, `position_of(id)` |

`MarkerError`: `LimitReached`, `RegionTooShort`, `RegionIncomplete`,
`NotFound`, `NoTrack` (controller-level: no current track).

### 1.6 Engine-side pure arithmetic — `modplayer_engine::loop_math`

```rust
pub fn effective_crossfade(configured: u64, a: u64, b: u64) -> u64   // min(configured, b − a, a)
pub fn wrap_position(a: u64, b: u64, elapsed: u64) -> u64            // a + ((elapsed − a) mod (b − a)) for elapsed ≥ a
pub fn crossfade_gains(index: u64, x: u64) -> (f32, f32)             // (cos, sin) of t·π/2, t = (index + 1)/(x + 1)
```

---

## 2. Real-time copy — `modplayer_engine::Processor`

```rust
struct LoopRt {
    a: u64,
    b: u64,
    crossfade_frames: u32,   // configured, in source frames
    repeat: u32,             // 0 = infinite
    wraps: u32,
}
struct SeamRt {              // an in-progress seam (FR-011a: completes with the old bounds)
    a: u64, b: u64, x: u64, gapless: bool,
}
// Processor fields added:
loop_staged: LoopRt,          // written by LoopSetA/LoopSetB/LoopSetSeam
loop_active: Option<LoopRt>,  // replaced only by LoopCommit / cleared by LoopDisarm
seam: Option<SeamRt>,
seam_in: Vec<f32>,            // preallocated 2 × max(x) samples, sized at construction from source_rate (50 ms)
```

Per-render decision (only while `transport == Playing`; the position is
`source.position()` — the engine clock's own cursor):

| Condition at the segment start | Segment |
|---|---|
| `loop_active == None` or `pos ≥ b` or `pos < a` (**armed-inactive** when armed) | plain fill to the buffer end (or to `a` when `pos < a`, then re-evaluate — natural entry) |
| `a ≤ pos < b − x` (**armed-active**) | fill to `min(buffer end, b − x)` |
| `b − x ≤ pos < b` | seam: fill outgoing from source, read incoming `[a − x + i, …)` from `decoded_store()`, mix with `crossfade_gains`; if the store did not yield all `x` frames at seam start → `x = 0`, `gapless = false` |
| `pos == b` | `source.seek(a)`; `wraps += 1`; push `LoopWrapped`; if `repeat != 0 && wraps ≥ repeat` → `loop_active = None`, push `LoopReleased` |

`x` is captured into `SeamRt` at the seam's first frame from the *then*
active bounds; a `LoopCommit` arriving mid-seam updates `loop_active` but
the running `SeamRt` finishes unchanged (FR-011a).

`RtShared` additions: `loop_wraps: AtomicU32`, `loop_state: AtomicU8`
(0 disarmed / 1 armed-inactive / 2 armed-active) — written once per
render, read by the controller/UI.

Position publication after a same-render wrap: see research R8.

---

## 3. UI state — `modplayer_ui::waveform::WaveformState` (extended) and `now_playing::markers`

```rust
pub struct MarkerDrag {
    pub marker: MarkerId,
    pub origin_position: u64,        // restored on Esc
    pub live: u64,                   // relative-delta accumulated (research R14)
    pub origin_detail: DetailWindow, // restored on Esc
}
// WaveformState gains:
pub marker_drag: Option<MarkerDrag>,
pub focused_marker: Option<MarkerId>,   // mirror of egui focus for the keyboard table
pub rename: Option<(MarkerId, String)>, // inline rename draft
pub clear_confirm: bool,                // "Clear N markers?" two-step state
pub text_field_ids: Vec<egui::Id>,      // rebuilt each frame; shortcut guard (research R17)
```

Per-frame derived values (never stored): `playhead_frame`, `len_frames`,
`sample_rate`, the sorted marker list projection
(`MarkerRow { id, kind_label, name, position_text, color, clamped, region: Option<RegionRow> }`)
and `RegionRow { id, armed, active, wraps_remaining: Option<u32>, repeat, crossfade_ms, armable: Result<(), MarkerError> }`.

Region visual state (FR-026):

| `armed` | `RtShared::loop_state` | Span style |
|---|---|---|
| false | 0 | outline only |
| true | 1 | hatched fill ("armed but inactive") |
| true | 2 | solid translucent fill |

---

## 4. Persisted file — `<track-state dir>/<hex(track_id)>.json`

```json
{
  "schema_version": 1,
  "track_id": "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
  "sample_rate": 44100,
  "len_frames": 12345678,
  "next_marker_id": 9,
  "next_region_id": 2,
  "current_region": 1,
  "markers": [
    { "id": 1, "kind": "region_start", "region": 1, "position": 441000, "name": "", "color": 0, "owner": "host", "transient": false, "visible": true },
    { "id": 2, "kind": "region_end",   "region": 1, "position": 882000, "name": "", "color": 0, "owner": "host", "transient": false, "visible": true },
    { "id": 3, "kind": "point",                     "position": 100,    "name": "Marker 1", "color": 1, "owner": "host", "transient": false, "visible": true },
    { "id": 4, "kind": "cue", "slot": 3,            "position": 2000,   "name": "", "color": 2, "owner": "host", "transient": false, "visible": true }
  ],
  "regions": [
    { "id": 1, "a": 1, "b": 2, "crossfade_ms": 5, "repeat": null }
  ]
}
```

Rules (FR-016, FR-017; contracts/marker-service.md §3):

- Written in full on every debounced flush; `.json.tmp` + `sync_all` +
  `rename`; user-only permissions where the platform supports them.
- `armed`, `wraps`, `clamped` are never written.
- `repeat: null` = infinite; an integer is clamped to `1..=1000` on load.
- `color` clamped to `0..=7`; `crossfade_ms` to `0..=50`; `position` to
  the *current* `len_frames` (setting `clamped`); a `cue` whose `slot` is
  outside `1..=8` or duplicates an earlier one is dropped; a region
  marker whose `region` does not exist, or a region whose `a`/`b` does
  not name a marker of the matching kind, is repaired (the dangling side
  becomes `None`; an orphan marker becomes a `point`).
- Unknown keys ignored; missing optional keys take defaults;
  `schema_version > 1` → `Warning` `track-state-newer-version`, in-memory
  empty state, file not rewritten until the user mutates.
- Unparseable JSON, I/O error, or a file > 64 KiB → `Warning`
  `track-state-unreadable`, empty state, replaced on the next mutation.
- Missing file → empty state, no notice.
- `len_frames` in the file is informational (what the track length was
  when saved) — the current length always wins.

---

## 5. Settings delta — `settings.toml`

```toml
[markers]
nudge_step_ms = 10        # 1..=1000, clamped silently
```

`RawMarkers { nudge_step_ms: i64 }` (`#[serde(default)]` table),
`AudioSettings.nudge_step_ms: u16`. Descriptor `setting-nudge-step` in
`settings_registry::DESCRIPTORS` under `SettingsCategory::Playback`.

---

## 6. Notifications (keys in `locales/en-US/playback.ftl`)

| Key | Severity | When |
|---|---|---|
| `track-state-unreadable` | Warning | §4 unreadable rule, once per load |
| `track-state-newer-version` | Warning | §4 newer schema rule, once per load |
| `track-state-save-failed` | Warning | the writer thread reports a failed write (previous file intact); no automatic retry |

---

## 7. State transitions

### 7.1 Loop region

```
(none) --I or "New loop region"--> Incomplete{a only | none}
Incomplete --O (or I)--> Complete{disarmed}
Complete{disarmed} --arm (armable)--> Armed{wraps=0}
Armed --disarm / arm(other) / track change / sign-out / LoopReleased--> Complete{disarmed}
Armed --delete endpoint--> Incomplete{disarmed}
Complete/Incomplete --delete last endpoint--> (removed)
```

### 7.2 Engine loop state (`RtShared::loop_state`)

```
0 disarmed --LoopCommit--> 1|2 (by position)  --LoopDisarm / repeat reached--> 0
1 armed-inactive <--position enters [a,b) / leaves--> 2 armed-active
```

### 7.3 Marker drag (UI)

```
idle --press on glyph + drag threshold--> dragging{live = origin}
dragging --pointer Δx--> dragging{live += Δx·fpp; detail recenters/zooms}
dragging --release--> idle (commit move_marker(live), detail keeps window, follow suspended)
dragging --Esc--> idle (no move, detail restored)
```

### 7.4 Per-track state lifecycle (controller)

```
track change: flush(prev) → LoopDisarm → load(new) [warnings] → set_len_frames(current len)
mutation: dirty=true → (≤ 250 ms) → serialize → writer thread → .tmp/sync/rename
sign-out: flush → clear in-memory state
shutdown: flush → drop sender → join writer
```
