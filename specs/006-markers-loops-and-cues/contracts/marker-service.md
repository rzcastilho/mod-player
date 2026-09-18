# Contract: Marker service, per-track state, and controller wiring

**Crate**: `modplayer-core` (new `markers/{mod,model,store}.rs`;
`controller.rs`, `settings/model.rs`, `settings_registry.rs`,
`notifications.rs`). Implements FR-001, FR-002, FR-006–FR-008, FR-011,
FR-013–FR-019, FR-021 (data half), FR-024, FR-027; research R5, R6, R9–R12,
R18; data-model.md §1, §4, §5, §6, §7.

## 1. `PlaybackController` API (all on the controller/UI thread)

Every mutator: applies the change to `TrackMarkers` (the shadow state),
marks it dirty (debounced save, §3), and — when it affects the armed
region — derives and pushes the engine commands of §2. All return
`Result<_, MarkerError>`; `NoTrack` when `current_track()` is `None`.

| Method | Effect |
|---|---|
| `markers(&self) -> Option<&TrackMarkers>` | current track's model (read-only projection for the UI) |
| `add_point_marker(&mut self) -> Result<MarkerId>` | at the current playhead frame (`position()` → frames), FR-001 |
| `set_loop_a(&mut self)` / `set_loop_b(&mut self)` | at the playhead; FR-006 (creates region/endpoint or moves it; I3 swap; if the region is armed → §2 re-commit with `reset_wraps: false`) |
| `new_loop_region(&mut self) -> RegionId` | list action; becomes current |
| `set_cue(&mut self, slot: CueSlot)` | at the playhead; FR-013 |
| `jump_to_cue(&mut self, slot: CueSlot) -> bool` | `seek_frames(cue.position)` through 005's path (transport-state rule of FR-014 is the reducer's existing behaviour); `false` (no-op) when the slot is empty |
| `move_marker(&mut self, id, frame)` | drag/nudge commit; FR-019 clamp inside the model; armed region → re-commit |
| `nudge_marker(&mut self, id, direction: i8, multiplier: u8)` | `frame ± nudge_step_frames × multiplier` (`multiplier` 1 or 10), saturating at 0/len |
| `rename_marker(&mut self, id, name: &str)` / `recolor_marker(&mut self, id, PaletteIndex)` / `cycle_marker_color(&mut self, id)` | |
| `delete_marker(&mut self, id)` | I9; if the deleted marker belonged to the armed region → `LoopDisarm` |
| `select_marker(&mut self, id)` | I8 only (no engine effect) |
| `arm_loop(&mut self, region: RegionId)` / `disarm_loop(&mut self)` / `toggle_current_loop(&mut self)` | I6/I7; arm → §2 with `reset_wraps: true` + `SourceCommand::PrefetchHint { frame: a − x }`; disarm → `LoopDisarm` |
| `set_loop_repeat(&mut self, region, RepeatCount)` / `set_loop_crossfade_ms(&mut self, region, u8)` | armed region → re-commit (`reset_wraps: false`) |
| `clear_all_markers(&mut self)` | `LoopDisarm` if armed; empties the model; immediate flush (writes the empty file) |
| `loop_status(&self) -> LoopStatus { state: LoopState, wraps: u32 }` | reads `RtShared::loop_state/loop_wraps` |
| `nudge_step_ms(&self) -> u16` / `set_nudge_step_ms(&mut self, u16) -> Result<(), SettingsError>` | clamps `1..=1000`, persists through `SettingsStore` like `set_device_name` |
| `track_state_dir(&self) -> Option<&Path>` | for tests/diagnostics |

`TrackMarkers` mutation semantics are data-model.md §1.5's; the
controller adds only the playhead lookup, the engine derivation and the
persistence trigger.

## 2. Engine derivation (research R4)

When a region becomes/stays armed:

```
x_frames   = round(crossfade_ms × rate / 1000)
repeat_u32 = match repeat { Infinite => 0, Times(n) => n }
push LoopSetA(pos(a)); push LoopSetB(pos(b));
push LoopSetSeam { crossfade_frames: x_frames, repeat: repeat_u32 };
push LoopCommit { reset_wraps }
```

`LoopDisarm` on: explicit disarm, arming another region, deleting an
endpoint of the armed region, `clear_all_markers`, every current-track
change (`sync_marker_attachment`, §4), `clear_for_sign_out`, and
`Event::LoopReleased` handling (model side only — the RT already cleared
itself). On stream rebuild (`open_stream`/device change) the four setters
+ `LoopCommit { reset_wraps: false }` are re-pushed when a region is
armed (research R6).

## 3. Per-track state store (`markers::store`)

```rust
pub const TRACK_STATE_DIR_ENV: &str = "MODPLAYER_TRACK_STATE_DIR";
pub struct TrackStatePaths { dir: PathBuf }              // resolve() → env or <data_local_dir>/ModPlayer/track-state
impl TrackStatePaths { pub fn resolve() -> Option<Self>; pub fn with_dir(dir) -> Self; pub fn file_for(&self, id: &TrackId) -> PathBuf; }
pub fn encode_track_id(id: &TrackId) -> String;           // lowercase hex of the bytes
pub fn decode_track_id(name: &str) -> Option<TrackId>;    // inverse; None on odd length / non-hex / invalid id
pub enum LoadWarning { Unreadable, NewerSchema }
pub struct LoadOutcome { pub state: TrackMarkers, pub warning: Option<LoadWarning>, pub rewrite_allowed: bool }
pub fn load(paths: &TrackStatePaths, id: &TrackId, rate: u32, len_frames: u64) -> LoadOutcome;
pub fn encode(state: &TrackMarkers) -> Vec<u8>;           // serde_json, data-model.md §4
pub enum PersistJob { Save { path: PathBuf, bytes: Vec<u8> }, Delete { path: PathBuf } }
pub fn spawn_writer(notify: Sender<StoreEvent>) -> (Sender<PersistJob>, JoinHandle<()>);
pub enum StoreEvent { SaveFailed { path: PathBuf } }
```

Rules:

1. **Write**: `path.with_extension("json.tmp")` → `write_all` →
   `sync_all` → `rename(tmp, path)`; on any error the previous file is
   left untouched and `StoreEvent::SaveFailed` is sent (controller raises
   `track-state-save-failed`, no retry). Directory created on first write.
2. **Debounce**: `TRACK_STATE_DEBOUNCE = 250 ms`, checked in `tick()`;
   the last mutation before the window elapses wins; `flush_track_state()`
   forces it (track change, sign-out, shutdown, `clear_all_markers`).
3. **Read** (data-model.md §4): missing → empty, no warning; > 64 KiB,
   I/O error or invalid JSON → empty + `Unreadable`, `rewrite_allowed =
   true`; `schema_version > 1` → empty + `NewerSchema`,
   `rewrite_allowed = false` until the first mutation (the controller
   only flushes a `NewerSchema` state once `dirty` was set by a user
   action); field repairs as listed there. The synthetic host's track id
   (`modplayer:synthetic:built-in`) persists like any other.
4. **Not encrypted, not deleted on sign-out** (markers are not audio, not
   account data; spec assumption). `clear_for_sign_out` flushes and drops
   the in-memory state only.
5. **Rate change**: a file recorded at a different `sample_rate` is
   rescaled on load (research R12).

## 4. Controller wiring

| Trigger | Action |
|---|---|
| `sync_marker_attachment()` after every `dispatch` (same hook as `sync_analysis_attachment`), when `queue.current()`'s id changed **or the same id was re-started** (`TrackStarted` accepted for the same id — FR-016 "same-session reload") | `flush_track_state()` for the previous track; `LoopDisarm`; `load(new)` → raise warning if any; `set_len_frames(current len)`; `marker_state_track = new id` |
| `Input::TrackStarted` updating `track_len_ms` | `set_len_frames` (FR-018 clamps + flags) |
| `tick()` | drain `event_rx` (engine events): `LoopWrapped` → coalesced `SourceCommand::Seek(a_ms)` (first immediately, then ≤ 1 per `LOOP_RESEEK_INTERVAL = 250 ms`); `LoopReleased` → `markers.disarm()` (model only); `ToneFinished`/`TrackLooped` unchanged (ignored); then `flush_track_state_if_due()`; drain `StoreEvent`s |
| `map_source_event(EndOfTrack)` | ignored while `shared.loop_state() == 2` (research R5 rule 3) |
| `clear_for_sign_out()` | flush, drop model, `LoopDisarm` |
| `shutdown()` | flush, drop the writer sender, join the writer (after `analysis.shutdown()`) |
| `open_stream`/rebuild | re-push engine loop commands when armed (§2) |
| `PlaybackController::new` | `TrackStatePaths::resolve()`; `None` → in-memory only (no warning, like `LibraryPaths`) |

## 5. Settings delta (FR-027)

| Item | Value |
|---|---|
| `settings.toml` | `[markers] nudge_step_ms = 10` (`RawMarkers`, `#[serde(default)]`) |
| Domain | `AudioSettings.nudge_step_ms: u16`, clamped `1..=1000` silently |
| Registry | `SettingDescriptor { id: "markers.nudge_step_ms", category: Playback, title_key: "setting-nudge-step", desc_key: "setting-nudge-step-desc" }` |
| UI | Settings › Playback `DragValue` (1–1000, suffix ` ms`), labelled by `setting-nudge-step`, commits via `set_nudge_step_ms` on change |

## 6. Notification keys (`notifications.rs`)

`KEY_TRACK_STATE_UNREADABLE = "track-state-unreadable"`,
`KEY_TRACK_STATE_NEWER_VERSION = "track-state-newer-version"`,
`KEY_TRACK_STATE_SAVE_FAILED = "track-state-save-failed"` — all
`Severity::Warning`, raised at most once per load/failed write, dismissed
like any other.

## 7. Tests pinning this contract

`crates/modplayer-core/tests/markers_model.rs` (proptest + unit):

- `limit_is_64_across_all_kinds` / `move_paths_never_hit_limit` (SC-005)
- `a_after_b_swaps_kinds_keeps_names` / `equal_positions_do_not_swap` (SC-006)
- `region_under_1ms_is_not_armable` / `region_3ms_is_armable_with_3ms_effective_crossfade`
- `positions_clamp_to_len` (proptest) / `set_len_flags_clamped_markers_only` (SC-012)
- `cue_slot_is_unique_and_set_moves` / `delete_cue_frees_slot`
- `delete_endpoint_makes_region_incomplete_and_disarmed` / `delete_last_endpoint_removes_region`
- `only_one_region_armed` / `arm_resets_wraps`
- `default_names_and_colors_by_kind` / `rename_trims_truncates_and_keeps_old_on_empty_point`
- `markers_stay_sorted_after_any_sequence` (proptest over random op sequences)

`crates/modplayer-core/tests/markers_store.rs`:

- `round_trip_is_identity_for_any_state` (proptest; `armed`/`wraps`/`clamped` excluded)
- `crash_mid_write_keeps_previous_file` (write `.tmp`, skip `rename`, reload → previous state) (SC-009, SC-014)
- `missing_file_loads_empty_without_warning` / `unparseable_loads_empty_with_unreadable` / `newer_schema_loads_empty_and_is_not_rewritten_until_mutation` / `oversize_file_is_unreadable`
- `unknown_keys_ignored_and_out_of_range_clamped` / `dangling_region_refs_are_repaired`
- `track_id_hex_encoding_round_trips_and_is_case_free` (proptest over valid ids)
- `writer_reports_save_failed_and_leaves_previous_file` (read-only dir)

`crates/modplayer-core/tests/controller_markers.rs` (`FakeBackend` +
`ScriptedHost`/`SyntheticHost`):

- `arm_pushes_setters_then_commit_and_prefetch_hint`
- `edit_armed_region_recommits_without_resetting_wraps`
- `track_change_flushes_disarms_then_loads` / `same_track_restart_reloads_and_clears_armed`
- `loop_wrapped_reseeks_source_once_per_tick_then_throttled`
- `end_of_track_ignored_while_loop_active` / `end_of_track_advances_while_loop_inactive`
- `loop_released_disarms_model`
- `stream_rebuild_repushes_armed_region`
- `clear_all_writes_empty_state_immediately`
- `sign_out_flushes_and_drops_state_but_keeps_file`
- `nudge_step_setting_round_trips_and_clamps` (`tests/settings.rs`)
- `warnings_raised_once_per_load` (`Unreadable`, `NewerSchema`, `SaveFailed`)
