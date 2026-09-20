# Contract: Markers, loops and cues UI (extends 005 contracts/ui-waveform.md)

**Crate**: `modplayer-ui` (`now_playing.rs`, new `markers.rs`
(panel + overlay + lanes), `waveform/{mod,paint,state,input}.rs`,
`theme.rs`, `settings/playback.rs`). Locale: en-US,
`locales/en-US/playback.ftl` + `settings.ftl`. Every control is
keyboard-operable with an accessible name, role and state (FR-022;
`accessibility` test enumerates them). Implements FR-003, FR-004,
FR-004a, FR-005, FR-020–FR-023, FR-026; research R13–R17;
data-model.md §3.

## 1. Now Playing layout delta (top to bottom)

| Element | Accessible name key | Behaviour |
|---|---|---|
| Elapsed label, **marker lane (overview)**, waveform overview, remaining label | lane glyphs: `marker-glyph` (`{ $role } { $name } { $time }`) | 14 px lane sharing the overview `TimeSpace`; glyphs per §3 |
| **Marker lane (detail)**, waveform detail | same | lane sharing the detail `TimeSpace`; only markers inside the window get glyphs |
| **Markers panel** (new, between detail and master volume) | `markers-panel` | §4; empty state `markers-empty` = "No markers — press I to set A" (FR-020) |
| Master volume, peak meter, queue panel | unchanged | |

Overlays inside both waveforms (through the new `overlays` hook, painted
after peaks, before the playhead — FR-026): a 1 px vertical line per
marker in its palette colour; the current region's `[A, B)` span
(outline / hatched / solid per data-model.md §3); a small warning
triangle at the top of a `clamped` marker's line. Colours: `theme::MARKER_PALETTE`
+ existing visuals only.

## 2. View-level shortcuts (Now Playing shown, no text field focused — FR-004a)

| Key | Action | Refusal (inline, `markers-status` label under the panel header, cleared on the next successful action) |
|---|---|---|
| `M` | `add_point_marker()` | `marker-limit-reached` |
| `I` / `O` | `set_loop_a()` / `set_loop_b()` | `marker-limit-reached` |
| `L` | `toggle_current_loop()` | `loop-region-incomplete` / `loop-region-too-short` |
| `1`–`8` | `jump_to_cue(n)`; silent no-op on an empty slot | — |
| `Shift+1`–`Shift+8` | `set_cue(n)` | `marker-limit-reached` (only when creating) |

Physical `Key::Num1..Num8` and `Key::M/I/O/L`, no modifier (or `Shift`
for cue set); `Ctrl/Cmd+n` keeps 001's section shortcuts because the
shell consumes them first. The guard: none of the view's text fields
(inline rename, repeat, crossfade) has egui focus (research R17).

## 3. Marker glyphs and focus (FR-004, FR-005)

| Glyph | Shape | Role / name |
|---|---|---|
| point | 10 px downward triangle | `Role::Button`, `marker-glyph { role = "Marker", name, time }` |
| region A / B | `[` / `]` bracket | `marker-glyph { role = "A" / "B", … }` |
| cue n | 10 px square with the digit | `marker-glyph { role = "Cue n", … }` |

Selecting (click or `Tab`) a glyph or its panel row gives it egui focus
and sets `WaveformState::focused_marker`, calls `select_marker(id)`
(current region), and highlights the line (2 px) and the row. While a
glyph or row has focus:

| Key | Action |
|---|---|
| `←` / `→` | `nudge_marker(id, ∓1, 1)` |
| `Shift+←` / `Shift+→` | `nudge_marker(id, ∓1, 10)` |
| `Delete` / `Backspace` | `delete_marker(id)`; focus returns to the detail waveform |
| `F2` / `Enter` | open inline rename (§4 row field gets focus; `Enter` commits, `Esc` cancels) |
| `C` | `cycle_marker_color(id)` |
| `Esc` | (not dragging, not renaming) focus → detail waveform |
| `Tab` / `Shift+Tab` | next/previous glyph or row in visual order, then 005's waveform stops |

**Drag** (either lane; press + move beyond egui's drag threshold):
relative-delta in detail space (research R14); the glyph, the line, the
panel row's time and the detail window follow `live` every frame;
release → `move_marker(id, live)`; `Esc` → no move, detail window
restored. The detail window keeps its zoomed state after a commit with
follow suspended (005 §5). Pointer drags on the waveform rect itself
remain 005 seeks — glyph drags never seek.

## 4. Markers panel (FR-021)

Header row: `markers-heading` ("Markers"), buttons `markers-new-loop`
("New loop region") and `markers-clear-all` ("Clear all markers"); the
clear button turns into the two-step inline confirmation
`markers-clear-confirm { $count }` ("Clear N markers?") with
`markers-clear-yes` / `markers-clear-no` buttons (keyboard-operable,
`Esc` cancels); confirming calls `clear_all_markers()`.

One row per marker, sorted by position (`markers-row` container,
`Role::ListItem`):

| Cell | Widget | Accessible |
|---|---|---|
| colour swatch | 8-swatch popup (`markers-color { $index }`), keyboard `C` cycles | `Role::Button` |
| role/kind | `marker-role-a` / `-b` / `-cue { $slot }` / `-point` | label |
| name | `TextEdit` inline when renaming, else label; empty region/cue names show only the role | `markers-rename` |
| position | `m:ss.mmm` (`format_mmss_millis_frames`) | label (live during drag) |
| warning glyph | when `clamped` | `marker-clamped-desc` |
| loop cells (A row only, spanning the region) | arm toggle `loop-arm` / `loop-disarm` (disabled with reason tooltip `loop-region-incomplete` / `loop-region-too-short`); repeat `DragValue` `loop-repeat` (blank/0 = infinite, 1–1000); crossfade `DragValue` `loop-crossfade` (0–50 ms); `loop-wraps-remaining { $count }` / `loop-wraps-infinite` while armed; `loop-armed-inactive` badge when `loop_state == 1` | `Role::CheckBox` for arm; `SpinButton`/`Slider` for the numeric fields |

Empty state: the list is replaced by `markers-empty`.

## 5. `waveform` module delta

```rust
pub fn overview(ui, len_frames, sample_rate, playhead, previewing, enabled, paint_data,
                overlays: &mut dyn FnMut(&Painter, &TimeSpace)) -> (WaveformResponse, Option<WaveformEvent>)
pub fn detail(ui, window, sample_rate, playhead, previewing, enabled, paint_data,
              overlays: &mut dyn FnMut(&Painter, &TimeSpace)) -> (WaveformResponse, Option<WaveformEvent>)
// paint.rs: paint() no longer draws the playhead; new paint::playhead(painter, space, playhead, visuals)
// state.rs: DetailWindow::zoom_assist(&self, live: u64, target_width: u64, len, rate) -> Self   // width ×0.8 per call toward target, recentred on `live`
//           WaveformState gains marker_drag / focused_marker / rename / clear_confirm / text_field_ids (data-model.md §3)
```

`zoom_assist` target: `max(rect_width_px × rate × 0.0025, DETAIL_MIN_WINDOW_MS × rate / 1000)`
frames (≤ 2.5 ms per pixel → SC-003's 5 ms with margin).

## 6. Theme delta

`theme::MARKER_PALETTE: [Color32; 8]` — the fixed accent palette
(research R13); `theme::marker_color(PaletteIndex) -> Color32`.

## 7. Settings › Playback delta

`setting-nudge-step` ("Marker nudge step") + `setting-nudge-step-desc`
("How far ← / → move a focused marker, in milliseconds (Shift moves 10×).")
as a `DragValue` 1–1000 ms; committed on change via
`controller.set_nudge_step_ms`.

## 8. Fluent keys added (`playback.ftl` unless noted)

`markers-panel`, `markers-heading`, `markers-empty`, `markers-new-loop`,
`markers-clear-all`, `markers-clear-confirm`, `markers-clear-yes`,
`markers-clear-no`, `markers-status`, `marker-limit-reached`,
`loop-region-incomplete`, `loop-region-too-short`, `marker-glyph`,
`marker-role-a`, `marker-role-b`, `marker-role-cue`, `marker-role-point`,
`marker-default-name` (`Marker { $n }` — core default name is produced
via `tr_args` so it is externalized), `markers-rename`, `markers-color`,
`marker-clamped-desc`, `loop-arm`, `loop-disarm`, `loop-repeat`,
`loop-crossfade`, `loop-wraps-remaining`, `loop-wraps-infinite`,
`loop-armed-inactive`, `track-state-unreadable`,
`track-state-newer-version`, `track-state-save-failed`;
`settings.ftl`: `setting-nudge-step`, `setting-nudge-step-desc`.

## 9. Tests pinning this contract (`crates/modplayer-ui/tests/markers.rs` unless noted)

- `i_then_o_creates_region_at_playhead_positions` (US1 AS1)
- `l_toggles_current_region_and_refuses_with_reason` (AS7, FR-008)
- `m_creates_point_marker_with_default_name_sorted` (US3 AS8)
- `shift_digit_sets_cue_and_digit_jumps_keeping_state` (US4 AS1–AS3, `seek_frames` observed; paused stays paused)
- `digit_on_empty_slot_is_noop` (AS5)
- `sixty_fifth_marker_refused_inline` (SC-005) / `shift_digit_on_occupied_slot_moves_at_limit` (AS6)
- `shortcuts_inactive_while_rename_open` (FR-004a)
- `glyph_focus_arrow_nudges_by_setting_and_shift_ten_x` (SC-004; 25 ms setting)
- `delete_focused_endpoint_makes_region_incomplete` (AS5)
- `f2_rename_commits_on_enter_cancels_on_esc` (AS1)
- `c_cycles_palette_and_row_matches_glyph` (AS2)
- `drag_from_overview_zooms_detail_and_lands_within_5ms` (SC-003: synthetic pointer path, assert `|committed − intended| ≤ 5 ms`) / `drag_esc_restores_position_and_window`
- `overlay_paints_lines_and_span_states` (paint-callback capture: line per marker, span style by `loop_state`)
- `clamped_marker_shows_warning_glyph` (SC-012)
- `empty_state_shows_press_i_hint` (FR-020) / `clear_all_two_step_confirm_and_cancel` (US2 AS6)
- `armed_region_shows_wraps_remaining` / `armed_inactive_badge_when_state_1`
- `accessibility`: every glyph = `Role::Button` named `marker-glyph …`; rows `ListItem`; arm toggle `CheckBox` with state; numeric fields labelled; panel/empty state exposed; every key in §2/§3 reachable
- `fluent_keys`: every key in §8 present and none unused
- `now_playing::keyboard_table_matches_pointer_results` extended with the §3 rows
- `settings::playback::nudge_step_drag_value_commits` (`settings/playback.rs` unit test)
