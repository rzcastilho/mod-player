# Contract: Effect Chain panel and meters (UI)

**Crate**: `modplayer-ui` (`src/effects_view.rs` new, `src/widgets/
chain_meters.rs` new, `src/now_playing.rs`, `src/actions.rs`, `src/app.rs`
(claims), `tests/effects_view.rs` new, `tests/{accessibility,fluent_keys,
actions,now_playing}.rs` extended), `locales/en-US/effects.ftl` new,
`locales/en-US/controls.ftl` (+1 action label). Implements FR-003,
FR-004, FR-008 (note), FR-011 (display), FR-012 (labels), FR-015, FR-017
(dispatch); research R14, R15. Follows 007 contracts/ui-actions.md for
dispatch and focus claims and 006 contracts/ui-markers.md for panel
conventions.

## 1. Placement and toggle (FR-003)

- The Now Playing transport row gains a `selectable_label(effects_open,
  tr("effects-toggle"))` beside "Queue". Open state lives in egui temp
  memory under `now-playing-effect-chain-open` (`effects_view::
  panel_open_id()`), read/written exactly like `queue_panel_open_id`, so
  it survives track changes and is dropped with the process.
- `effects_view::toggle_effect_chain_panel(ctx)` flips the flag; it is
  what `invoke(HostAction::ToggleEffectChain)` calls.
- When open, `effects_view::show(ui, controller, state)` is drawn after
  the waveform/markers block and before the Queue panel (both may be open
  at once; a separator between them).
- Rendered regardless of whether a track is loaded (an empty chain with
  0 % figures is valid — spec edge case).

## 2. Panel layout

```
[Effect chain]                       chain: 12 %   overloads: 0   [⚠ over budget]?
 Pre   ▮▮▮▮▮▯▯▯  peak −6.1 dB  rms −14.2 dB | Post  ▮▮▮▮▮▮▯▯  peak −3.0 dB  rms −11.8 dB
 ▁▂▃▅▆▇▅▃▂▁▁▂▃▄▃▂▁ … (64 bars)
 ┌ ⋮  1. Time stretch · host        [bypass ○]   4 %   ratio [ 60 % ]  mode [quality ▾]  (auto-switched to quality)   [remove] ┐
 ┌ ⋮  2. Equalizer · host           [bypass ●]   3 %   band 1 [63 Hz][0 dB][1.0][peak ▾] … band 8 …                     [remove] ┐
 [Add node… ▾]  (Chain is full — remove a node first)?
```

| Element | Widget | Accessible name / role / state |
|---|---|---|
| chain figure | label | "Chain CPU load: 12 %" |
| overload counter | label | "Overloads: N" |
| over-budget badge | label, only while `over_budget` | "Effect chain over budget" |
| pre/post meters | `chain_meters::level_pair` (reuses `peak_meter` scale, two bars: peak with hold, RMS) | "Pre-chain level: peak −6.1 dB, RMS −14.2 dB" / "Post-chain …" |
| spectrum | `chain_meters::spectrum` (64 bars, log-frequency axis labelled 20 Hz/1 kHz/20 kHz) | "Spectrum, 64 bands" (value: "peak band 1.2 kHz") |
| row | `ui.horizontal` inside `dnd_drop_zone` | group "Node 1 of 2: Time stretch, owner host" |
| drag handle | focusable `Button` "⋮" as `dnd_drag_source` | "Reorder Time stretch; Up and Down arrows move it" |
| bypass | `toggle_value` | "Bypass Time stretch", state on/off; when `auto_bypassed`: label "Bypass Time stretch (auto-bypassed, over budget)" |
| cost | label | "CPU 4 %" |
| params | per kind (§3) | each control's `WidgetInfo` carries the param label and unit |
| mode note | small label, only while `mode_note` | "Quality mode auto-switched" |
| remove | `Button` | "Remove Time stretch" |
| add | `ComboBox` of the six kinds + "Add" `Button` | "Add node" |
| inline refusal | label under the add row | text of `effects-chain-full` |

Order = processing order; Tab moves row by row, then control by control
inside a row.

## 3. Parameter controls (catalog-driven; FR-006, FR-010)

| Kind | Controls |
|---|---|
| Pitch shift | `Slider` semitones −12..+12 step 0.01 (suffix " st"); `toggle_value` formant; `ComboBox` mode |
| Time stretch | `Slider` shown in percent 25..200 (`ratio × 100`, suffix " %"), writes `ratio = pct / 100`; `ComboBox` mode |
| Gain | `Slider` dB −60..+12; `toggle_value` mute |
| Equalizer | 8 rows: `DragValue` Hz (logarithmic speed), `DragValue` dB, `DragValue` Q, `ComboBox` type |
| Filter | `ComboBox` mode; `DragValue` cutoff Hz; `Slider` resonance 0..1 |
| Stereo tools | `Slider` width 0..2; `Slider` balance −1..+1; `toggle_value` mono sum; `toggle_value` phase invert **`add_enabled(mono_sum)`**; `toggle_value` swap |

Rules:

- Every change goes through `controller.chain_set_param` and the control
  is immediately redrawn with the **returned** value (SC-005; a typed
  30 000 Hz shows 19 845 Hz on a 44.1 kHz track).
- Mode combos call `chain_set_mode` (clears the auto-switched flag —
  FR-008 rule 3).
- `DragValue`s register `TextLike` claims while focused (007 FR-019) so
  typing digits never triggers cue jumps.

## 4. Reorder (FR-004, FR-005)

- Pointer: egui `dnd_drag_source(handle_id, payload = NodeId)` on the
  handle and `dnd_drop_zone` per row; on drop, `controller.chain_move_node
  (id, target_index)`.
- Keyboard: the handle registers `EFFECT_HANDLE_CLAIMS = {ArrowUp,
  ArrowDown}` with `FocusClaims` while focused; `effects_view::
  handle_focused_handle_keys` consumes `↑`/`↓` → `chain_move_node_by(id,
  ∓1)` and keeps focus on the same node's handle after the move (row ids
  are keyed by `NodeId`, not index).
- Add appends; the new row's handle is *not* auto-focused (focus stays on
  the Add button so repeated adds are keyboard-fluent).

## 5. Actions (FR-017; 007 contracts/ui-actions.md §3)

| Action | `invoke` |
|---|---|
| `ToggleEffectChain` | `effects_view::toggle_effect_chain_panel(ctx)` |
| `TempoStepUp` | `controller.tempo_step(1)` |
| `TempoStepDown` | `controller.tempo_step(-1)` |

Precedence unchanged: a focused waveform's `WAVEFORM_CLAIMS` already
includes `Plus`/`Minus`/`Equals` for zoom, so with waveform focus the
keys zoom and the registered action is never consulted (007 FR-019 step
2 before 3). `E` has no claim anywhere in 001–007.

## 6. Fluent keys (`locales/en-US/effects.ftl`; `controls.ftl` +1)

`effects-toggle`, `effects-panel-title`, `effects-chain-cpu`,
`effects-overloads`, `effects-over-budget-badge`, `effects-pre`,
`effects-post`, `effects-peak`, `effects-rms`, `effects-spectrum`,
`effects-add-node`, `effects-add`, `effects-remove`, `effects-bypass`,
`effects-auto-bypassed`, `effects-reorder-handle`, `effects-cpu`,
`effects-mode-note`, `effects-chain-full`, `effects-owner-host`,
`effects-owner-plugin`, `effects-kind-{pitch-shift,time-stretch,gain,
equalizer,filter,stereo-tools}`, `effects-param-{semitones,formant,mode,
ratio,level,mute,band-freq,band-gain,band-q,band-type,filter-mode,cutoff,
resonance,width,balance,mono-sum,phase-invert,channel-swap}`,
`effects-mode-{performance,quality}`, `effects-band-type-{peak,low-shelf,
high-shelf}`, `effects-filter-{high-pass,low-pass}`,
`effect-chain-over-budget` (`{ $node }`, `{ $owner }`),
`effect-chain-auto-bypassed` (`{ $node }`), `effects-no-time-stretch`;
`controls.ftl`: `action-nav-toggle-effect-chain`. All resolved through
`tr`/`tr_args`; `tests/fluent_keys.rs` asserts no unused key.

## 7. Repaint

While the panel is open and `transport == Playing`, request a repaint
every 16 ms (meters, costs) — the Now Playing screen already does this
while playing, so no new timer.

## 8. Tests pinning this contract (`modplayer-ui tests/effects_view.rs` unless noted)

| Test | Pins |
|---|---|
| `e_and_header_toggle_panel_and_it_survives_track_change` | FR-003, US2 AS1a, SC-012 |
| `rows_list_nodes_in_processing_order_with_type_owner_bypass_handle_cost` | FR-003, US2 AS1 |
| `add_each_kind_appends_at_defaults_and_shows_zero_cost` | US2 AS1, US4 AS2 |
| `seventeenth_add_is_refused_inline` | US2 AS5, SC-007 |
| `drag_drop_reorders_and_calls_move_to` | FR-004, US2 AS2 |
| `arrow_up_on_focused_handle_moves_node_and_keeps_focus` | US2 AS1b |
| `bypass_toggle_updates_state_immediately` | US2 AS3 |
| `remove_row_disappears_order_preserved` | US2 AS4 |
| `out_of_range_entry_displays_clamped_value` (gain +20 → +12, EQ 30 000 Hz → 19 845 Hz, resonance 1.5 → 1.0) | US3 AS1, AS4, AS4b, SC-005 |
| `phase_invert_disabled_unless_mono_sum` | US3 AS6 |
| `quality_note_appears_at_25_percent_and_clears_at_100` | US1 AS4, SC-006 |
| `plus_minus_step_tempo_unless_waveform_focused` | US1 AS7, SC-012 |
| `plus_without_time_stretch_notifies_once_while_held` | US1 AS8, SC-012 |
| `auto_bypassed_label_shown_from_view_flag` | US4 AS5 (view-level) |
| `over_budget_badge_and_counter_follow_view` | US4 AS4 |
| `meters_and_spectrum_render_from_snapshot` | FR-011, SC-008 |
| `tests/accessibility.rs` enumeration extended to every panel control | FR-015 |
| `tests/fluent_keys.rs` extended | NFR-7.1 |
| `tests/actions.rs::toggle_effect_chain_dispatches_in_now_playing_only`, `::tempo_actions_enabled_and_repeat` | FR-017 |
