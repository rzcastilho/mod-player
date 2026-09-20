# Contract: Waveform overlays, plugin settings pages, plugin notifications

**Feature**: 011-plugin-ui-contributions | **Spec**: FR-014–FR-020a | **Extends**: [005 ui-waveform.md](../../005-now-playing-waveform/contracts/ui-waveform.md) (overlay callback), [006 ui-markers.md](../../006-markers-loops-and-cues/contracts/ui-markers.md) §5 (draw order), 001 Settings/search and notification contracts, [009 gateway-and-runtime.md](../../009-plugin-runtime-and-permissions/contracts/gateway-and-runtime.md) G5/G6 (rate/storage)

## 1. Overlays (FR-014–FR-016)

### 1.1 Core (`OverlayRegistry`)

- **O1** `add` validates the whole batch (`validate_primitives` with the plugin's manifest glyph keys), then checks `existing ∪ batch` count ≤ 500 → else `overlay_limit`, nothing changed.
- **O2** `remove` refuses `not_found` if any id is unknown, removing nothing.
- **O3** `clear` on: explicit call; `TrackChanged` (any direction, including sign-out's `None`); `on_stop(any reason)`.
- **O4** `view()` returns layers for `Active` plugins only, ordered by the plugin's first-registration sequence (stable for the session).

### 1.2 Painting (`modplayer-ui/src/plugin_overlays.rs`)

- **O5** Hook point: inside both `waveform::overview` and `waveform::detail`'s `overlays` closure, **after** 006's marker/loop pass and **before** `paint::playhead` — so plugin drawings sit above markers and the loop region and below the playhead (FR-015).
- **O6** Position mapping: `frame = at_ms × sample_rate / 1000` via `TimeSpace::x_of`; a frame beyond `len_frames` is skipped (drawn once the streaming duration grows, 005's fallback).
- **O7** Geometry: `line` — 1 px vertical over the waveform rect; `region` — fill at 25 % alpha over the waveform rect; `glyph` — 12 px, centred on `x`, in a 16 px **plugin lane** reserved above the waveform rect of both views; `label` — detail view only, `small` font in the lane, left-anchored at `x`, clipped to the view.
- **O8** Colours: `theme::overlay_color(token, &visuals)` — `accent` = `visuals.selection.bg_fill`, `secondary` = `visuals.hyperlink_color`, `positive` = `MARKER_PALETTE[2]`, `warning` = `MARKER_PALETTE[3]`, `neutral` = `visuals.weak_text_color()`. No other colour input exists.
- **O9** Overlays are painted only; they register no `Sense`, no `WidgetInfo`, no focus claim.
- **O10** Zero controller/plugin calls on zoom/scroll — the paint reads the registry view each frame (SC-003).

### 1.3 Assets

- **O11** `PluginAssets` are decoded once at discovery (PNG only; icon ≤ 128 px / 256 KiB, glyph ≤ 32 px / 64 KiB; square icon). Each failure → console warning naming the file; the generic glyph substitutes.
- **O12** Host glyph set: `dot`, `flag`, `note`, `chord`, `star`, `warning` — painted vector shapes in `theme.rs` (no bitmap assets).

## 2. Settings pages (FR-017, FR-018)

- **S1** `register_settings` validates the whole schema (`validate_schema`); replaces the plugin's page; values for kept fields are taken from the `stored` snapshot the runtime shipped (default-substituted when invalid) and retained for removed fields in storage untouched.
- **S2** Page location: Settings › Plugins → below the existing plugin-management placeholder, a "Plugin settings" list, one sub-page per Active plugin with a page, sorted by name. Hidden while the plugin is not Active.
- **S3** Field rendering: `boolean` → `Checkbox`; `number` → `Slider` with `DragValue` (`step`, clamp to `[min,max]`); `string` → single-line `TextEdit` (`char_limit(1024)`); `choice` → `ComboBox`. Accessible name = label; description = tooltip and AccessKit description.
- **S4** Apply timing: boolean/choice on change; number/string on commit (`Enter` or `lost_focus`). Each apply → `controller.plugin_settings_edit` → registry `edit` → `Control::SettingsWrite { changes }` → plugin thread `store.set(Scope::Settings, …)` → `settings_changed { changes }` handler.
- **S5** Storage: `settings.json` beside `plugin.json`, same atomic writer, counted in the 10 MB cap; unreachable from `api.state.*`. Sign-out does not clear it.
- **S6** Search: `search_plugin_settings(query, views)` matches case-insensitively on field label (and description); hits render as "Plugins › <plugin name> › <label>" and open the sub-page.
- **S7** `get_settings()` and the page never disagree: both read the same `Settings` scope (runtime) / the same `values` map (core) and both substitute defaults with the same `validate_stored`.

## 3. Notifications (FR-019, FR-020)

- **N1** `notify` is admitted under the `notify` bucket (6 / 60 s) **before** validation; `validate_notify` then refuses a bad level or > 200 chars with `invalid_value`.
- **N2** Raised via `NotificationCenter::raise_attributed(severity, "plugin-notification", [("plugin", name), ("text", text)], PluginAttribution { id, name })` — an ordinary non-blocking entry under 001's lifecycle; never a modal; no dedupe key, no actions.
- **N3** The UI renders the attribution icon (16 px, generic glyph fallback) and name before the text; AccessKit name is the resolved Fluent string.
- **N4** Already-shown notifications are unaffected by the plugin's later disable/suspend.

## 4. Named tests

- core `tests/plugin_ui_registry.rs::{overlay_501st_refused_prior_unchanged, overlay_readd_replaces_in_place, remove_unknown_not_found_atomic, overlays_cleared_on_track_change, overlays_cleared_on_stop, settings_reregister_keeps_values, settings_invalid_stored_uses_default}`
- core `tests/controller_plugin_ui.rs::{overlay_layers_in_registration_order, notify_seventh_rate_limited, notify_window_rolls, notify_invalid_consumes_slot, settings_edit_delivers_changed, settings_persist_across_restart, settings_not_visible_via_state_plugin, settings_hidden_when_disabled}`
- gateway `tests/state_store.rs::{settings_scope_counts_toward_cap, settings_scope_round_trip}`
- ui `tests/plugin_overlays.rs::{primitives_reproject_under_zoom_and_scroll, label_only_on_detail, above_markers_below_playhead_order, beyond_duration_not_drawn}`
- ui `tests/settings_plugins.rs::{page_listed_by_name, field_roles_and_names, number_applies_on_commit, boolean_applies_on_change, search_finds_field_with_path}`
- ui `tests/notifications.rs::{plugin_notification_attributed, never_modal}`
