---

description: "Task list for High-Contrast Appearance Option"

---

# Tasks: High-Contrast Appearance Option

**Input**: Design documents from `/specs/017-high-contrast-appearance/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Requested explicitly (Constitution VIII, plan.md Testing section). Every implementation task is preceded by the test task(s) that pin its contract clauses; tests must go RED before the paired implementation task turns them GREEN.

**Organization**: Grouped by user story (spec.md US1–US5). Because FR-017 mandates **one selection site** for the whole axis, the four `Roles` tables, the persisted setting and the style-application plumbing are genuinely shared prerequisites for every story — they live in Phase 2 (Foundational), not duplicated per story. Each user-story phase then adds only what is specific to that story: paint-site changes, control wiring, or verification tests plus its manual scenario(s).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no unmet dependency — safe to run in parallel
- **[Story]**: US1–US5 per spec.md, present only on story-phase tasks
- Every task names its exact file path(s) and the contract clause(s) it pins (parenthesized, e.g. `H11`, `A5`, `M7`)

---

## Phase 1: Setup

**Purpose**: Confirm the baseline is green before any change (no new crate/dependency exists to scaffold — plan.md Scale/Scope: 0 new crates, 0 new dependencies).

- [X] T001 Confirm the pinned toolchain and a clean baseline: `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo build -p modplayer` then `rtk cargo test --workspace`, both green, before touching any file (quickstart.md §1–§2.1) — **DONE (2026-09-24)**: build finished clean (10.96s, 0 crates compiled fresh besides workspace crates), `cargo test --workspace` = 1805 passed, 11 ignored, 0 failed (142 suites, 145.61s)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The single selection site FR-017 requires — the persisted boolean, the four `Roles` tables, the two widths, the outline helpers, and the one `apply_tokens_for` — built test-first. **No user-story phase can start until this phase is green.**

**⚠️ CRITICAL**: This phase *is* most of the feature by design (research R1–R3, R10; Constitution X "one construction site"). Story phases 3–7 add only paint-site calls, UI wiring and verification on top of it.

**⚠️ Test-first (Constitution VIII)**: every RED task in this phase must be written and observed failing before the implementation task it gates. The `appearance.high_contrast` `SettingDescriptor` is deliberately **not** here — its only pinning clause is `A16`, whose test lives in Phase 4, so the descriptor is implemented there (T026) immediately after that test goes RED.

### Persisted setting (`modplayer-core`)

- [X] T002 [P] Write settings-model tests for the recovery table and independence (`A1`–`A9`, `A11`–`A13`) in `crates/modplayer-core/src/settings/model.rs` `#[cfg(test)]` — RED
- [X] T003 [P] Write the property-based round-trip test for an arbitrary `(Theme, bool)` pair (`A10`) beside `plugin_panels_round_trip_proptest` in `crates/modplayer-core/tests/settings.rs` — RED
- [X] T004 [P] Write the controller shadow-state tests — `high_contrast()` reads `false` by default and the persisted value at launch, `set_high_contrast(on)` updates the shadow state only (no write, no other field touched), and the pair is independent of `theme()`/`focus_policy()` — in `crates/modplayer-core/src/controller.rs`'s `#[cfg(test)]` module (`:4851`), mirroring how `theme`/`focus_policy` are exercised (data-model §1.5; the controller face of `A13`, and the halves of `A17`/`A19` that live in core) — RED
- [X] T005 Implement `AudioSettings.high_contrast: bool`, `RawAppearance.high_contrast: toml::Value` + `default_high_contrast()`, `InvalidField::HighContrast` + its `field_name()` arm, the `into_settings` recovery match, and `to_raw`'s `Value::Boolean` emission in `crates/modplayer-core/src/settings/model.rs` — turns T002/T003 GREEN; `SCHEMA_VERSION` (`:69`) stays `1` (`A9`)
- [X] T006 Add `high_contrast` shadow field + `high_contrast()` / `set_high_contrast()` to `PlaybackController`, mirroring `theme`/`focus_policy`, populated from `settings.high_contrast` at launch (`:808`), in `crates/modplayer-core/src/controller.rs` (data-model §1.5; depends T004, T005) — turns T004 GREEN

### Theme token plumbing (`modplayer-ui`)

- [X] T007 [P] Write the new `high_contrast.rs` suite covering the table set, the promotion, the divider, the focus ring, the selection-site recovery and allocation-free re-apply (`H1`–`H4`, `H11`–`H16`, `S1`, `S2`, `S4`, `S5`) in `crates/modplayer-ui/tests/high_contrast.rs` — RED
- [X] T008 [P] Extend `design_token_contrast.rs` with a new block over `[&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST]` for the 7:1/3:1/4.5:1 floors (`H17`, `H18`) — the five existing loops stay **verbatim** (research R14) — in `crates/modplayer-ui/tests/design_token_contrast.rs` — RED
- [X] T009 [P] Write the outline-**value** tests that gate the marker/overlay helpers: `MARKER_OUTLINE_WIDTH == 1.0` (`M2`), `marker_outline(r)` is `None` for both normal tables and `Some(Stroke::new(1.0, r.text_primary))` for both high-contrast tables (`M1`), the palette-independent floor over all eight `MARKER_PALETTE` entries (`M3`) — all in `crates/modplayer-ui/tests/markers.rs` — plus `overlay_outline(token, visuals)` covering exactly `Positive`/`Warning` in high contrast and nothing else in any style (`M11`) in `crates/modplayer-ui/tests/plugin_overlays.rs` — RED (no paint site is asserted here; those are `M4`–`M9`/`M12` in Phases 5–6)
- [X] T010 Add `Roles.divider_alpha: f32` and `Roles.high_contrast: bool` fields in `crates/modplayer-ui/src/theme/tokens.rs` (data-model §2.1)
- [X] T011 Add `LIGHT_HIGH_CONTRAST` / `DARK_HIGH_CONTRAST` static tables with the FR-009 swatches, `text_secondary == text_primary` (FR-005), `divider_alpha = 1.0` (FR-007); keep `LIGHT`/`DARK` field-for-field identical to today, including `divider_alpha = 0.08` and `high_contrast = false` (`H3`, the FR-002/US1-AS-3 regression net) in `crates/modplayer-ui/src/theme/tokens.rs` — do **not** write "every high-contrast role differs from normal" — dark `warning` is deliberately unchanged (research R4)
- [X] T012 Implement `for_theme(dark, hc)`, keep `for_dark_mode(dark) = for_theme(dark, false)`, and `is_high_contrast(&Visuals)` reading `weak_text_color == Some(noninteractive.fg_stroke.color)` with its precondition (`LIGHT.text_secondary != LIGHT.text_primary`, same for `DARK`) asserted; update `roles(&Visuals)` to call both, in `crates/modplayer-ui/src/theme/tokens.rs` (`H1`, `H2`, `H12`, `S1`, `S2`) — carries a runnable doc example (Constitution VII)
- [X] T013 Update `divider_color_for` to `text_primary.gamma_multiply(divider_alpha)` in `crates/modplayer-ui/src/theme/tokens.rs` (`H14`) — turns T007/T008's divider/table assertions GREEN together with T010–T012
- [X] T014 [P] Add `FOCUS_RING_WIDTH_HIGH_CONTRAST = 3.0`, `focus_ring_width(&Roles)` and update `focus_ring`'s body to use it, in `crates/modplayer-ui/src/theme/controls.rs` (`H16`) — `focus_ring_width` carries a runnable doc example (Constitution VII); `FOCUS_RING_GAP` untouched (`H8`)
- [X] T015 [P] Add `MARKER_OUTLINE_WIDTH = 1.0`, `marker_outline(&Roles) -> Option<Stroke>`, `overlay_outline(OverlayColor, &Visuals) -> Option<Stroke>` and `casing_width(f32) -> f32` in `crates/modplayer-ui/src/theme/markers.rs` (`M1`, `M2`, `M3`, `M11`) (depends T009, T010–T012) — turns T009 GREEN; no paint call site yet
- [X] T016 Change `style::build_style` to `build_style(theme, high_contrast)`, with its only internal change being `tokens::for_dark_mode(dark)` → `tokens::for_theme(dark, high_contrast)`; update its three test call sites to pass `false` explicitly, in `crates/modplayer-ui/src/theme/style.rs` (depends on T010–T014) — `no_geometry_or_interaction_field_changes` (`H8`) must still pass verbatim
- [X] T017 Widen `STYLES` to `OnceLock<[Arc<Style>; 4]>` (`[L, D, L-HC, D-HC]`), add `apply_tokens_for(ctx, high_contrast)`, keep `apply_tokens(ctx) = apply_tokens_for(ctx, false)` as a thin wrapper — the 25 existing test call sites to `apply_tokens` stay unmodified — in `crates/modplayer-ui/src/theme/mod.rs` (depends on T016) (`S4`, `S5`)
- [X] T018 Update the two production call sites — `App::new` and the first statement of `App::ui` — from `apply_tokens(ctx)` to `apply_tokens_for(ctx, controller.high_contrast())` in `crates/modplayer-ui/src/app.rs:125,223` (depends on T006, T017) — turns T007's `S1`/`S2`/`S4`/`S5` assertions and the table-set assertions fully GREEN

**Checkpoint**: `rtk cargo test -p modplayer-core -p modplayer-ui` green, including T002/T003/T004/T007/T008/T009. Foundation ready — every story phase below only adds to it.

---

## Phase 3: User Story 1 - Every text sample reads clearly (Priority: P1) 🎯 MVP

**Goal**: With high contrast on, every text sample measures ≥7:1; with it off, nothing changes (US1 AS-1–AS-3).

**Independent Test**: Sample any text element in any view against its background with the project's contrast tool; ratio ≥7:1 with high contrast on, unchanged from today with it off. Already true once Phase 2 lands (the promotion is a table value, not call-site code) — this phase is verification plus the manual walk.

### Tests for User Story 1

- [X] T019 [P] [US1] Verify/extend `high_contrast.rs` assertions for the promotion and its consumers (`H11`, `H13`) and unaffected text roles (`H6`, the `text_disabled`/FR-006 slice) in `crates/modplayer-ui/tests/high_contrast.rs`
- [X] T020 [P] [US1] Verify `design_token_contrast.rs`'s new text-floor block (`H17`'s `text_primary`/`text_secondary` rows) passes at ≥7:1 in both themes in `crates/modplayer-ui/tests/design_token_contrast.rs`

### Manual scenario for User Story 1

- [X] T021 [US1] Execute manual scenarios **M3** (text ≥7:1 across Library/Search/Now Playing/Settings, point-sampled) and **M4** (high contrast off ⇒ unchanged from today) per quickstart.md §3; record pass/deviation on this task and, if any deviation, write it back into quickstart.md/research.md — **DONE (2026-09-23), PASS with one deviation.** Ran against the real signed-in build (window formed, no keychain/sign-in gate hit this session), not a stub. Colorimetric point-sampling of Library/Search/Settings screenshots confirms every sampled title, secondary/help line, count label, duration and nav label promotes to `#1c1c1e` (ratio 17.01) with high contrast on, and to the unchanged `#5b5b60` with it off — M3/M4's core claim holds. Deviation: the last row of a capped search-results group (`GROUP_VISIBLE_ROWS`, `search_view.rs`) renders dimmed (~6.75:1) regardless of the high-contrast setting — a pre-existing, out-of-scope defect this feature's promotion merely makes measurable, not a regression it introduces; no file it lives in is named by any task in this plan. Full evidence: quickstart.md M3/M4 sign-off note, research.md R17.

**Checkpoint**: User Story 1 independently verified — MVP.

---

## Phase 4: User Story 2 - A live, persistent, independent setting (Priority: P1)

**Goal**: A checkbox next to the Theme combo, orthogonal to it, applies on the next frame, and survives a restart alongside the theme choice (US2 AS-1–AS-4).

**Independent Test**: Open Appearance, enable high contrast, confirm the current view repaints without restart, restart, confirm both choices restored exactly.

### Tests for User Story 2

- [X] T022 [US2] Write the checkbox's accessible-node test — `Role::CheckBox`, label from `tr("setting-high-contrast")`, `Toggled` state, not a fourth Theme-combo entry (`A14`) — in `crates/modplayer-ui/tests/high_contrast.rs` — RED
- [X] T023 [P] [US2] Add `setting-high-contrast` / `setting-high-contrast-desc` to `tests/fluent_keys.rs`'s Appearance inventory block (`A15`) in `crates/modplayer-ui/tests/fluent_keys.rs` — RED
- [X] T024 [US2] Write the search/focus test — search finds the `appearance.high_contrast` descriptor, arriving with `focus == Some("appearance.high_contrast")` focuses the checkbox, not the combo (`A16`) — in `crates/modplayer-ui/tests/high_contrast.rs` — RED
- [X] T025 [US2] Write the live-application tests — toggle persists via the reload-mutate-save block (`A17`), next frame uses the new tables with no intermediate frame on old ones (`A18`), restart restores both axes (`A19`), rapid toggling never mixes tables (`A20`) — in `crates/modplayer-ui/tests/high_contrast.rs` — RED

### Implementation for User Story 2

- [X] T026 [US2] Add the `appearance.high_contrast` `SettingDescriptor` row (category `Appearance`, `setting-high-contrast{,-desc}` keys) beside `appearance.theme` in `crates/modplayer-core/src/settings_registry.rs` (`A16`, the registry half; depends T024) — its pinning test is T024, which must be RED first
- [X] T027 [US2] Add `setting-high-contrast` / `setting-high-contrast-desc` to `locales/en-US/settings.ftl` after `setting-theme-dark` (depends T023) — turns T023 GREEN
- [X] T028 [US2] Add the checkbox (`switch(ui, SwitchKind::Checkbox, ..)`) below the Theme combo, its `focus == Some("appearance.high_contrast")` guard, and its persist branch (`set_high_contrast` + reload-mutate-save, raising `settings-save-failed` on write error like the theme branch) in `crates/modplayer-ui/src/settings/appearance.rs` (depends T006, T018, T022, T024, T025, T026) — turns T022/T024/T025 GREEN

### Manual scenarios for User Story 2

- [X] T029 [US2] Execute manual scenarios **M1** (control exists, independent of the combo), **M2** (live apply, no relaunch), **M9** (both axes survive a restart), **M10** (found by search, focus lands on checkbox), **M11** (malformed value recovers without collateral damage) per quickstart.md §3; record pass/deviation, writing back any deviation into quickstart.md/research.md — **DONE (2026-09-23), PASS, no deviation.** Real signed-in build (`rodrigo.zampieri`), driven via Quartz (`target/manual-walk/drive.py`) against scratch `MODPLAYER_CONFIG_DIR`s (not the real user config), screenshotted with `screencapture`. **M1**: Appearance screen shows the checkbox directly below the Theme combo (`M1-checkbox-off.png`); opening the combo lists exactly System/Light/Dark, no fourth entry (`22-combo-open.png`, `m9-05-combo-open.png`). **M2**: clicking the checkbox flips the switch and repaints the same running process immediately, no relaunch (`M2-live-apply-confirmed.png`, `m9-06-both-set.png` — `settings.toml` on disk updates to `high_contrast = true` in the same instant, confirming the reload-mutate-save write, not just the widget). **M9**: fresh scratch config, set Theme→Light and High contrast→on (both written to `settings.toml` live), quit (`kill`), relaunched pointing at the identical config dir — Settings›Appearance shows Theme still "Light" and High contrast still on (`m9-04-appearance.png` before vs. `m9-08-relaunch-appearance.png` after; Library's own chrome also repaints high-contrast-light on the very first frame, `m9-07-relaunch-initial.png`). **M10**: typing "contrast" into Settings' search box surfaces "Appearance › High contrast" (`23-search-contrast.png`); opening the result lands on Appearance with keyboard focus on the checkbox, and pressing Space toggles it (`24-after-result-click.png`, `M10-after-space.png`). **M11**: hand-built `settings.toml` with `high_contrast = "yes"` (malformed) alongside non-default `theme = "dark"`, `master_volume = 77` and a bogus `connect_device_id`; on launch the app starts, shows a "settings reset to defaults" toast, high contrast reads off, **and** theme stays "dark" and volume stays 77 unchanged (`m11-03-appearance.png`; the malformed device id recovers independently to a real device without touching theme/volume — the exact independent-per-field recovery `A5` requires). Full screenshots and configs under `target/manual-walk/` (`m9-*`, `m11-*`, gitignored).

**Checkpoint**: User Stories 1 AND 2 both independently verified.

---

## Phase 5: User Story 3 - Markers stay individually identifiable (Priority: P2)

**Goal**: Every palette-coloured host surface gains a 1px `text.primary` outline in high contrast, hue unchanged, focus-width difference preserved (US3 AS-1–AS-3).

**Independent Test**: Place markers of different palette colours, enable high contrast, confirm each stays distinguishable from its neighbours and the background.

### Tests for User Story 3

- [X] T030 [US3] Write the paint-**site** tests — outline presence/absence, unrecoloured fills, and focus-width preservation (`M4`–`M9`) — in `crates/modplayer-ui/tests/markers.rs` — RED for M4–M6, M9; M7/M8 pass immediately (see T033). The outline-value clauses `M1`–`M3` are already pinned by T009.

### Implementation for User Story 3

- [X] T031 [US3] Add casing/outline paint at the detail-lane glyph sites — `paint_bracket`, `paint_point_glyph`, `paint_cue_glyph` (O1–O3) — reading `theme::markers::marker_outline`/`casing_width`, in `crates/modplayer-ui/src/markers.rs` (depends T015, T030)
- [X] T032 [US3] Add casing/outline paint at the overview-lane marker line and clamped-marker warning (O4, O6) in `crates/modplayer-ui/src/markers.rs` (depends T015, T030)
- [X] T033 [US3] Add `rect_stroke` outline around the armed loop-region span shading (O5) in `crates/modplayer-ui/src/markers.rs`; add the swatch's inherited-outline regression test (`M7`, research R7 — **no paint code** at that call site) in `crates/modplayer-ui/tests/markers.rs`

### Manual scenario for User Story 3

- [X] T034 [US3] Execute manual scenario **M7** (load a track, place ≥4 markers incl. palette index 7 olive, one armed loop region; each stays identifiable, fills unchanged, focus preserved) per quickstart.md §3; record pass/deviation — **NOT EXECUTED (2026-09-23).** Built and launched the real, already-signed-in build; the macOS GUI session had locked between T029's earlier sign-off and this attempt (only `loginwindow`/`SecurityAgent`/`ScreenSaverEngine` on screen, no app window ever formed for 18+ seconds of polling) — killed the process rather than route around the lock, per Governance › Manual Scenario Sign-Off and quickstart.md §4. Full evidence: quickstart.md M7 sign-off note, research.md R18. The paint-site claims M7 covers are machine-checked this session by `crates/modplayer-ui/tests/markers.rs`'s new M4–M9 suite (T030); M7 itself should be re-attempted once the session is unlocked.

**Checkpoint**: User Story 3's automated coverage (T030–T033) is green;
its manual scenario (M7, T034) is **not reached** — the GUI session was
locked when attempted (research R18) — so "independently verified" per
quickstart.md §3 does not yet hold for US3. Re-attempt T034 once the
session is unlocked.

---

## Phase 6: User Story 4 - A docked plugin panel is never the one unreadable area (Priority: P2)

**Goal**: A docked plugin panel reads the identical applied style as host chrome, with zero plugin-side code change (US4 AS-1–AS-2).

**Independent Test**: Dock a plugin panel, enable high contrast, confirm panel text/controls match host chrome contrast, no plugin-side call made.

### Tests for User Story 4

- [X] T035 [US4] Write the plugin-overlay paint-site coverage (`M12`), host/plugin colour parity (`P4`), and the no-plugin-call assertion (`P5`) in `crates/modplayer-ui/tests/plugin_overlays.rs` — RED for M12, P4; `M11` is already pinned by T009 and green from T015; P1/P2/P3/P6 already covered by unmodified/extended existing suites

### Implementation for User Story 4

- [X] T036 [US4] Add outline handling at the four plugin overlay primitive arms — `Line`, `Region`, `Label`, host `Glyph` (O7–O10) — calling `theme::markers::overlay_outline`, in `crates/modplayer-ui/src/plugin_overlays.rs:97,118,151,163-192` (depends T015, T035)

### Manual scenario for User Story 4

- [X] T037 [US4] Execute manual scenario **M8** (dock a plugin panel, toggle high contrast, panel repaints on next frame matching host chrome, sampled ≥7:1) per quickstart.md §3; record pass/deviation — **NOT EXECUTED (2026-09-24).** Built `modplayer` fresh and launched it against a scratch config seeded from the real signed-in `hc-config` (`account.toml` carried over) with a `[plugin_panels."org.modplayer.section-loop/main"]` entry pre-set to docked/enabled so the panel would already be docked on the first frame — the macOS GUI session was locked (`CGSSessionScreenIsLocked = 1` confirmed directly; no window formed for this app or any other after 25+s of polling), the identical blocker T034/T039 hit (R18/R19). Killed the process rather than route around the lock, per Governance › Manual Scenario Sign-Off and quickstart.md §4. Full evidence: quickstart.md M8 sign-off note, research.md R21. The mechanism M8 covers is machine-checked this session by `plugin_overlays.rs`'s `M12` suite and `docked_plugin_panel_matches_host_chrome` (`P4`, T035/T036); M8 itself should be re-attempted once the session is unlocked.

**Checkpoint**: User Story 4's automated coverage (T035–T036) is green;
its manual scenario (M8, T037) is **not reached** — the GUI session was
locked when attempted (research R21), matching US3's own not-reached
state (R18) — so "independently verified" per quickstart.md §3 does not
yet hold for US4. Re-attempt T037 once the session is unlocked.

---

## Phase 7: User Story 5 - Structure stays visible, not just text (Priority: P3)

**Goal**: Borders/dividers reach ≥3:1 and the focus ring visibly thickens in high contrast (US5 AS-1–AS-2).

**Independent Test**: Sample a border/divider against its surface (≥3:1); move focus onto a control and confirm a visibly thicker ring.

### Tests for User Story 5

- [X] T038 [US5] Write the border/divider floor test (`H15`, plus `H17`'s divider rows) and the focus-ring width test (`H16`) in `crates/modplayer-ui/tests/high_contrast.rs` — both pass immediately given Phase 2 (T013, T014), so this task is verification, not TDD-red — **VERIFIED**: `every_border_follows_the_divider` (H15) and `focus_ring_thickens_only_in_high_contrast` (H16) already present in `high_contrast.rs` (added under T007), `high_contrast_divider_clears_the_non_text_floor` (H17 divider rows) already present in `design_token_contrast.rs` (added under T008); `rtk cargo test -p modplayer-ui --test high_contrast` (21 passed) and `--test design_token_contrast` (12 passed), both green, no new code needed

### Manual scenarios for User Story 5

- [X] T039 [US5] Execute manual scenarios **M5** (panel border, row divider, `Default` button outline all ≥3:1) and **M6** (focus ring measurably thicker, 3px vs 2px, in the high-contrast accent) per quickstart.md §3; record pass/deviation — **NOT EXECUTED (2026-09-24).** Built `modplayer` fresh and launched it against a scratch config (seeded from `hc-config-off`, onboarding pre-acknowledged); the macOS GUI session was still locked (screen-saver active, no window for any app after 23+s of polling — `Quartz.CGWindowListCopyWindowInfo` showed only `ScreenSaverEngine` plus non-GUI-session owners) — the identical blocker T034 hit a day earlier (R18), unrelated to this session's code. Killed the process rather than route around the lock, per Governance › Manual Scenario Sign-Off and quickstart.md §4. Full evidence: quickstart.md M5/M6 sign-off note, research.md R19. The values M5/M6 would visually confirm are machine-checked this session by `high_contrast.rs`'s `every_border_follows_the_divider` (H15) and `focus_ring_thickens_only_in_high_contrast` (H16), and `design_token_contrast.rs`'s `high_contrast_divider_clears_the_non_text_floor` (H17) — see T038; M5/M6 themselves should be re-attempted once the session is unlocked.

**Checkpoint**: User Story 5's automated coverage (T038) is green; its
manual scenarios (M5/M6, T039) are **not reached** — the GUI session was
locked when attempted (research R19) — so "independently verified" per
quickstart.md §3 does not yet hold for US5, matching US3's own
not-reached state (R18). Re-attempt T039 once the session is unlocked.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: The mechanical enforcement clauses (no literals, single selection site) and the full regression net, run once every story phase is in.

- [X] T040 [P] Write the source-scan test that `high_contrast` is named only in `crates/modplayer-ui/src/theme/**` plus `app.rs` and `settings/appearance.rs` (`S3`, FR-017's mechanical form) in `crates/modplayer-ui/tests/high_contrast.rs`
- [X] T041 [P] Confirm `design_token_literals.rs`'s `EXPECTED_BASELINE_HITS` stays `0` (`H19`) — unmodified file, run to verify only
- [X] T042 Run the full regression net unmodified per quickstart.md §2.3: `rtk cargo test --workspace`, and confirm the twelve individually-named tests/files (design_token_literals, design_token_roles, interaction_states, control_variants, control_inventory, accessibility, actions, controls, api_reference, `no_geometry_or_interaction_field_changes`, `apply_tokens_installs_both_themes`, `apply_tokens_is_idempotent`, `maps_every_theme_to_its_preference`, `overlay_color_maps_every_token`, `marker_palette_has_eight_distinct_colours`) all pass verbatim — **DONE (2026-09-24)**: `cargo test --workspace` = 1805 passed, 11 ignored, 0 failed (142 suites); the nine file-level suites all ran green inside that pass; the six individually-named tests reconfirmed directly via `cargo test -p modplayer-ui --lib` — `marker_palette_has_eight_distinct_colours`, `overlay_color_maps_every_token`, `apply_tokens_installs_both_themes`, `apply_tokens_is_idempotent`, `no_geometry_or_interaction_field_changes`, `maps_every_theme_to_its_preference` all `... ok`
- [X] T043 [P] Run `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo deny check` (zero dependency change expected)
- [X] T044 [P] Run `rtk cargo test -p modplayer-ui --doc` and confirm **new** doctests appear for `is_high_contrast` and `focus_ring_width` (Constitution VII)
- [X] T045 Re-run `python3 target/plan-scratch/contrast.py` and confirm every FR-009 candidate prints its spec value, light `danger` vs `surface.raised` printing the tightest value, **7.00** (quickstart.md §2.4)
- [X] T046 Consolidate the manual-scenario sign-off across T021, T029, T034, T037, T039 into quickstart.md/research.md — every deviation (or, per quickstart.md §4, every scenario recorded **not executed** with reason if the build could not be driven) written back before calling the feature done

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup; **blocks every user story** — T002→T005, T003 independent-but-precedes-T005, T004→T006 (the controller pair's RED test), T006 also needs T005; T018 needs T006; T007/T008 precede T010–T013 and T009 precedes T015; T010→T011→T012→T013 sequential (same file); T014/T015 parallel to T010–T013 (different files) but T016 needs all of T010–T014 done; T016→T017→T018 sequential
- **User Story phases (3–7)**: all depend on Phase 2 complete; independent of each other's implementation (different files each), so 3/4/5/6/7 can run in parallel once Phase 2 is green
- **Polish (Phase 8)**: depends on all five story phases

### Within Each Story Phase

- Tests before the implementation task(s) they gate (T019/T020 before nothing further — verification only; T022/T023/T024/T025 before T026–T028, and specifically T024 before T026, the descriptor's only pinning clause; T030 before T031–T033; T035 before T036)
- Manual scenario task last in each phase

### Parallel Opportunities

- Phase 2: T002/T003/T004 together; T007/T008/T009 together; T014/T015 together (after/alongside T010–T013, T015 also after T009, both before T016)
- Once Phase 2 checkpoint is green: Phases 3, 4, 5, 6, 7 can proceed in parallel (each touches its own files: `tests/high_contrast.rs` additions in 3/4/7 are additive blocks, not conflicting edits, but coordinate if run by parallel agents)
- Phase 8: T040/T041/T043/T044 together

---

## Parallel Example: Phase 2 kickoff

```bash
# Core tests, together:
Task: "Write settings-model recovery/independence tests (A1-A9,A11-A13) in crates/modplayer-core/src/settings/model.rs"
Task: "Write the (Theme,bool) proptest round-trip (A10) in crates/modplayer-core/tests/settings.rs"
Task: "Write the controller shadow-state tests (data-model 1.5) in crates/modplayer-core/src/controller.rs"

# Theme token tests, together:
Task: "Write high_contrast.rs table/promotion/divider/ring/selection tests (H1-H4,H11-H16,S1,S2,S4,S5)"
Task: "Extend design_token_contrast.rs with the high-contrast floor block (H17,H18)"
Task: "Write the outline-value tests (M1-M3 in tests/markers.rs, M11 in tests/plugin_overlays.rs)"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup
2. Phase 2: Foundational (the four tables, the persisted setting, the one selection site) — **this is the bulk of the work**
3. Phase 3: User Story 1 — verify text promotion + run M3/M4
4. **STOP and VALIDATE**: `rtk cargo test --workspace` green, M3/M4 recorded
5. This alone is not shippable without US2 (no control reaches the setting) — but it proves the core promise machine-checked before any UI is touched

### Incremental Delivery

1. Setup + Foundational → foundation ready (persisted axis + all four tables + selection site)
2. + US1 → verify text contrast → checkpoint
3. + US2 → the control exists, live-applies, persists → **first real end-to-end deliverable**
4. + US3 → markers stay identifiable
5. + US4 → plugin panels follow
6. + US5 → structure (borders, focus ring) visibly reinforced
7. Polish → full regression net, literal/selection-site scans, doctest check, manual sign-off consolidated

### Parallel Team Strategy

After Phase 2's checkpoint, up to five people can take one story phase each — 3/4/5/6/7 touch disjoint file sets (`settings_registry.rs`+`settings/appearance.rs`+`settings.ftl` for US2; `markers.rs` for US3; `plugin_overlays.rs` for US4; test-only for US1/US5) — and each is independently testable and independently sign-off-able per quickstart.md §3.

---

## Notes

- [P] tasks = different files, no unmet dependency
- [Story] label maps a task to its user story for traceability; Setup, Foundational and Polish carry none
- Constitution VIII (test-first): every RED task above must actually fail before its paired implementation task runs — 014's Phase 2 inverted this once and had to record it; do not repeat that here. Every implementation task in this file now names the RED task that gates it: T005←T002/T003, T006←T004, T010–T013←T007/T008, T014←T007, T015←T009, T026←T024, T027←T023, T028←T022/T024/T025, T031–T033←T030, T036←T035
- Do not write the forbidden assertion "every high-contrast role differs from its normal-mode value" (dark `warning` is deliberately identical, research R4)
- `MARKER_PALETTE`, `surface.base`/`surface.raised`, the type scale, hover/pressed alphas, and the cue digit's `text.on-accent` mark are all **out of scope** — no task above should touch them (FR-002, FR-006, FR-014, FR-020)
- Each manual-scenario task (T021, T029, T034, T037, T039) is executed by the implementing agent itself against the real build (Governance › Manual Scenario Sign-Off) — never handed back as a to-do, and never signed off from automated evidence alone if the window cannot be driven (quickstart.md §4)
