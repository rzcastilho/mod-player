# Contract: Now Playing waveform surface (extends 003 contracts/ui-surface.md §1)

**Crate**: `modplayer-ui` (`now_playing.rs`, new `waveform/`). Locale:
en-US, keys in `locales/en-US/playback.ftl`. Every control is
keyboard-operable with an accessible name, role and state (FR-013,
FR-018; the `accessibility` test enumerates them). Implements FR-001,
FR-002, FR-009, FR-011–FR-015, FR-018–FR-020; research R10–R13, R15;
data-model.md §5.

## 1. Now Playing layout (top to bottom)

| Element | Accessible name key | Behaviour |
|---|---|---|
| Artwork (96 px) | `now-playing-artwork` (`{ $title }`) | `ArtworkCache::get(track.artwork_url)`; `Loading`/`Failed`/no URL → `initials_placeholder(album title, 96)` (004 FR-020) |
| Title / artists / album | `now-playing-title`, `now-playing-artist`, `now-playing-album` | from `current_track()`; album line omitted when `None` |
| Status line, transfer banner | unchanged (003) | |
| Transport buttons, Queue toggle | unchanged (003) | |
| **Elapsed** label | `time-elapsed` (`{ $time }`) | `m:ss` of the playhead (preview while dragging) |
| **Waveform overview** | `transport-seek` (role slider, value text `m:ss / m:ss`, description `waveform-overview-desc` = `waveform-detail-window { $start } { $end }`) | full track; playhead line; detail window as translucent region; click/drag seek |
| **Remaining** label | `time-remaining` (`{ $time }`) | `-m:ss` of `len − playhead` |
| **Waveform detail** | `waveform-detail` (role slider, value text `m:ss / m:ss`, description `waveform-detail-window { $start } { $end }`) | `DetailWindow` (data-model.md §5.1); playhead line; click/drag seek; zoom/pan |
| Master volume + peak meter | unchanged (001) | |
| Queue panel | unchanged (003) | |
| Empty state (no current track) | `now-playing-pick-a-track` | replaces artwork/labels/waveforms with a centred hint; transport buttons keep 003's disabled state |
| Analysis unavailable | `waveform-unavailable` | centred label inside each waveform rect when `status == Failed && peaks.is_none()`; the rect remains a seek surface |

Removed: 003's `Slider` seek control and the `transport-position` label
(key deleted from `playback.ftl`; `fluent_keys` test updated).

## 2. Pointer behaviour (both waveforms)

| Input | Effect |
|---|---|
| click (press/release under egui's drag threshold) | `controller.seek_frames(space.frame_at(x))` |
| drag | `state.drag = Some(target = frame_at(x))` every frame; playhead + both labels show the preview; audio unchanged; release → `seek_frames(target)`, `drag = None` |
| `Esc` during drag | `drag = None`, no seek |
| vertical wheel / pinch (`zoom_delta`) on either waveform | detail `zoom_about(frame_at(pointer_x_in_detail_space), factor)`; on the overview the anchor is the playhead |
| horizontal scroll / `Shift`+wheel | detail `pan(±frames)` |
| drag on either waveform is never a pan | FR-009 |

## 3. Keyboard (either waveform focused) — FR-013

| Key | Action |
|---|---|
| `←` / `→` | `seek_frames(playhead ∓ 5 s)` |
| `Shift+←` / `Shift+→` | `seek_frames(playhead ∓ 500 ms)` |
| `Home` / `End` | `seek_frames(0)` / `seek_frames(len)` |
| `+` or `=` / `-` | detail `zoom_step(playhead, ×2)` / `(÷2)` |
| `0` | detail `reset(len)`; follow re-enabled |
| `Alt+←` / `Alt+→` | detail `pan(∓ 10 % of width)` |
| `Alt+Shift+←` / `Alt+Shift+→` | detail `pan(∓ width)` |
| `Esc` | cancel drag (only while dragging) |
| `Tab` | focus moves between overview and detail like any egui widget |

Identical on macOS / Windows / Linux; the keys are consumed only while a
waveform has focus so 003's other shortcuts (`Space`, `Cmd/Ctrl+Q`) are
unaffected.

## 4. Rendering rules

- Column source: `peaks.level_for(space.frames_per_pixel())`; a column
  = the buckets overlapping that pixel's frame range; present → filled
  `min..max` bar centred vertically; any absent → placeholder (dimmed
  hatched band, 40 % height). `Pending`/no snapshot → all placeholder.
- Playhead: 1 px accent line at `space.x_of(playhead)` on both views,
  redrawn every frame while playing (`request_repaint_after(16 ms)` as
  003).
- Overview highlight: `rect` from `x_of(detail.start)` to
  `x_of(detail.start + width)`, translucent accent fill; not focusable.
- Track length for the coordinate space = `transport_state().track_len_ms`
  (peaks' `len_frames` may differ by ≤ 1 chunk at the tail; the space
  always uses the transport length).
- Theme: colours from `theme.rs` tokens only (no new literals).

## 5. Follow (FR-012), applied once per frame before drawing

```
if track changed          → detail = initial(playhead, len) keeping the previous width if any
if drag is None:
  if playing && follow    → detail.follow_playhead(playhead)
  last_seek observed      → detail.recenter(target) if !detail.contains(target); follow = true
user pan/zoom this frame  → detail.suspend_follow_if_outside(playhead)
'0' pressed               → detail.reset(len); follow = true
```

## 6. `TimeSpace` for later slices (FR-014)

`waveform::overview(ui, ..) -> WaveformResponse { response, space }` and
`waveform::detail(ui, ..) -> WaveformResponse`. `space.x_of(frame)` /
`frame_at(x)` are the only mapping later overlays use; the widgets
accept an `overlays: &mut dyn FnMut(&Painter, &TimeSpace)` hook (empty
in this slice) so markers/loops/plugin overlays paint after the peaks
and before the playhead without changing the widget.

## 7. Tests pinning this contract (`modplayer-ui`)

- `accessibility`: overview = role slider, name `transport-seek`, value
  `m:ss / m:ss`; detail = role slider, name `waveform-detail`,
  description contains `1:10` and `1:40` for a 30 s window at 1:25; the
  empty state exposes `now-playing-pick-a-track`; `transport-position`
  no longer present; every key in §3 reachable.
- `now_playing::seek_slider_commits_once_per_release` (003) retargeted:
  a drag on the overview produces exactly one `seek_frames` on release.
- `now_playing::click_on_overview_seeks_to_exact_frame` — click at the
  pixel for 1:23.500 → `Command::Seek(3_682_350)` on the engine queue.
- `now_playing::drag_previews_without_seeking_and_esc_cancels`.
- `now_playing::keyboard_table_matches_pointer_results` — each row of §3
  vs the equivalent pointer action → same controller calls.
- `now_playing::seek_while_paused_stays_paused` /
  `::seek_while_stopped_enters_paused`.
- `now_playing::placeholder_columns_for_uncovered_buckets` — a
  `Progressive` scripted store renders placeholder columns exactly where
  buckets are absent; none after `Complete`.
- `now_playing::analysis_unavailable_label_when_failed_without_peaks`
  and `::partial_peaks_stay_when_failed_with_peaks`.
- `now_playing::empty_state_shows_pick_a_track`.
- `now_playing::artwork_falls_back_to_initials` (with
  `MODPLAYER_ARTWORK_FORCE_FAIL`).
- `waveform::detail_window_*` unit tests for every `DetailWindow`
  method (30 s initial, 200 ms floor, whole-track ceiling, page-forward
  follow, clamp at ends, recenter on outside seek, suspend on pan/zoom,
  reset).
- proptest `waveform::time_space_round_trips_within_one_pixel`.
- `fluent_keys`: every key in §1 present in `playback.ftl`.
