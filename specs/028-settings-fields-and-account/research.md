# Research: Settings Fields, Placeholders, and Account Summary

**Feature**: 028-settings-fields-and-account | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)

Every Technical Context unknown is resolved below. Format per item: **Decision / Rationale / Alternatives considered**. Code facts cited are from the tree at plan time.

---

## R1 — One shared "settings field" builder instead of per-screen ad-hoc layout

**Decision**: Add `crates/modplayer-ui/src/settings/field.rs` with one builder, `field::row(ui, FieldSpec, |ui| -> Response) -> FieldOutput`, that draws, in order: label (body), help text (indented `space::SM`, `text::SECONDARY`, `roles.text_secondary`, capped at `theme::body_measure`), the control (closure), an optional **Reset** button on the control's line, and an optional range caption under the control. It returns the whole field's rect (for highlight / scroll), the control's `Response` (for focus) and whether Reset was clicked. Audio, Playback, Appearance and Language are rewritten to go through it; each group is wrapped in the existing `widgets::controls::panel_card(ui, header, …)`.

**Rationale**: Today every screen repeats `ui.label` + `ui.scope(set_max_width…)` (`audio.rs`, `playback.rs`, `language.rs`; `appearance.rs` even skips the measure cap). FR-001–FR-006 and FR-007's "highlight label+help+control" all need the *same* geometry; one builder makes them testable once (help x-offset, secondary style, caption presence, reset presence) and keeps categories consistent. `panel_card` already provides `surface_raised` fill, `space::LG` padding, `radius::MD`, and a `section_label` header with `Role::Heading` + un-uppercased accessible name — exactly "card with small group header (section-label style)".

**Alternatives considered**: (a) Patch each screen inline — rejected: 4 copies of the indentation/reset/caption logic, no single test point. (b) A `SettingField` trait with per-field impls — rejected: Constitution X (no single-implementor traits) and overkill; a plain struct + closure suffices. (c) `egui::Grid` two-column label/control layout — rejected: help text must sit *under* the label at 72-char measure, and 40 % expansion breaks fixed columns at 960 px.

## R2 — Units rendered inside the control via Fluent-formatted values

**Decision**: Sliders/drag values use `custom_formatter` / `custom_parser` (egui 0.36) instead of `.suffix(" ms")`. Formatter: `tr_args("setting-value-dbfs" | "setting-value-percent" | "setting-value-ms", [("value", formatted_number)])`. Parser: extract the leading signed decimal number from the typed text (tolerates the unit and pseudo-expansion padding), `None` on no number (egui then keeps the old value). Number formatting stays `{:.1}` for dBFS, integer for % and ms (spec: decimal separator unchanged).

**Rationale**: FR-003 says the unit is inside the control's value box and comes from Fluent (pt-BR may place/space it differently, e.g. `50 %`). A Fluent message with a `$value` placeholder lets each locale choose placement and spacing; a string suffix cannot. Also removes the one literal user-facing string left in `playback.rs` (`" ms"`).

**Alternatives considered**: `.suffix(format!(" {}", tr("unit-dbfs")))` — rejected: bakes English spacing/order into code. Separate unit label next to the slider — rejected by the clarification ("not in a separate label").

## R3 — Range captions read from the clamp constants

**Decision**: Captions are `tr_args("setting-range-dbfs" | "-percent" | "-ms", [("min", …), ("max", …)])` fed from the same constant the control clamps to:
- ceiling → `CeilingDb::MIN` / `CeilingDb::MAX` (engine, existing, read-only use);
- nudge step → new `pub const NUDGE_STEP_MS_RANGE: RangeInclusive<u16> = 1..=1000` in `modplayer-core/src/settings/model.rs`, re-exported, and used by the existing private `clamp_nudge_step_ms` **and** the UI `DragValue::range` + caption (removes the duplicated `1..=1_000` literal in `playback.rs`);
- safe-volume cap → UI-local `const SAFE_VOLUME_CAP_RANGE: RangeInclusive<u8> = 0..=100` in `audio.rs`, used by the slider range, the `clamp` and the caption.

**Rationale**: Spec clarification: "caption values are read from the same constants the control clamps to (no duplicated literals)". For the cap, adding `VolumePercent::MIN/MAX` to the engine crate would pull the engine into this presentation feature and require GOV-3.2 area-maintainer sign-off for two constants; the UI already owns the 0..=100 literal, so centralising it there removes the duplication without touching the engine.

**Alternatives considered**: Engine `VolumePercent::MAX` — rejected (engine touch, GOV-3.2 for a UI need). Hard-coding caption text in Fluent ("-6.0 to -0.1 dBFS") — rejected: would drift from the clamp.

## R4 — Per-field reset: defaults, visibility and write path

**Decision**: Defaults come from **one** snapshot, `AudioSettings::default()` (held as `SettingsScreen.defaults`), plus `CeilingDb::default()` via that snapshot's `limiter_ceiling_db`. Reset visibility is `current != default`, recomputed every frame from the live value the control shows. Write paths (identical to the manual-change path for each field):

| Field | "current" read from | Reset writes via |
|---|---|---|
| Buffer preset | `controller.preset()` | `controller.confirm_device(preferred, default)` — offered only when `preferred_device()` is `Some` (same precondition as a manual change) |
| Limiter ceiling | `controller.ceiling()` | `controller.set_ceiling(default)` |
| Safe-volume switch | `cached.safe_volume.enabled` | `persist_safe_volume(.., {enabled: default, cap: current})` |
| Safe-volume cap | `cached.safe_volume.cap` | `persist_safe_volume(.., {enabled: current, cap: default})` (offered even when switch off) |
| Theme | `cached.theme` | existing reload-mutate-save + `theme::apply` (factored into `persist_theme`) |
| High contrast | `cached.high_contrast` | `controller.set_high_contrast(false)` + reload-mutate-save (factored into `persist_high_contrast`) |
| Device name | `cached.device_name.is_some()` | `controller.set_device_name("")` (existing "empty restores default" path), then refresh `PlaybackScreen` draft/effective name and `cached` |
| Nudge step | `controller.nudge_step_ms()` | `controller.set_nudge_step_ms(default)` |

After a reset the field's control gets `request_focus()` on the **next** frame (reset button disappears the same frame, so focus is deferred via a `pending_focus: Option<&'static str>` on `SettingsScreen`, reusing the descriptor-id focus path). `cached` is refreshed from the store after any write that goes through the controller and touches a field the Settings screen reads from `cached` (device name).

**Rationale**: FR-005/FR-006/FR-017: reset must use the existing setters; no new persistence path. Comparing against `AudioSettings::default()` means a hand-edited settings file shows Reset automatically. Deferring focus avoids requesting focus on a widget that is removed the same frame.

**Alternatives considered**: A new `PlaybackController::reset_field(id)` API — rejected: widens core API for a UI affordance; each field already has a setter. Per-field default literals in UI — rejected: drift from `AudioSettings::default()`. Offering buffer-preset reset with no preferred device — rejected: there is no write path (manual change is also a no-op then); recorded as an edge in plan.md.

## R5 — Search highlight: state, geometry, lifetime

**Decision**: `SettingsScreen` gains `highlight: Option<FieldHighlight { id: &'static str, started_at: f64 /* ctx.input time */, armed: bool }>`. Set when a descriptor result is chosen for Audio/Playback/Appearance/Language/Account (not Controls/Developer; plugin hits unchanged). The field builder receives `highlight_id` and, when it matches, (1) calls `response.scroll_to_me(Some(Align::Center))` on the field rect the first frame, (2) paints a stroke of `max(2.0, controls::focus_ring_width(roles))` in `roles.accent` plus a fill `roles.accent.gamma_multiply(HIGHLIGHT_FILL_ALPHA)` (theme-module constant) behind the field rect, and (3) keyboard focus goes to the control (existing `focus_target`). Clearing: `ctx.input(|i| i.time) - started_at >= 3.0`, or any `Event::Key { pressed: true }` / `Event::PointerButton { pressed: true }` in a frame *after* the one it was armed in. While active, `ctx.request_repaint_after(remaining)` guarantees the 3 s expiry repaints. Account targets: Re-check and Sign out buttons are wrapped by the same highlight painter.

**Rationale**: FR-007/FR-008: outline ≥ 2 px is the non-colour cue; accent role keeps high-contrast behaviour (HC roles already raise accent contrast; HC focus ring width 3 px). `armed` prevents the very click/Enter that selected the result from clearing it instantly. Using egui's `input.time` makes the timer testable headless by setting `RawInput::time`.

**Alternatives considered**: `std::time::Instant` — rejected: not controllable in headless tests. Animating fade — rejected: YAGNI; spec asks only for clear-on-timeout. Painting in a foreground layer like the focus ring — rejected: highlight must scroll with content and sit behind the controls.

## R6 — "Coming soon" status and category row badge

**Decision**: Add `pub const fn is_available(self) -> bool` to `SettingsCategory` in `modplayer-core/src/settings_registry.rs` (false for `Offline`, `PrivacyDiagnostics`; true otherwise). In `category_row.rs`, an unavailable item's text is a `LayoutJob`: `section_label(label)` + `space::XS` gap + `settings-coming-soon` in `text::SECONDARY`/`roles.text_secondary`. `measure_category_width` measures the same job (so the badge counts in `partition`); pinned items and "More" menu items use the same job builder. Accessible name: `tr_args("settings-category-coming-soon-a11y", [("category", label)])`. Content area for these two categories: `panel_card(ui, category label, |ui| label(tr(unavailable key)))` with a per-category key.

**Rationale**: FR-009/FR-010. Keeping status in the registry next to `label_key` means the later features (003-offline/001, 007-ops/001) flip one line and the registry test that already asserts "Offline/Privacy contribute no descriptors" can assert consistency (`!is_available ⇒ no descriptors`). A single job builder guarantees the drawn width equals the measured width, preserving the 020 row contract (no wrapping, correct overflow).

**Alternatives considered**: UI-only match on the two variants — rejected: status would silently drift when later features add descriptors. Icon/dot badge — rejected: colour/icon-only markers violate FR-010 ("text"). Disabling the categories — rejected by clarification (must stay selectable).

## R7 — Account summary layout and fallbacks

**Decision**: `account.rs::show_signed_in` renders `panel_card(ui, tr("account-summary-title"), …)` containing: identity line `RichText::strong()` (body) → `display_name` or `tr("account-identity-unavailable")` when `trim().is_empty()`; then two label–value lines in `text::SECONDARY` label + body value: `account-tier` → Premium/Free/`account-tier-unverified` ("Not verified yet") for `Tier::Unknown`; `account-last-verified` → formatted time (R8) or `account-never-verified`. Below the card: Re-check help + button (unchanged behaviour), `destructive_gap`/divider, destructive Sign out. `Expired` uses the same path (already routed). The `account() == None` fallback now calls `show_signed_out` instead of printing `placeholder-settings-category`.

**Rationale**: FR-011/FR-012; `panel_card` gives the "one summary block" and a heading for AT. `tier-unknown` ("Unknown") stays for the tier gate screens that already use it; a new key avoids changing their wording.

**Alternatives considered**: Re-using `tier-unknown` and editing its value — rejected: changes other screens' text. Putting actions inside the card — rejected: spec places them beneath the card.

## R8 — Human-readable local date/time for `last_validated_at`

**Decision**: Split into a pure formatter and an offset resolver:
- `fn format_last_verified(when: OffsetDateTime, offset: UtcOffset) -> String` (pure, unit-tested): converts `when.to_offset(offset)` and returns `tr_args("account-last-verified-at", [day, month (from `date-month-short-{1..12}` keys), year, time "HH:MM" zero-padded 24 h])` → en-US "30 Sep 2026, 17:59".
- `fn local_offset_at(when: OffsetDateTime) -> UtcOffset`: uses `chrono::Local.timestamp_opt(when.unix_timestamp(), 0)` → `offset().fix().local_minus_utc()` → `UtcOffset::from_whole_seconds`, falling back to `UtcOffset::UTC` on any failure.
- Add `chrono = { version = "0.4", default-features = false, features = ["clock"] }` to `crates/modplayer-ui/Cargo.toml` (workspace `[workspace.dependencies]` entry). `chrono 0.4.45` with `iana-time-zone` is **already in Cargo.lock** (via `oauth2`), so no new crate enters the build and `cargo deny` sees no new license/duplicate.

**Rationale**: `time`'s `UtcOffset::current_local_offset()` needs the `local-offset` feature and returns `IndeterminateOffset` on Unix once the process is multi-threaded (the UI runs after tokio/cpal threads start); capturing it once at startup would also be wrong across DST changes. `chrono::Local` resolves per-instant via the OS tz database, is thread-safe and DST-correct. Month abbreviations via Fluent keep the date localisable (pt-BR "30 set 2026, 17:59").

**Alternatives considered**: (a) `time` + `local-offset` captured at the top of `main` — rejected: DST-incorrect, relies on single-threaded window. (b) RFC 3339 / ISO numeric — rejected: spec requires "human-readable", example uses month name. (c) `icu`/`fluent` date formatting — rejected: heavy new dependency tree. (d) show UTC — rejected: spec says local.

## R9 — Sign-out dialog width, wrapping and default focus

**Decision**: Pure `pub fn signout_dialog_width(viewport_width: f32) -> f32 = (viewport_width - 2.0 * space::XXL).clamp(420.0, 560.0)` (constants `SIGNOUT_DIALOG_MIN_WIDTH = 420.0`, `SIGNOUT_DIALOG_MAX_WIDTH = 560.0`). Inside `Modal::show`, `ui.set_width(w)`; title/intro/bullets drawn with `egui::Label::new(..).wrap()` (never `truncate`); each bullet is its own wrapped label with a hanging indent (`•` + text in a horizontal with wrapping text). Focus/Escape/backdrop behaviour is kept as is (already: Cancel focus once, `should_close` → Cancel, Destructive variant) and locked by tests.

**Rationale**: FR-013/FR-014. At 960 px: `960 − 64 = 896 → 560`; min clamp guarantees usable width even if the viewport ever shrinks. Two consequence categories (`signout-category-credential`, `…-account-details`) plus title/intro/buttons are ≈ 200 px tall with 40 % expansion — far under 640 px, so no internal scroll needed. `space::XXL = 32` ⇒ `2×XXL = 64`, matching FR-013's `viewport_width − 64`.

**Alternatives considered**: Fixed 560 px — rejected: spec requires the clamp formula. A `ScrollArea` inside the modal — rejected: spec says all consequences visible without internal scrolling.

## R10 — pt-BR strings

**Decision**: Create `locales/pt-BR/settings.ftl` and `locales/pt-BR/account.ftl` holding pt-BR values for **this feature's new and changed keys only**, each with the 025/027 header note ("not yet a shipped locale; exists so the parity test proves translations exist"). Extend `crates/modplayer-ui/tests/fluent_keys.rs` with the new keys (tables + `no_unused_keys…` coverage) and a parity test asserting every key in the new pt-BR files exists in en-US and every new/changed en-US key exists in pt-BR.

**Rationale**: Constitution X / FR-015; `i18n.rs` resolves en-US only, and prior features (025, 027) established the "stage pt-BR per feature + parity test" pattern (`locales/pt-BR/{effects,library,plugins}.ftl`).

**Alternatives considered**: Translating all of settings.ftl/account.ftl — rejected: out of scope. Skipping pt-BR — rejected: FR-015.

## R11 — 40 % text expansion and high-contrast verification

**Decision**: Reuse the 018 `i18n::with_pseudo_expansion(40, …)` hook in headless tests: render each owned category and the sign-out modal at 960×640 and assert via AccessKit node bounds that (a) group headers, labels, unit values, captions, Reset buttons and badges are not clipped (`bounds` within the parent/card and viewport), (b) the category row still does not wrap (020 tests re-run with expansion), (c) the modal rect lies fully inside the viewport. High-contrast: run the same render with `apply_tokens_for(ctx, true)` and assert the highlight stroke width ≥ 2 and that `design_token_literals.rs` finds no colour literals in new files.

**Rationale**: SC-007, FR-016; mechanisms already exist (018 R12, 017 HC, 014 literal scan).

**Alternatives considered**: Screenshot diffing — rejected: no infra; manual scenarios cover visual judgement.

## R12 — Grouping and field order

**Decision**: Exactly the clarified groups: Audio → "Output" (output device, buffer preset, Test output device) and "Level protection" (limiter ceiling, safe-volume switch, safe-volume cap); Playback → "Connect device" (device name) and "Markers" (nudge step); Appearance → "Theme" (theme, high contrast); Language → "Language" (locale). Account → summary card + actions. Controls, Plugins, Developer, About untouched. Test-output-device is a field with help and an action button (no reset).

**Rationale**: Spec clarification Q1; keeps today's field order within groups.

**Alternatives considered**: One card per field — rejected: defeats grouping purpose (UX-33).

## R13 — Deviations found in the manual walk (T048, 2026-09-30)

Three live-only defects surfaced in the M1–M9 walk; headless tests had not caught them. Each now has a regression test and a fix:

1. **Slider rails invisible inside cards (M1).** egui paints a slider rail with `widgets.inactive.bg_fill` = `surface_raised`, which is also the `panel_card` fill. Moving the ceiling and cap sliders into cards made the rail disappear and left only the knob. **Fix:** `field::row` scopes the control with `widgets.inactive.bg_fill = surface_base`. Buttons and combo boxes fill from `weak_bg_fill`, so they are unaffected. **Test:** `field::tests::slider_rail_is_visible_on_the_card_fill`, run in light and dark, with and without HC. The Now Playing effect sliders inside cards probably have the same problem. That is outside this feature's scope and was not changed.
2. **"Coming soon" badge unreadable on the selected item (M5).** `text_secondary` on the accent-filled selected item measured about 1.04:1. **Fix:** while its item is selected, `category_item_job(.., selected)` gives the badge the button's inherited (on-accent) text colour. Width is unchanged. **Test:** `category_row::tests::badge_follows_the_selected_text_colour`.
3. **Sign-out dialog opened with nothing focused (M7).** The first Tab landed on Cancel, and Enter/Space did nothing. In the real shell, the Cancel focus request made on the opening frame did not survive the frame. **Fix:** the request stays pending until Cancel actually has focus, then clears, so the user can still move focus. **Test:** `settings_account::s4_cancel_focus_survives_a_competing_request_on_the_opening_frame`. That test models the lost request; it was red before the fix and is green after.

**Not a feature defect (M6):** with the live account, the profile/tier check after sign-in failed silently. `account.toml` kept `tier = "unknown"`, an empty `display_name` and no `last_validated_at`, and Re-check did not change that. The summary card therefore showed its fallbacks ("Name unavailable", "Not verified yet", "Never verified online"). This is the A2/A3 behaviour working correctly. The populated path (name, "Premium", localized date) could not be observed live. A1/A4 unit and integration tests cover it. The cause is in `modplayer-account`'s profile fetch, outside FR-017's scope. Follow-up: investigate the silent `TierCheckFailed`.
