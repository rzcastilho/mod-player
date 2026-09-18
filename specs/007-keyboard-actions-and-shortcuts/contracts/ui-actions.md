# Contract: Key dispatch and the Settings › Controls shortcut map (`modplayer-ui`)

**Crate**: `modplayer-ui` (new `src/actions.rs`, new `src/settings/controls.rs`; modified `app.rs`, `shell.rs`, `now_playing.rs`, `markers.rs`, `waveform/{input,state}.rs`, `widgets/volume.rs`, `rows.rs`, `search_view.rs`, `settings/mod.rs`; locale `locales/en-US/controls.ftl`, `settings.ftl`) | **Traces**: FR-005, FR-006, FR-007, FR-008, FR-011, FR-012, FR-014, FR-015, FR-017, FR-018, FR-019; SC-001–SC-003, SC-005–SC-007, SC-009–SC-011; NFR-6.1, NFR-6.2, NFR-7.1, NFR-9.2 | **Design**: [research.md](../research.md) R3–R5, R8–R14, [data-model.md](../data-model.md) §4

## 1. Frame order (`App::ui`)

```
controller.tick()
scope   = ScopeState { now_playing_shown, marker_focused }          (R5)
invocs  = actions::dispatch(&ctx, &self.claims, controller.actions(), &scope)   ← only while launch_step == Main && device_check.is_none()
self.claims.clear()
for inv in invocs { actions::invoke(inv, &mut controller, &mut shell, &mut waveform, &ctx) }
… nav rail, notifications, central panel (widgets register claims for next frame) …
```

`dispatch` and `invoke` run before any widget and in the same frame as
the key event (FR-005, SC-011). `Shell::handle_shortcuts`,
`now_playing::handle_marker_shortcuts`, the `Cmd+Q` branch in
`now_playing::show` and the arrow arms of `waveform::focused_marker_key`
are removed; their behaviour is the catalog.

## 2. `actions::dispatch` — FR-019 precedence, one consumer per press

For each `Event::Key { key, physical_key, modifiers, pressed: true, repeat }` in
`ctx.input().events`, in order:

| Step | Condition | Effect |
|---|---|---|
| 1 | `ctx.text_edit_focused()` or `claims.claim_for(memory.focused()) == Some(TextLike)` | dispatcher returns `[]` for the whole frame; no event touched |
| 2 | focused id has `Claim::Keys(set)` and the event's (mods, key) ∈ `set`; or focused id has no claim and (mods, key) ∈ `TOOLKIT_DEFAULT_CLAIMS`; or the focused id has a claim and (mods, key) ∈ `{Tab, Shift+Tab}` | event left untouched (widget/toolkit consumes it) |
| 3a | `Mods::from_egui(modifiers, is_mac)` is `None` (macOS physical Control) | event left untouched |
| 3b | pass 1 chord (R3) resolves via `registry.resolve(chord, scope)` | consume event; if `repeat && !def.repeats_while_held` → dropped, else push `Invocation` |
| 3c | else pass 2 chord (physical key + full mods) resolves | same as 3b |
| 3d | pass 1 or pass 2 chord is *bound* but resolves to `None` (conflict, disabled, scope not live) | event left untouched (falls through to whatever the focused widget does, spec edge cases) |
| 4 | nothing matched | untouched |

Guarantees (tested):
- No key event is ever both consumed here and acted on by a widget in
  the same frame (SC-010): consumed events are removed from
  `ctx.input_mut().events` before any widget runs.
- A physical key-down of a `repeats_while_held: false` action produces
  exactly one `Invocation`, however many `repeat == true` events follow
  (US1 AS10); a `repeats_while_held: true` action produces one per
  event.
- Claims are last frame's; `FocusClaims::clear()` runs right after
  `dispatch`, and every widget with custom keys re-registers while it
  draws (`volume::master_volume`, `waveform::overview/detail`,
  `markers::lane` glyphs and panel rows, `rows::row`, 006's `DragValue`s
  (`TextLike`), settings search box, Controls filter box and capture
  control (`TextLike`)).

Claim sets (`ChordPattern` = (`Mods`, `KeyName`)):

| Constant | Registered by | Members |
|---|---|---|
| `TOOLKIT_DEFAULT_CLAIMS` | any focused widget without a claim | `Space`, `Enter`, `Tab`, `Shift+Tab`, `Escape`, `Left`, `Right`, `Up`, `Down` |
| `VOLUME_SLIDER_CLAIMS` | `widgets/volume.rs` | `Left`, `Right`, `PageUp`, `PageDown`, `Tab`, `Shift+Tab` |
| `WAVEFORM_CLAIMS` | `waveform/input.rs` (overview + detail) | `Left`, `Right`, `Shift+Left`, `Shift+Right`, `Alt+Left`, `Alt+Right`, `Alt+Shift+Left`, `Alt+Shift+Right`, `Home`, `End`, `Plus`, `Equals`, `Minus`, `0`, `Escape`, `Tab`, `Shift+Tab` |
| `MARKER_CLAIMS` | `markers.rs` glyphs and panel rows | `Delete`, `Backspace`, `F2`, `Enter`, `C`, `Escape`, `Tab`, `Shift+Tab` — **not** arrows (the nudge actions own them) |
| `ROW_CLAIMS` | `rows.rs` (library, search, queue rows) | `Enter`, `Space`, `Shift+F10`, `Escape`, `Tab`, `Shift+Tab`, `Up`, `Down`, `Left`, `Right` |

The marker glyph/row additionally sets
`EventFilter { horizontal_arrows: true, ..Default::default() }` while
focused so egui's traversal does not also move focus on a nudge arrow.

## 3. `actions::invoke` — owner mapping (FR-005)

| `HostAction` | Call | Refusal / note |
|---|---|---|
| `Play` / `Pause` / `Stop` | `controller.play()` / `pause()` / `stop()` | no-op while `!transport_enabled()` |
| `TogglePlayPause` | `pause()` if `transport_state().intent == Playing` else `play()` | same rule the Now Playing button uses |
| `NextTrack` / `PreviousTrack` | `skip_forward()` / `skip_back()` | |
| `SeekForwardStep` / `SeekBackwardStep` | `seek_step(+1)` / `seek_step(-1)` | |
| `VolumeUp` / `VolumeDown` | `step_master_volume(+1)` / `(-1)` | |
| `AddPointMarker`, `SetA`, `SetB`, `ToggleLoop`, `SetCue(n)` | 006's calls; `waveform.marker_status = err.map(markers::refusal_key)` | identical to the removed `handle_marker_shortcuts` arms |
| `JumpToCue(n)` | `let _ = controller.jump_to_cue(n); waveform.marker_status = None` | silent no-op on empty slot |
| `NudgeEarlier`/`NudgeLater` (`×10`) | `controller.nudge_marker(id, ∓1, 1 or 10)` for `id = waveform.focused_marker` | scope guarantees `Some`; `None` → no-op |
| `ClearAllMarkers` | `controller.clear_all_markers()` | unbound by default; when bound, clears without the panel's two-step confirm (a deliberate keyboard shortcut) |
| `NavLibrary` … `NavSettings` | `shell.section = …` | |
| `ToggleQueue` | `now_playing::toggle_queue_panel(ctx)` | flips the existing egui temp-memory flag |
| `FocusSearch` | `shell.section = Search; shell.focus_search_requested = true` | |
| `TempoStepUp` / `TempoStepDown` | no-op | disabled; unreachable through `dispatch` today |

## 4. Settings › Controls (`settings/controls.rs`) — FR-006/FR-007/FR-011/FR-012/FR-014

Layout, top to bottom, all keyboard-reachable in this `Tab` order:

| Element | Accessible name (Fluent key) | Behaviour |
|---|---|---|
| Filter `TextEdit` | `controls-filter` ("Filter actions") | case-insensitive substring over `tr(label_key)` and `tr(category.label_key())`; registered `TextLike` |
| "Reset all to defaults" | `controls-reset-all` | → inline `controls-reset-all-confirm` ("Reset all bindings?") + `controls-confirm` / `controls-cancel`; `Esc` or focus leaving both buttons cancels; Confirm → `controller.reset_all_bindings()` |
| Category header (only if ≥ 1 row matches) | `action-cat-*` | heading |
| Row label | `action-*` + `controls-kind-trigger` ("trigger") + `controls-inactive` (" (inactive)") when disabled | disabled rows use `weak_text_color`; every control stays enabled |
| Binding chip (per chord) | `controls-binding-chip { $binding }` ("Binding {binding}") — text is `Chord::display(platform)` | non-interactive label; conflicting chips get `⚠` + `controls-conflict-with { $other }` ("Conflicts with {other}") |
| Chip remove button | `controls-remove-binding { $binding }` ("Remove binding {binding}") | `controller.remove_binding(a, c)` immediately (FR-008) |
| "Add binding" | `controls-add-binding` | enters capture (§5) |
| "Reset to default" | `controls-reset-action { $action }` | `controller.reset_binding(a)` immediately, no confirm |
| No-match line | `controls-no-match` ("No actions match") | only when the filter hides every row |

Platform: `platform = if ctx.os().is_mac() { Platform::Mac } else { Platform::Other }`.

The settings registry gains `SettingDescriptor { category: Controls, id: "controls.keybindings", title_key: "setting-keybindings", description_key: "setting-keybindings-desc" }`;
`settings_registry::tests::placeholder_only_categories_contribute_no_descriptors` drops `Controls` from its list; a search hit focuses the filter box.

## 5. Capture mode (`ControlsScreen::capture`) — FR-007

- Activating "Add binding" on row *a*: `capture = Some(a)`, the row's
  "Add binding" button is replaced by a focused capture control
  (`Button` labelled `controls-capture-prompt` "Press a key combination…",
  accessible name `controls-capture { $action }`), registered
  `TextLike`, with `set_focus_lock_filter(id, EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })`
  and `request_focus()` on the frame it appears.
- Each frame while `capture == Some(a)`, in order:
  1. `!response.has_focus()` (after the first frame) → `capture = None` (focus lost: click elsewhere, window blur).
  2. First `Event::Key { pressed: true, .. }`:
     - `Escape` (any mods) → `capture = None`.
     - `Tab`/`Shift+Tab` → `capture_error = controls-reject-tab`.
     - `is_mac && modifiers.ctrl` → `controls-reject-mac-control` ("Use ⌘ instead").
     - lone modifier: egui-winit emits no `Event::Key` for a bare `Shift`/`Ctrl`/`Alt`/`⌘` press (egui's `Key` has no modifier variants), so such a press yields no candidate and capture simply stays open — the spec's "rejected inline" is rendered as the standing `controls-capture-prompt` plus `controls-reject-modifier-only` shown whenever `ctx.input().modifiers.any()` is true with no key event this frame; `CaptureRule::LoneModifier` covers the synthetic case of a candidate with modifiers and no key and is unit-tested.
     - canonical chord (R3 capture rule) ∈ `registry.bindings(a)` → `controls-reject-duplicate`.
     - otherwise → `controller.add_binding(a, chord)`, `capture = None`, `capture_error = None`; if the chord is now conflicting the row shows `controls-conflict-with { $other }` on that chip immediately.
  3. Any rejection leaves capture active and shows `capture_error`
     inline next to the control.
- No timeout.

## 6. Fluent keys (`locales/en-US/controls.ftl`, new; `settings.ftl` delta)

- 44 labels `action-transport-play`, `action-transport-pause`, `action-transport-toggle`, `action-transport-stop`, `action-transport-next`, `action-transport-previous`, `action-transport-seek-forward-step`, `action-transport-seek-backward-step`, `action-transport-volume-up`, `action-transport-volume-down`, `action-markers-add-point`, `action-markers-set-a`, `action-markers-set-b`, `action-markers-nudge-earlier`, `action-markers-nudge-later`, `action-markers-nudge-earlier-x10`, `action-markers-nudge-later-x10`, `action-markers-clear-all`, `action-loop-toggle`, `action-cues-set-1` … `-8`, `action-cues-jump-1` … `-8`, `action-nav-library`, `action-nav-search`, `action-nav-now-playing`, `action-nav-plugins`, `action-nav-settings`, `action-nav-toggle-queue`, `action-nav-focus-search`, `action-effects-tempo-step-up`, `action-effects-tempo-step-down`.
- 6 categories `action-cat-transport`, `action-cat-markers`, `action-cat-loop`, `action-cat-cues`, `action-cat-navigation`, `action-cat-effects`.
- Page: `controls-heading`, `controls-filter`, `controls-no-match`, `controls-kind-trigger`, `controls-kind-continuous`, `controls-inactive`, `controls-binding-chip`, `controls-remove-binding`, `controls-add-binding`, `controls-capture`, `controls-capture-prompt`, `controls-reset-action`, `controls-reset-all`, `controls-reset-all-confirm`, `controls-confirm`, `controls-cancel`, `controls-conflict-with`, `controls-reject-tab`, `controls-reject-mac-control`, `controls-reject-modifier-only`, `controls-reject-duplicate`.
- Warning: `keybindings-invalid-entries` (`{ $ids }`).
- `settings.ftl`: `setting-keybindings`, `setting-keybindings-desc`.

`fluent_keys.rs` extends its unused-key audit to `controls.ftl` and
resolves every key above.

## 7. Tests pinning this contract (`crates/modplayer-ui/tests/actions.rs`, `tests/controls.rs`, extended `tests/markers.rs`, `tests/now_playing.rs`, `tests/accessibility.rs`, `tests/fluent_keys.rs`, `shell.rs` unit tests)

| Requirement | Test |
|---|---|
| FR-017 inherited set unchanged | `actions.rs::inherited_bindings_produce_identical_controller_calls` — table over `Primary+1..5`, `Primary+F`, `Slash`, `I`, `O`, `L`, `M`, `1`–`8`, `Shift+1`–`Shift+8` (logical `Exclamationmark`-shape and physical-fallback shape both), marker-focus `Left`/`Right`/`Shift+Left`/`Shift+Right`; existing `markers.rs::i_then_o_creates_region_at_playhead_positions`, `::shift_digit_sets_cue_and_digit_jumps_keeping_state`, `::glyph_focus_arrow_nudges_by_setting_and_shift_ten_x`, `now_playing.rs` key tests and `shell.rs::*shortcut*` tests re-pointed at `dispatch`/`invoke` |
| FR-017 sole deviation | `actions.rs::slash_in_focused_text_field_types_and_does_not_focus_search` |
| US1 AS1–AS6, AS8, SC-009 | `actions.rs::transport_shortcuts_from_now_playing_and_library` (each transport chord from both sections) |
| US1 AS7 / SC-002 | `actions.rs::i_o_l_arms_loop_with_no_pointer` |
| US1 AS9 / FR-019 / SC-010 | `actions.rs::focused_stop_button_wins_space_exactly_one_effect`, `::focused_waveform_lets_space_toggle`, `::focused_volume_slider_keeps_arrows` |
| US1 AS10 / FR-019 repeat | `actions.rs::held_space_toggles_once_held_volume_repeats` |
| US1 AS11 | `actions.rs::q_toggles_queue_panel_in_now_playing_only` |
| FR-018 scopes | `actions.rs::i_outside_now_playing_falls_through`, `::nudge_requires_focused_marker` |
| FR-005 / SC-011 same frame | `actions.rs::invocation_lands_in_the_same_frame_as_the_key_event` |
| US2 AS1, AS8a / SC-006 | `controls.rs::filter_matches_label_and_category_case_insensitively`, `::filter_no_match_shows_line` |
| US2 AS2–AS4b / FR-007 | `controls.rs::capture_accepts_chord_and_adds_chip`, `::capture_esc_and_focus_loss_cancel`, `::capture_rejects_duplicate_tab_and_mac_control_inline`, `::capture_accepts_space_enter_arrows_function_keys`, `::capture_of_conflicting_chord_flags_both_and_names_partner` |
| US2 AS5–AS7 / FR-008 / FR-011 | `controls.rs::remove_chip_immediately_allows_zero_bindings`, `::reset_action_restores_default_without_confirm`, `::reset_all_two_step_confirm_cancel_esc_focus_loss` |
| US2 AS8 / FR-012 / SC-005 | `controls.rs::disabled_rows_greyed_rebindable_and_never_fire` |
| US3 AS1–AS6 / SC-003 | `actions.rs::conflicting_chord_fires_neither_until_resolved`, `controls.rs::conflict_flag_shows_on_both_rows`, `actions.rs::disabled_tempo_binding_does_not_block_and_flags_on_enable` |
| FR-014 / NFR-6.1 | `accessibility.rs` enumeration extended to every Controls control (name, role, state) and `Tab` order |
| FR-015 / NFR-7.1 | `fluent_keys.rs` extended (all keys resolve; none unused) |
| Platform display / NFR-9.2 | `controls.rs::chips_display_platform_glyphs` |
| R2 bijection | `actions.rs::key_name_table_matches_egui_key_all` |
