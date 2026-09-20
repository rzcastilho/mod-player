# Key & Tempo plugin package (`org.modplayer.key-tempo`)

The behavioural contract of the second bundled plugin, written against
[plugin-api-v1.4.md](plugin-api-v1.4.md) and the 009/011 runtime rules.
Rule ids: P (package), G (registration), A (actions/interactions), E
(event mirroring), M (per-track memory), L (lifecycle). Named tests in
§7. Spec traceability: FR-001–FR-017 in [spec.md](../spec.md).

## 1. Package (P)

- **P1** Files: `plugin.toml`, `main.luau`, `README.md`, `LICENSE-MIT`,
  `LICENSE-APACHE` under `plugins/bundled/org.modplayer.key-tempo/`;
  the two licence files are byte-identical to the repository root's.
  (FR-001)
- **P2** Manifest: identifier `org.modplayer.key-tempo`, `api = "1.4"`,
  `license = "MIT OR Apache-2.0"`, `source = "bundled"`, `default_locale
  = "en-US"`, exactly five **required** permissions — `audio.effects`,
  `ui.panel`, `ui.shortcuts`, `state.track`, `playback.observe` — each
  with a non-empty justification; no optional permission; no `icon`/
  `[glyphs]`; a `[strings.en-US]` table holding every key data-model §3.6
  lists. (FR-002, FR-017)
- **P3** `main.luau` references only `api.<namespace>.<method>` names
  that exist in `v1.toml` (`SC-008` scan) and never a host crate or
  private symbol; no `api.debug_probe`. (FR-001, SC-008)
- **P4** Embedded via `bundled::packages()` as the second element;
  discovered, granted and enabled by default like Section Loop (009
  FR-024, FR-013). (FR-019)

## 2. Registration on `ready_ack` (G)

- **G1** Order: register the 8 actions (§3.5 of data-model), register
  panel `main` (widgets in data-model §3.4 order), then `list_chain()`.
- **G2** Node adoption/creation (FR-005, research R14):
  - both own `pitch_shift` + `time_stretch` present → adopt; **no
    `set_param`**; mirror widgets from `params` (E1); `remember` ←
    `state.track.get("settings") ~= nil` (or `false` with no track);
    `restored` ← `""`.
  - exactly one present → `create_node` for the missing kind with
    `{ before = "time_stretch" }` (missing pitch) or `{ after =
    "pitch_shift" }` (missing stretch); then as above.
  - neither present → `create_node("pitch_shift")` then
    `create_node("time_stretch", { after = "pitch_shift" })`; then run
    E2's `track_changed` logic with `api.playback.state().track`
    (`nil` ≡ no entry).
- **G3** Both nodes are owned by the plugin and adjacent (pitch before
  stretch) after G2 in every case that creates them; the Effect Chain
  panel lists them under the plugin's name with no further call.
- **G4** `keep_across` is `false` after every `ready()`; `step` is 10;
  `step_note` is `""`.
- **G5** Registration refusals are logged (`api.log.error`) and never
  retried in a loop; a refused panel leaves the plugin Active with its
  actions (011 rule, as Section Loop).

## 3. Actions and panel interactions (A)

Every action handler and its panel button call the same function; the
six buttons are plain (`action` field absent — research R6) so they stay
operable while `tempo_up`/`tempo_down`'s `Plus`/`Minus` chords are
flagged as conflicting with the host's on a fresh install.

- **A1** `key_step(±1)` (`key_up`/`key_down`, buttons): `S.key = clamp(S.key ± 1, -12, 12)`;
  `cents` untouched; send composed (`A7`). (FR-004, FR-006)
- **A2** `key` slider commit `v`: `S.key = v`; send composed.
  `cents` slider commit `v`: `S.cents = v`; send composed. (FR-006)
- **A3** `tempo` slider commit `v`: `set_param(S.stretch, "ratio", v / 100)`. (FR-007)
- **A4** `tempo_step(±S.step)` (`tempo_up`/`tempo_down`, buttons):
  `set_param(S.stretch, "ratio", clamp(S.tempo ± S.step, 25, 200) / 100)`.
  At a bound the call is still made with the clamped value (a no-op
  host-side) and nothing is refused. (FR-004; Edge Cases)
- **A5** `step` slider commit `v`: `S.step = v`; `update_widget(main,
  step_note, v == 10 and "" or "@step_note_text")`. (FR-013)
- **A6** `formant` toggle `v`: `set_param(S.pitch, "formant", v)`.
  `quality` toggle `v`: `set_param(S.pitch, "quality_mode", name)` **and**
  `set_param(S.stretch, "quality_mode", name)`, `name = v and "quality"
  or "performance"`. The plugin never calls `set_param` for a mode it
  merely observed (E1). (FR-008, FR-009)
- **A7** Composed send: `set_param(S.pitch, "semitones", S.key + S.cents / 100)`;
  `S.sent = { semitones = composed, key = S.key, cents = S.cents }` on
  success. (FR-006)
- **A8** `reset_key`: `S.key, S.cents = 0, 0`; send composed.
  `reset_tempo`: `set_param(S.stretch, "ratio", 1.0)`. (FR-010)
- **A9** `toggle_remember(v)` (action flips `not S.remember`; toggle
  passes its value):
  - `v == true` → `state.track.set("settings", current_table())`;
    on success `S.remember = true`, `S.last_written = table`; on
    refusal → `update_widget(main, remember, false)`,
    `update_widget(main, restored, err.message)`, nothing stored.
  - `v == false` → `state.track.remove("settings")`; `S.remember =
    false`; `S.last_written = nil`; badge unchanged (it describes how the
    current values were seeded). (FR-011; Edge Cases)
- **A10** `toggle_keep_across_tracks`: `S.keep_across = not S.keep_across`
  (toggle passes its value); mirror the widget; nothing persisted. (FR-011)
- **A11** A control whose node is `nil` (user removed it) is a no-op:
  no `set_param`, the `restored` text reads `@node_removed_text`. (Edge Cases)
- **A12** After any refusal the script changes no local value it has not
  seen confirmed by the host (E1); the refusal `message` goes to
  `restored` until the next successful action clears it (A9's
  `restored` semantics: a successful action restores the badge text if
  `S.restored`, else `""`).

## 4. Event mirroring (E)

- **E1** `effect_chain_changed { nodes }` (and the `list_chain()` in G2):
  1. Re-find own nodes by `owner == IDENTIFIER` and `kind`; a missing
     one becomes `nil` and sets `S.node_removed` (A11); a node that
     reappears clears it.
  2. `S.tempo = round(ratio × 100)`; `S.formant = formant`;
     `S.quality = (pitch.quality_mode == "quality") or (stretch.quality_mode == "quality")`.
  3. `semitones` → if `S.sent` and `|semitones − S.sent.semitones| ≤ 1e-6`
     keep `S.key, S.cents = S.sent.key, S.sent.cents`; else `S.key =
     round-half-up(semitones)`, `S.cents = round((semitones − S.key) × 100)`
     and `S.sent = nil`.
  4. `update_widget` for each widget whose value differs from what the
     panel shows (`key`, `cents`, `tempo`, `formant`, `quality`).
  5. If `S.remember` and `current_table() ~= S.last_written` →
     `state.track.set("settings", current_table())`; `S.last_written`
     on success. (FR-006, FR-007, FR-009, FR-011, SC-009)
  The badge is not touched here.
- **E2** `track_changed { track }`:
  1. `S.restored = false`; `update_widget(main, restored, "")`.
  2. `entry = track and state.track.get("settings")` (nil on `no_track`).
  3. entry → validate field-by-field (data-model §3.3); apply:
     `S.key, S.cents = entry.key, entry.cents`; send composed (A7);
     `set_param(pitch, "formant", entry.formant)`; both
     `quality_mode = entry.quality`; `set_param(stretch, "ratio",
     entry.tempo / 100)`; `S.remember = true`; `S.last_written = entry`
     (as validated); `S.restored = true`; `update_widget(main, remember,
     true)`; `update_widget(main, restored, "@restored_text")`.
  4. no entry and `not S.keep_across` → `S.key, S.cents = 0, 0`; send
     composed; `formant = false`; both `quality_mode = "performance"`;
     `ratio = 1.0`; `S.remember = false`; `update_widget(main, remember, false)`.
  5. no entry and `S.keep_across` → no `set_param`; `S.remember = false`;
     `update_widget(main, remember, false)`.
  Widget values for key/tempo/etc. are **not** updated here — E1 will,
  once the host confirms (or immediately in case 5, where nothing changed).
  (FR-012)
- **E3** No other event is handled; `position`, `play_state_changed`,
  `queue_changed` are not subscribed.

## 5. Per-track memory (M)

- **M1** Exactly one key, `settings`, per track; value = data-model §3.3.
- **M2** "Remember is on" ⇔ the entry exists; the toggle mirrors that at
  every `track_changed`/`ready_ack`.
- **M3** Every write is the full table, from mirrored values only (never
  from an unconfirmed request).
- **M4** A field missing or out of range is read as its default and the
  full table is rewritten on the next E1 step 5; the entry is never
  refused or removed by the plugin for being malformed.
- **M5** Turning remember off removes the entry immediately; turning it
  on again writes a fresh table from the values then in effect.
- **M6** `keep_across` is never written to any store.

## 6. Lifecycle and isolation (L)

- **L1** Disable/suspend/crash: the host orphans both nodes; they keep
  their last parameters and keep processing (009 rule); the panel and
  actions are removed/greyed per 011; no plugin code runs. (FR-014, SC-005)
- **L2** Re-enable/Restart: G2's adopt path — no parameter is touched;
  what is playing does not change. (FR-005, SC-005)
- **L3** A handler throw or hang is contained by 009's budget; the
  plugin never affects Section Loop's state or vice versa — the two
  hold disjoint permissions and disjoint `state.track` keys. (FR-016)
- **L4** The host's Effect Chain panel may edit, bypass or remove either
  node at any time; the plugin mirrors (E1) and never reverts or
  recreates while Active. (Constitution X)
- **L5** Sign-out clears the per-track scope host-side; the plugin needs
  no code for it. (Edge Cases)

## 7. Named tests (`crates/modplayer-core/tests/controller_key_tempo.rs` unless noted)

| Test | Rule(s) | Asserts |
|---|---|---|
| `key_tempo_package_is_valid` (`bundled_key_tempo.rs`) | P1, P2 | manifest validates, api 1.4, 5 required / 0 optional, `packages()[1]` |
| `key_tempo_licences_match_root` (`bundled_key_tempo.rs`) | P1 | byte-identical licence copies |
| `key_tempo_uses_only_public_api` (`bundled_key_tempo.rs`) | P3 | every `api.<ns>.<m>(` in `main.luau` ∈ `v1.toml`; no `debug_probe` |
| `both_bundled_plugins_enabled_with_grants` (`plugins_manifest_discovery.rs`) | P4 | Section Loop and Key & Tempo `Active`, all declared permissions granted, `can_uninstall == false` |
| `ready_creates_adjacent_pitch_and_stretch_nodes` | G1–G3 | chain = `[pitch_shift(owner=kt), time_stretch(owner=kt)]`, 8 actions, panel `main` with 16 widgets |
| `key_minus_two_sets_semitones_not_ratio` | A1, A7 | two `key_down` → `semitones == -2.0`, `ratio == 1.0` (US1-1) |
| `reset_key_zeroes_key_and_cents` | A8 | from +7/+30 → `semitones == 0`, widgets 0/0 (US1-2) |
| `fine_tune_composes_with_key` | A2, A7 | key −5, cents +30 → `semitones == -4.7`, widgets −5 / 30 (US1-3) |
| `formant_touches_only_pitch_node` | A6 | `pitch.params.formant == true`, stretch unchanged (US1-4) |
| `tempo_sixty_percent_keeps_semitones` | A3 | `ratio == 0.6`, `semitones` unchanged (US2-1) |
| `tempo_change_while_section_loop_armed` | A3, L3 | Section Loop armed region stays armed; chain has exactly the two kt nodes (US2-2) |
| `tempo_up_button_uses_configured_step` | A4, A5 | 4 × button at step 10 → 140 %; step 5 → +5 (US2-3, US2-5) |
| `host_plus_minus_drives_stretch_and_panel_follows` | E1, research R7 | `tempo_step(1)` → `ratio == 1.1`; after one tick the `tempo` widget == 110; kt's `Plus` binding flagged in conflict (US2-4) |
| `step_note_shown_iff_step_not_ten` | A5 | text ↔ `""` (US2-5) |
| `reset_tempo_returns_to_one` | A8 | `ratio == 1.0` (US2-6) |
| `remember_on_writes_settings_immediately` | A9, M1 | store holds `{key=-2,cents=0,tempo=100,formant=false,quality="performance"}` (US3-1) |
| `every_actor_updates_remembered_entry` | E1 step 5 | panel edit, action, `tempo_step`, `chain_set_param` from host → entry follows (US3-1, SC-009) |
| `next_track_without_entry_resets_and_no_badge` | E2 step 4 | `semitones == 0`, `ratio == 1.0`, `restored == ""` (US3-2) |
| `return_to_remembered_track_restores_with_badge` | E2 step 3 | −2 applied, `remember == true`, `restored == "Restored from this track's memory"` (US3-3) |
| `never_remembered_track_resets_tempo_markers_untouched` | E2, L3 | tempo 100 % on return, Section Loop's markers intact (US3-4) |
| `keep_across_tracks_carries_values_without_badge` | E2 step 5 | −2 persists on a no-entry track, badge `""` (US3-5) |
| `remember_off_removes_entry` | A9, M5 | key absent; later visit resets (US3-6) |
| `restart_after_suspend_adopts_nodes_without_set_param` | G2, L2 | same node ids, no `ChainSetParam` pushed, widgets mirrored, `remember` on, badge `""` (US3-7) |
| `enable_mid_track_applies_entry_like_track_changed` | G2 (neither) | −2 audible, badge shown (US3-8) |
| `malformed_entry_defaults_field_by_field_and_rewrites` | M4 | `{key="x", tempo=999}` → 0 / 100 applied; entry rewritten in full on next change |
| `remember_with_no_track_reverts_toggle_shows_message` | A9 | toggle back to false, `restored` == refusal message |
| `auto_switch_shows_quality_on_without_plugin_set_param` | A6, E1 | key +7 → pitch `auto_switched`, `quality` widget true; no `set_param(quality_mode)` from the plugin |
| `user_quality_flip_sets_both_nodes_and_clears_auto` | A6 | both `quality_mode == "performance"`, `auto_switched == false` |
| `host_panel_edit_is_mirrored_not_reverted` | E1 step 3, L4 | `chain_set_param(pitch, 0, -4.7)` → widgets −5 / 30, no counter-write |
| `own_split_survives_echo` | E1 step 3 | key +4 / cents +50 stays +4 / +50 after the echo |
| `removed_node_is_not_recreated_while_active` | A11, L4 | `chain_remove_node(pitch)` → 1 node, `restored` == node-removed text, key controls no-op |
| `disable_orphans_nodes_with_last_params` | L1 | `orphaned == true`, `params` unchanged, panel gone |
| `every_string_is_at_key` (`bundled_key_tempo.rs`) | P2, FR-017 | no bare user-facing literal in `register_panel`/`register_action`/`update_widget` calls except `""` |
| `panel_controls_keyboard_operable_named` (`modplayer-ui/tests/plugin_panels.rs`) | FR-017, SC-006 | every widget has a non-empty accessible name; sliders/toggles/buttons focusable |
