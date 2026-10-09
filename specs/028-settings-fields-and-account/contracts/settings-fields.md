# Contract: Settings Fields, Groups, Reset, Highlight, Placeholder Categories

**Feature**: 028-settings-fields-and-account | Covers FR-001–FR-010, FR-015–FR-017, SC-001–SC-003, SC-005, SC-007.

UI contract for the Settings screen (desktop egui app). Each rule has an id (`F#`) that tests cite. "Headless test" = egui `Context::run_ui` at 960×640 logical px with AccessKit enabled, reading node bounds/roles/names (existing harness in `crates/modplayer-ui/tests/`).

## Groups (FR-001)

| ID | Rule |
|---|---|
| F1 | Audio body = two `panel_card`s in order: **Output** {output device, buffer preset, Test output device} then **Level protection** {limiter ceiling, safe-volume switch, safe-volume cap}. |
| F2 | Playback = **Connect device** {device name} then **Markers** {nudge step}. Appearance = **Theme** {theme, high contrast}. Language = **Language** {locale}. |
| F3 | Each card header is an AccessKit `Role::Heading` whose name is the un-uppercased Fluent value (`panel_card` behaviour). Field order inside a group = today's order. |
| F4 | Controls, Plugins, Developer, About bodies are unchanged (byte-for-byte same widget sequence; existing tests stay green). |

## Field anatomy (FR-002–FR-004)

| ID | Rule |
|---|---|
| F5 | Every field in an owned category is drawn by `field::row`: label, then help (if any), then control line (control [+ Reset]), then range caption (if any). |
| F6 | Help text node's left edge ≥ label node's left edge + `space::SM` (8 px); font = `text::SECONDARY`; colour = `roles.text_secondary`; width ≤ `theme::body_measure`. |
| F7 | Limiter ceiling value text = `setting-value-dbfs {$value}` with one decimal ("-1.0 dBFS"); safe-volume cap = `setting-value-percent` ("50%"); nudge step = `setting-value-ms` ("10 ms"). Unit is inside the slider/drag-value text, never a separate label. Buffer preset keeps "(~N ms)". |
| F8 | Typing into a unit-bearing value accepts a number with or without the unit; unparsable text leaves the value unchanged. |
| F9 | Range caption under ceiling = `setting-range-dbfs {$min=-6.0} {$max=-0.1}`; cap = `setting-range-percent {0} {100}`; nudge = `setting-range-ms {1} {1000}`. Values come from `CeilingDb::MIN/MAX`, `SAFE_VOLUME_CAP_RANGE`, `NUDGE_STEP_MS_RANGE` — the same constants the controls clamp with. |

## Per-field reset (FR-005, FR-006, FR-017)

| ID | Rule |
|---|---|
| F10 | Resettable set = exactly the 8 ids in data-model §3.3. Reset shown ⇔ current ≠ `AudioSettings::default()` value (and, for buffer preset, a preferred device exists). |
| F11 | Reset is a compact `Variant::Secondary` button on the control's line, visible text `settings-reset` ("Reset"), accessible name `settings-reset-a11y {$field}` ("Reset Limiter ceiling to default"). |
| F12 | Activating Reset (click, or Enter/Space when focused) writes the default through the field's normal setter (data-model §3.3); no confirmation; value takes effect the same frame; next frame Reset is absent and keyboard focus is on the field's control. |
| F13 | A value equal to default by any route (manual change back, hand-edited file) shows no Reset. Safe-volume cap Reset is offered while the switch is off. |
| F14 | No Reset for output device, locale, Test output device, Re-check, Sign out, Controls, Developer, plugin fields. |
| F15 | Reset never changes any other field (settings file diff after reset = only that field's key, or removal of `device_name`). |

## Search → field highlight (FR-007, FR-008)

| ID | Rule |
|---|---|
| F16 | Result list text stays `"{category} › {field}"`; plugin hits stay `"Plugins › {plugin} › {field}"`. |
| F17 | Choosing a descriptor result in Audio/Playback/Appearance/Language/Account: category selected, field rect scrolled into view (fully inside the content viewport), control focused, highlight drawn — all in the same activation. |
| F18 | Highlight = rect around label+help+control(+caption) with stroke width ≥ 2 px (`max(2, focus_ring_width)`) in `roles.accent` + translucent accent fill; no colour literals. Visible in high-contrast. |
| F19 | Highlight clears when `input.time − started_at ≥ 3.0 s`, or on the first key press or pointer-button press after the selecting frame, or on category change. A repaint is scheduled so expiry happens without input. |
| F20 | Controls/Developer results: focus only, no highlight (unchanged). Plugin hits: unchanged. Empty query: no results, no highlight. |

## Not-yet-available categories (FR-009, FR-010)

| ID | Rule |
|---|---|
| F21 | Offline body = `panel_card(tr("settings-cat-offline"))` + one wrapped sentence `settings-unavailable-offline`; Privacy & diagnostics likewise with `settings-unavailable-privacy-diagnostics`. `placeholder-settings-category` no longer appears there (still used by Plugins management placeholder — unchanged). |
| F22 | Both remain selectable (not disabled) in the row and "More" menu. |
| F23 | Category item for an unavailable category shows text badge `settings-coming-soon` ("Coming soon") after its label — inline, pinned and in "More" menu — in secondary style; accessible name `settings-category-coming-soon-a11y {$category}` ("Offline, coming soon"). Available categories: no badge, name = label. |
| F24 | Badge width is included in `measure_category_width`; 020 row contracts (single line, overflow into More, pinned selected item, equal row height) hold with badges, including at 40 % expansion. |
| F25 | `SettingsCategory::is_available()` is the only source of the status; test: `!is_available ⇒ no DESCRIPTORS entries`. |

## Cross-cutting (FR-015, FR-016, SC-007)

| ID | Rule |
|---|---|
| F26 | Every new visible string resolves via `tr`/`tr_args` (no literals); keys listed in [fluent-strings.md](./fluent-strings.md) exist in en-US and pt-BR. |
| F27 | With `with_pseudo_expansion(40, …)` at 960×640: no owned-category label, unit value, caption, group header, Reset, or badge node is clipped by its card/viewport, and nothing overlaps its neighbour on the same line. |
| F28 | Tab order within an owned category follows visual order: for each field, control then its Reset; groups top to bottom. All new interactive elements have accessible names. |
| F29 | `design_token_literals.rs` scan covers `settings/field.rs` and finds no colour/spacing literals. |

## Test map

| Test file | Rules |
|---|---|
| `crates/modplayer-ui/tests/settings_fields.rs` (new) | F1–F3, F5–F15, F27, F28 |
| `crates/modplayer-ui/tests/settings_search_highlight.rs` (new) | F16–F20 |
| `crates/modplayer-ui/tests/settings_category_row.rs` (extend) | F22–F24 |
| `crates/modplayer-ui/src/settings/mod.rs` unit tests | F21, highlight state machine (data-model §3.4) |
| `crates/modplayer-core/src/settings_registry.rs` unit tests | F25 |
| `crates/modplayer-core/src/settings/model.rs` unit tests | `NUDGE_STEP_MS_RANGE` clamp unchanged |
| `crates/modplayer-ui/tests/fluent_keys.rs` (extend) | F26 |
| `crates/modplayer-ui/tests/design_token_literals.rs` (extend scope if file-listed) | F29 |
| `crates/modplayer-ui/tests/high_contrast.rs` (extend) | F18 HC |
| existing `settings_plugins.rs`, `controls.rs`, `developer_removed.rs` | F4 (must stay green) |
