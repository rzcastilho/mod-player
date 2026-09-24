# Contract: The Appearance Setting (A1–A16)

**Feature**: 017-high-contrast-appearance
**Covers**: FR-001, FR-003, FR-004, FR-014, FR-015, FR-016 · SC-004,
SC-005, SC-009

Shapes are from [data-model.md](../data-model.md) §1 and §6; the reason
the raw type is `toml::Value` is [research.md](../research.md) R10.

Target suites: `crates/modplayer-core/src/settings/model.rs`'s
`#[cfg(test)]` module, `crates/modplayer-core/tests/settings.rs`,
`crates/modplayer-ui/tests/fluent_keys.rs`, and a new
`crates/modplayer-ui/tests/high_contrast.rs`.

---

## §1 — Persistence (FR-003)

| # | Clause | Test |
|---|---|---|
| **A1** | `AudioSettings::default().high_contrast == false`. | `default_settings_have_high_contrast_off` |
| **A2** | `[appearance]` with `theme` but **no** `high_contrast` yields `high_contrast == false` and **no** `InvalidField`. | `absent_high_contrast_defaults_off_silently` |
| **A3** | `high_contrast = true` yields `true`; `= false` yields `false`. | `high_contrast_round_trips` |
| **A4** | A present-but-non-boolean value (`"yes"`, `1`, `[]`, `{}`) yields `high_contrast == false` **and** pushes `InvalidField::HighContrast`. | `malformed_high_contrast_recovers_and_reports` (a case per TOML type) |
| **A5** | **In case A4, every other setting in the file survives** — `theme`, volume, device, keybindings, panel flags are all parsed normally, and the outcome is **not** `SettingsWarning::Unreadable`. *This is the clause a plain `bool` field would fail (research R10), and the one most likely to be lost to a "simplifying" refactor.* | `a_malformed_high_contrast_does_not_discard_the_file` |
| **A6** | `InvalidField::HighContrast.field_name() == "appearance.high_contrast"` — the same channel `InvalidField::Theme` uses. | `high_contrast_invalid_field_has_its_own_name` |
| **A7** | A malformed `theme` **and** a malformed `high_contrast` in one file report **both** invalid fields, in declaration order, and recover both. | `both_appearance_fields_can_be_invalid_at_once` |
| **A8** | `to_raw` writes a plain TOML boolean: a saved file contains `high_contrast = false`, not a tagged or quoted value, and re-reads identically. | `saved_high_contrast_is_a_plain_toml_boolean` |
| **A9** | `SCHEMA_VERSION` is still `1`; a file written before this feature loads with no warning (forward-compatible, no migration). | `pre_feature_settings_files_still_load` |
| **A10** | **Property-based round-trip** (Constitution VIII: state serialization requires a proptest): for an arbitrary `(Theme, bool)` pair, `to_raw` → serialize → deserialize → `into_settings` returns the same pair with an empty `invalid` list. Lives beside `plugin_panels_round_trip_proptest` in `crates/modplayer-core/tests/settings.rs`. | `appearance_axis_round_trip_proptest` |

## §2 — Independence from the theme axis (FR-001, FR-014)

| # | Clause | Test |
|---|---|---|
| **A11** | `modplayer_engine::Theme` still has exactly three variants; no fourth is added. | `theme_enum_is_unchanged` |
| **A12** | All six `(Theme, high_contrast)` combinations round-trip through the settings store and select the intended role table (data-model §1.1). US2 AS-1/AS-4. | `every_theme_and_axis_combination_round_trips` |
| **A13** | Changing `theme` leaves `high_contrast` untouched and vice versa — neither write normalises the other. US2 AS-4. | `the_two_axes_are_independent` |

## §3 — The control (FR-015, FR-016)

| # | Clause | Test |
|---|---|---|
| **A14** | The Appearance screen renders a **checkbox** below the Theme combo whose accessible node has `Role::CheckBox`, a non-empty label from `tr("setting-high-contrast")`, and a `Toggled` state matching the setting. It is **not** a fourth entry in the Theme combo (FR-001: "alongside, not inside"). US2 AS-1. | `appearance_screen_shows_a_high_contrast_checkbox` |
| **A15** | `setting-high-contrast` and `setting-high-contrast-desc` exist in `locales/en-US/settings.ftl`, resolve through `tr()`, and appear in `tests/fluent_keys.rs`'s inventory. No Rust string literal labels the control (Constitution X). | `tests/fluent_keys.rs` (extended inventory) |
| **A16** | A `SettingDescriptor` with category `Appearance`, id `appearance.high_contrast` and those two keys exists in `settings_registry.rs`; a Settings search for the title returns it; arriving at the Appearance screen with `focus == Some("appearance.high_contrast")` leaves keyboard focus **on the checkbox**, not on the Theme combo. SC-009. | `high_contrast_is_searchable_and_focusable` |

## §4 — Live application (FR-004 · SC-005)

| # | Clause | Test |
|---|---|---|
| **A17** | Toggling the checkbox calls `controller.set_high_contrast(on)` and persists through the same reload-mutate-save block the Theme combo uses (`settings/appearance.rs:51-61`), raising `settings-save-failed` on a write error exactly as the theme branch does. | `toggling_the_checkbox_persists_and_updates_the_controller` |
| **A18** | `App::new` applies the persisted value **before the first paint**, and `App::ui` applies `controller.high_contrast()` as its first statement, so the frame after a toggle renders with the new tables and no intermediate frame renders with the old ones. US2 AS-2, SC-005. | `the_next_frame_uses_the_new_tables` |
| **A19** | After a simulated restart (fresh controller from the same settings file), both `theme` and `high_contrast` are exactly as left. SC-004, US2 AS-3. | `both_axes_survive_a_restart` |
| **A20** | Toggling on and off repeatedly within consecutive frames leaves no frame on a mixed palette: every frame's applied style is one of the four whole tables, never a blend. Edge Case 2. | `rapid_toggling_never_produces_a_mixed_palette` |

## §5 — Out of scope, pinned (FR-014)

| # | Clause | Test |
|---|---|---|
| **A21** | No OS high-contrast / increased-contrast preference is read anywhere: no new platform API call, and `theme::apply` still maps only `Theme` → `ThemePreference` (its existing `maps_every_theme_to_its_preference` test passes verbatim). | `theme/mod.rs` test, unmodified |
| **A22** | Keyboard operability, accessible names/roles/states and every component's layout and interaction behaviour are unchanged: `tests/accessibility.rs`, `tests/actions.rs`, `tests/controls.rs`, `tests/control_variants.rs` and `tests/control_inventory.rs` all pass **unmodified**. The only new accessible node in the app is the checkbox (A14). | the five named suites, unmodified |
