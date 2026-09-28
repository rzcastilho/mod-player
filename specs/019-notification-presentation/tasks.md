---

description: "Task list for feature implementation"
---

# Tasks: Notification Placement, Severity, and Message Quality

**Input**: Design documents from `/specs/019-notification-presentation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md (all present)

**Tests**: Included. Plan.md's Testing section and Constitution Principle VIII ("test-first for each FR") both require them; quickstart.md names every test file and command.

**Organization**: Tasks are grouped by user story (US1–US4, all from spec.md) to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no unmet dependency)
- **[Story]**: US1 / US2 / US3 / US4 — maps to spec.md's four user stories
- File paths are exact and relative to repo root

## Path Conventions

Cargo workspace, crate per component (per plan.md Project Structure):
- `crates/modplayer-core/{src,tests}/` — presentation-agnostic core (notifications, device policy, settings, controller)
- `crates/modplayer-ui/{src,tests}/` — placement, cap, styling, truncation, wiring
- `locales/en-US/{app,controls}.ftl` — Fluent strings

---

## Phase 1: Setup

**Purpose**: Confirm a clean baseline before any change.

- [X] T001 Confirm baseline is green: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `rtk cargo deny check` (all pass before any edit in this feature)

---

## Phase 2: Foundational (Blocking Prerequisites)

**None.** Each user story below is self-contained on top of current `main`/branch code: US1 only touches the `Area` anchor, and every later story's data-model/API addition is introduced inside that story's own phase (not shared blocking infra used by an earlier story). No cross-story prerequisite work is required before Phase 3 begins.

---

## Phase 3: User Story 1 - Notifications stop hiding what the user is working on (Priority: P1) 🎯 MVP

**Goal**: Re-anchor the notification stack bottom-right (8 px offset), proving zero overlap with headers/tab strips/category list/table headers/nav rail at 960×640 and 1200×820 (FR-001, FR-002).

**Independent Test**: Open the library with the tab strip and first row visible, raise three notifications, confirm the tab strip and first row stay fully visible and clickable.

### Tests for User Story 1 ⚠️

> Write first; must FAIL against the current top-right anchor.

- [X] T002 [US1] Create `crates/modplayer-ui/tests/notification_stack.rs`: headless test asserting the `shell-notifications` `Area` anchors `Align2::RIGHT_BOTTOM` at `(-8, -8)` (via `ctx.memory(|m| m.area_rect(id))`), plus the zero-intersection check (SC-001) — collapsed stack rect vs. library tab strip + first list row, settings category list, plugin table column headers, nav rail — at `960×640` and `1200×820` (contract S1, research R2)

### Implementation for User Story 1

- [X] T003 [US1] In `crates/modplayer-ui/src/app.rs`, change the `shell-notifications` `Area` anchor from `Align2::RIGHT_TOP, vec2(-8.0, 8.0)` to `Align2::RIGHT_BOTTOM, vec2(-8.0, -8.0)`; keep the same `Area` id, `Order` and draw position in `update` (research R1, contract S1)
- [X] T004 [US1] Run `rtk cargo test -p modplayer-ui --test notification_stack` and confirm T002 passes; if the worst-case geometry margin from research R2 is tight, adjust only the test fixture, not the anchor

**Checkpoint**: Stack renders bottom-right; SC-001 automated check passes for plain (pre-cap, pre-styling) cards. Story 1 is independently shippable.

---

## Phase 4: User Story 2 - Notifications don't pile up past a usable limit (Priority: P1)

**Goal**: Cap the collapsed stack at the three newest non-dismissed notifications with an "{N} more" / "Show fewer" control; expanded stack scrolls within 60% of window height (FR-003–FR-006).

**Independent Test**: Raise three notifications, confirm all three show with no overflow control; raise a fourth, confirm exactly three cards plus a "1 more" control, and activating it reveals the fourth.

### Tests for User Story 2 ⚠️

> Write first; must FAIL (types/functions don't exist yet).

- [X] T005 [P] [US2] Add `proptest` in `crates/modplayer-ui/src/notifications.rs` for `partition(visible_len, expanded) -> StackPartition` invariants: `shown ≤ visible_len`; collapsed ⇒ `shown ≤ 3`; `overflow == visible_len.saturating_sub(3)` (research R3, contract S9)
- [X] T006 [US2] Extend `crates/modplayer-ui/tests/notification_stack.rs` with SC-002 (3 notifications ⇒ no button; 4th ⇒ 3 cards + "1 more" below oldest shown; activate ⇒ 4 cards + "Show fewer"; activate again ⇒ collapse; dismiss down to 3 while expanded ⇒ button disappears, stack collapses, nothing lost) and the 60% max-height scroll cap (S3) (contract S2–S3, FR-003–FR-006)
- [X] T007 [P] [US2] Extend `crates/modplayer-ui/tests/fluent_keys.rs`: `notification-more` (plural, `$count`) and `notification-show-fewer` resolve non-empty

### Implementation for User Story 2

- [X] T008 [US2] In `crates/modplayer-ui/src/notifications.rs`, add `pub const MAX_COLLAPSED_CARDS: usize = 3;`, `pub fn partition(visible_len: usize, expanded: bool) -> StackPartition`, `pub fn card_width(screen_width: f32) -> f32`, and `pub struct StackState { expanded: bool, show_more: BTreeSet<u64>, details: BTreeSet<u64> }` (fields used by later stories; only `expanded` is read/written here) (data-model §6, research R3/R5, contract S9)
- [X] T009 [US2] [depends: T008] In `crates/modplayer-ui/src/notifications.rs`, render `center.visible().take(shown)` newest-first; when `overflow > 0 ∧ ¬expanded` show the "{N} more" button below the oldest shown card (`notification-more`, `$count = overflow`); when `expanded` show "Show fewer" (`notification-show-fewer`) outside a `ScrollArea::vertical().max_height(0.6 × screen_rect().height()).auto_shrink([false, true])`; force `expanded = false` when `overflow == 0` (contract S2–S3, research R3–R4)
- [X] T010 [P] [US2] Add `notification-more = { $count -> [one] { $count } more *[other] { $count } more }` and `notification-show-fewer = Show fewer` to `locales/en-US/app.ftl` (contract fluent-strings.md)
- [X] T011 [US2] In `crates/modplayer-ui/src/app.rs`, add `notification_stack: notifications::StackState` field to `ModPlayerApp`, initialize it, and pass `&mut self.notification_stack` into `notifications::show` at the call site
- [X] T012 [US2] Run `rtk cargo test -p modplayer-ui --test notification_stack --test fluent_keys` and confirm T005–T007 pass

**Checkpoint**: Stack caps at 3, "{N} more"/"Show fewer" works, expanded stack scrolls within 60%. Stories 1+2 both independently shippable.

---

## Phase 5: User Story 3 - Severity is legible without reading the words (Priority: P2)

**Goal**: Each card carries a 4 px left accent bar and icon in the severity's theme role colour (`danger`/`warning`/`positive`), plus the severity word as text (FR-007, FR-008).

**Independent Test**: Raise one critical and one informational notification together; confirm they're distinguishable by accent bar colour and icon alone, without reading the message.

### Tests for User Story 3 ⚠️

> Write first; must FAIL (cards are still uniform grey).

- [X] T013 [P] [US3] Extend `crates/modplayer-ui/tests/notifications.rs` (SC-003): Critical vs. Info card accent bar/icon colours equal `Roles::danger` / `Roles::positive` respectively (and differ), icon glyphs differ (`⛔` vs `ℹ`), each in light/dark/high-contrast themes (017); Warning uses `Roles::warning`
- [X] T014 [P] [US3] Extend `crates/modplayer-ui/tests/accessibility.rs`: each card's accessible text contains its severity word (`severity-critical`/`severity-warning`/`severity-info`), never conveyed by colour/icon alone
- [X] T015 [P] [US3] Extend `crates/modplayer-ui/tests/design_token_literals.rs` scan (or confirm existing scan already covers) `crates/modplayer-ui/src/notifications.rs` for zero colour literals in the new accent-bar/icon code

### Implementation for User Story 3

- [X] T016 [US3] In `crates/modplayer-ui/src/notifications.rs`, add `fn severity_color(roles: &Roles, s: Severity) -> Color32` (`Info → positive`, `Warning → warning`, `Critical → danger`) reading `theme::tokens::roles(ui.visuals())`; render each card as `Frame::new().fill(roles.surface_raised)` with `inner_margin.left` enlarged by 4 px, paint a 4 px full-height rect on the frame's left edge via `painter.rect_filled` (left corners rounded to the card radius) in the severity colour, and colour the existing icon glyph (`⛔`/`⚠`/`ℹ`) with the same role, keeping the severity word as visible text (research R7, contract S4, FR-007/FR-008)
- [X] T017 [US3] Run `rtk cargo test -p modplayer-ui --test notifications --test accessibility --test design_token_literals` and confirm T013–T015 pass

**Checkpoint**: Severity is visually distinct via colour + icon + text in all themes. Stories 1–3 independently shippable.

---

## Phase 6: User Story 4 - Notification text reads as guidance, not a log line (Priority: P2)

**Goal**: Rewrite device-availability wording to short human sentences (no raw id), add an optional `detail` payload behind "Details", truncate every message to two lines with "Show more", persist `output_device_name`, hold Info auto-dismiss while expanded (FR-009–FR-013, FR-016).

**Independent Test**: Launch with a confirmed output device no longer connected; confirm the visible text is a short sentence naming the device and fallback, ≤2 lines, with the raw device id available only after "Details".

### Tests for User Story 4 ⚠️

> Write first; must FAIL (fields/functions/keys don't exist or old raw-id wording still applies).

- [X] T018 [P] [US4] Add inline `#[cfg(test)]` unit tests in `crates/modplayer-core/src/notifications.rs` for `Notification.detail`, `hold_auto_dismiss`/`release_auto_dismiss`, and refined `tick` rules H1–H5 (Info dismissed iff `!held && now − auto_dismiss_from ≥ 10s`; Warning/Critical unaffected; hold/release no-op on unknown/dismissed ids; explicit `dismiss*` still dismisses a held notification); confirm the existing `info_auto_dismisses_after_ten_seconds` test still passes unchanged (contract C1–C3, data-model §1)
- [X] T019 [P] [US4] Extend `crates/modplayer-core/tests/notifications.rs` (integration) for raise-site changes (C4): `handle_device_lost` → `raise_with_detail(Critical, device-lost, [device, fallback], lost_id)`; `MissingPreferred` path → `raise_with_detail` with resolved name/generic phrase + fallback + id (or `raise_with_args` when no id); `handle_device_list_changed` → `raise_with_detail(Info, device-available-again, [device], found.id)`; `keybindings-invalid-entries` → `raise_with_detail(Warning, …, [], ids.join(", "))`; assert severity/trigger/action set unchanged per notification (FR-017)
- [X] T020 [P] [US4] Add `crates/modplayer-core/tests/device_policy.rs` tests for `display_name_for_saved(id, saved_name)`: `saved_name` (trimmed, non-empty) wins; else `name:<name>` id's remainder; else `None`; `proptest` — for arbitrary id not in `name:` form and `saved_name = None`, output is `None` and never any other substring of the raw id (data-model §4)
- [X] T021 [P] [US4] Extend `crates/modplayer-core/tests/settings.rs`: `output_device_name` round-trip via the existing settings `proptest` strategy; a pre-019 fixture file (no key) loads with `output_device_name == None` and re-saves without adding the key; empty/whitespace-only value loads as `None` with no `InvalidField`/warning; `keybindings-invalid-entries` assertion moved from `args` to `detail` (contract K1–K4, research R10–R11)
- [X] T022 [P] [US4] Extend `crates/modplayer-ui/tests/notifications.rs`: SC-004 (collapsed device text ≤2 lines at 360 px card width for device names up to 40 chars, contains no substring of the raw `DeviceId`, raw id shown only after "Details"), "Show more"/"Show less" toggle (S5) present iff the 2-row galley is elided, "Details"/"Hide details" toggle (S6) present iff `detail.is_some()`, SC-006 (every existing action — Sign in, Open status page, Retry, Open upgrade page, Restart plugin, Disable plugin — still dispatches from the new layout)
- [X] T023 [US4] Extend `crates/modplayer-ui/tests/notification_stack.rs`'s worst-case SC-001 fixture (from T002) to include, per card, two actions, a "Details" toggle, a "Show more" toggle and a 40-character device name (research R2), and confirm zero intersection still holds at 960×640/1200×820
- [X] T024 [P] [US4] Extend `crates/modplayer-ui/tests/accessibility.rs`: "Show more"/"Show less", "Details"/"Hide details" buttons have non-empty externalized names; a card's message-label accessible name equals the full, untruncated resolved text regardless of truncation
- [X] T025 [P] [US4] Extend `crates/modplayer-ui/tests/credential_leak.rs` to scan rendered "Details" text for credential leakage (Constitution VI)
- [X] T026 [P] [US4] Extend `crates/modplayer-ui/tests/fluent_keys.rs`: `notification-show-more`, `notification-show-less`, `notification-details`, `notification-hide-details`, `notification-device-unknown`, `notification-device-fallback-default` resolve non-empty; `DEVICE_NAMED_KEYS` updated for the new `$fallback` arg; `keybindings-invalid-entries` moves from `CONTROLS_WARNING_ARG_KEYS` to the plain-keys list; SC-005 (`device-lost`/`device-missing-at-launch` resolved text contains both device names)

### Implementation for User Story 4

- [X] T027 [US4] In `crates/modplayer-core/src/notifications.rs`, add `detail: Option<String>` (public), `auto_dismiss_from: Instant` (`pub(crate)`, `= created_at` at raise), `held: bool` (`pub(crate)`, default `false`); add `raise_with_detail(severity, message_key, args, detail: String) -> u64` (debug-assert `!detail.is_empty()`; release stores `None` for empty); add `hold_auto_dismiss(&mut self, id: u64)` and `release_auto_dismiss(&mut self, id: u64, now: Instant)`; refine `tick(now)` to rules H1–H5; every existing `raise*`/`raise_keyed`/`raise_attributed` sets `detail: None` (contract C1–C3, data-model §1)
- [X] T028 [US4] [depends: T027] In `crates/modplayer-core/src/device_policy.rs`, extend `DeviceWarning::MissingPreferred` to `{ device_name: Option<String>, device_id: Option<DeviceId>, fallback_name: String }`; add `pub fn display_name_for_saved(id: Option<&DeviceId>, saved_name: Option<&str>) -> Option<String>` (resolution order: `saved_name` trimmed non-empty → it; else `id` in `name:<name>` form → remainder; else `None`); add a `saved_name: Option<&str>` parameter to `resolve(...)` and thread it into `MissingPreferred` construction (contract C5, data-model §3–4)
- [X] T029 [P] [US4] In `crates/modplayer-core/src/settings/model.rs`, add `AudioSettings.output_device_name: Option<String>` (default `None`) and `RawAudio.output_device_name` under `[audio]` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; empty/whitespace-only values load as `None` (no `InvalidField`) (contract K1–K4, K7, data-model §5)
- [X] T030 [US4] [depends: T028, T029] In `crates/modplayer-core/src/controller.rs`, update `confirm_device` to look up the `OutputDeviceInfo` **before** saving and persist `info.name` into `output_device_name` alongside `output_device` in the same save (unchanged if the device isn't in the list); add a `preferred_device_name` shadow next to `preferred_device` for `launch()` (research R10, contract K3)
- [X] T031 [US4] [depends: T027, T028, T030] In `crates/modplayer-core/src/controller.rs`, update raise sites per contract C4: `handle_device_lost` → `raise_with_detail(Critical, device-lost, [device, fallback = fallback.name or tr(notification-device-fallback-default)], lost_id.to_string())`; `raise_device_warning(MissingPreferred)` → resolve `device_name` via `display_name_for_saved` (map `None` to `tr(notification-device-unknown)`) and call `raise_with_detail(Warning, device-missing-at-launch, [device, fallback], device_id.to_string())`, or `raise_with_args` with the same `args` when no id exists; `handle_device_list_changed` → `raise_with_detail(Info, device-available-again, [device], found.id.to_string())`; `raise_settings_warning(InvalidKeybindings)` → `raise_with_detail(Warning, keybindings-invalid-entries, vec![], ids.join(", "))` — severity/trigger/action set unchanged for every site (FR-017)
- [X] T032 [P] [US4] In `locales/en-US/app.ftl`, rewrite `device-lost`, `device-missing-at-launch`, `device-available-again` per contract fluent-strings.md wording; add `notification-show-more`, `notification-show-less`, `notification-details`, `notification-hide-details`, `notification-device-unknown`, `notification-device-fallback-default`
- [X] T033 [P] [US4] In `locales/en-US/controls.ftl`, rewrite `keybindings-invalid-entries` to drop `$ids` per contract fluent-strings.md
- [X] T034 [US4] [depends: T008, T009] In `crates/modplayer-ui/src/notifications.rs`, lay out each card's message as an `egui::text::LayoutJob` with `TextWrapping { max_width: text_width, max_rows: 2, break_anywhere: false, overflow_character: Some('…') }` (`text_width = card_width(...) − accent(4) − inner margins − icon column`); read `galley.elided` to show "Show more" (`notification-show-more`) iff elided, toggling `state.show_more` and re-laying-out with `max_rows: usize::MAX` when shown (label reads "Show less"); the label's AccessKit node always carries the full, untruncated text; for plugin-attributed notifications keep the attribution icon + name on the first row and cap only the text after them (research R6, contract S5)
- [X] T035 [US4] [depends: T027, T034] In `crates/modplayer-ui/src/notifications.rs`, render a "Details"/"Hide details" toggle (`notification-details`/`notification-hide-details`) iff `notification.detail.is_some()`, toggling `state.details`; when shown, render `detail` as `Label::new(RichText::new(detail).monospace()).selectable(true).wrap()` below the message, independent of "Show more" (contract S6, research R13)
- [X] T036 [US4] [depends: T034, T035] In `crates/modplayer-ui/src/notifications.rs`, compute `NotificationInteraction::hold_changes: Vec<(u64, bool)>` — `(id, true)` when an `Info` card's "Show more" or "Details" turns on this frame, `(id, false)` when both turn off; in `crates/modplayer-ui/src/app.rs`, apply each as `center.hold_auto_dismiss(id)` / `center.release_auto_dismiss(id, Instant::now())` after `notifications::show` returns (contract S7, research R8)
- [X] T037 [US4] Run `rtk cargo test -p modplayer-core --lib notifications --test notifications --test device_policy --test settings` and `rtk cargo test -p modplayer-ui --test notifications --test notification_stack --test accessibility --test credential_leak --test fluent_keys`; confirm T018–T026 all pass

**Checkpoint**: Device wording is human-readable with no raw id, Details/Show more work, Info hold/release honours WCAG 2.2.1, settings round-trip. All four stories independently shippable.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Full-suite verification, documentation, and the manual scenario sign-off the constitution requires.

- [X] T038 [P] Add doc comments with runnable examples for every new public item: `raise_with_detail`, `hold_auto_dismiss`, `release_auto_dismiss`, `display_name_for_saved`, `partition`, `card_width`, `StackState` (Constitution VII)
- [X] T039 Run the full automated gate: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `rtk cargo deny check`
- [X] T040 Execute manual scenarios M1–M11 from `specs/019-notification-presentation/quickstart.md` (placement at two sizes, cap/overflow, promotion, severity legibility + high contrast, device-missing named/legacy, truncation, device lost/available-again, actions preserved, keybindings detail); record each scenario's pass/fail result in this file under a "Manual Scenario Results" heading — **NOT EXECUTED (2026-09-25).** Built `modplayer` fresh (`RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer`) and launched it in the background against the real config dir (no `MODPLAYER_CONFIG_DIR` override). `ps` confirmed the process stayed alive and sleeping (`SN`) for 29 s; `Quartz.CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)` polled twice (at ~3 s and ~23 s) and never listed a `modplayer`-owned window among the 25/14 on-screen windows — only `SecurityAgent` (the lock prompt), `loginwindow`, `Dock`, `Window Server`, etc. `Quartz.CGSessionCopyCurrentDictionary()` confirmed `CGSSessionScreenIsLocked = 1`. This is the identical blocker recorded in 017's T037/T039 (research R18/R19/R21): a locked macOS GUI session never forms a window for a newly launched app, so `screencapture`/`CGEventPost` have nothing to target. Killed the process rather than route around the lock, per Governance › Manual Scenario Sign-Off — the recipe is macOS-host-only and assumes a live, unlocked session. M1–M11 are unreached this session; every mechanism they would visually confirm already has automated coverage from Phases 3–6 (SC-001–SC-006, C1–C5, K1–K7, S1–S9 per contracts/, all green under T039's full-workspace run — 1923 passed, 0 failed). Re-attempt T040 once the session is unlocked.
- [X] T041 Confirm the plan.md post-design Constitution Check still holds (no new crate/trait/flag; only additive cross-crate API growth) — no action expected if T039 is green — **CONFIRMED (2026-09-25)**: T039 is green; `git diff --stat babc20d..HEAD -- '**/Cargo.toml'` across every commit of this feature (checkpoint through US4) is empty — zero `Cargo.toml` changes anywhere in the workspace, so no new crate/dependency was added. API growth is additive only: `NotificationCenter::raise_with_detail`/`raise_with_actions_and_detail`/`hold_auto_dismiss`/`release_auto_dismiss`, `Notification.detail`, `device_policy::display_name_for_saved` + `resolve`'s new `saved_name` param, `AudioSettings.output_device_name`, and `modplayer_ui::notifications::{partition, card_width, StackState, StackPartition}` — no existing signature removed or narrowed, no new trait, no new feature flag. No action needed.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies
- **Foundational (Phase 2)**: None — proceed directly to Phase 3
- **User Story 1 (Phase 3)**: Depends on Setup only
- **User Story 2 (Phase 4)**: Depends on Setup only; independent of US1's content but edits the same `app.rs`/`notifications.rs` files US1 touched, so do it after US1 lands to avoid a merge conflict, not because of a data dependency
- **User Story 3 (Phase 5)**: Depends on Setup only; same file-conflict-avoidance rationale as US2 — sequence after US2
- **User Story 4 (Phase 6)**: Depends on Setup only for its *core* tests/impl (T018–T021, T027–T031); its *UI* truncation/Details/hold work (T034–T036) builds on `StackState`/`partition`/`card_width` from US2 (T008) and the card frame from US3 (T016), so sequence US4 after US2 and US3
- **Polish (Phase 7)**: Depends on all four user stories being complete

### User Story Dependencies

- **US1 (P1)**: No dependencies on other stories
- **US2 (P1)**: No data dependency on US1; sequenced after it to avoid file conflicts in `app.rs`/`notifications.rs`
- **US3 (P2)**: No data dependency on US1/US2; sequenced after US2 to avoid file conflicts in `notifications.rs`
- **US4 (P2)**: Core changes (device_policy, settings, controller) are independent; its UI changes reuse `StackState`/`partition`/`card_width` (US2) and the card frame (US3), so it must follow both

### Within Each User Story

- Tests written and failing before implementation (Constitution VIII)
- Core (`modplayer-core`) data/API changes before controller raise-site changes that use them
- Core changes before the UI code that depends on them (US4)
- Locale key additions can run in parallel with the Rust code that will reference them
- Story's verification task last

### Parallel Opportunities

- T018, T019, T020, T021 (US4 core tests, four different files) run in parallel
- T022, T024, T025, T026 (US4 UI tests, four different files) run in parallel; T023 touches the same file as T022's predecessor (T002/T006) so run it after T022
- T029 (settings model) runs in parallel with T027 (core notifications) and T028 (device_policy) — three different files
- T032, T033 (locale files) run in parallel with each other and with any Rust task
- T038 (doc comments) runs in parallel with T039 (gate run)

---

## Parallel Example: User Story 4 core tests

```bash
# Four different files, no shared state — launch together:
Task: "Inline unit tests for detail/hold/tick in crates/modplayer-core/src/notifications.rs"
Task: "Integration tests for raise-site changes in crates/modplayer-core/tests/notifications.rs"
Task: "display_name_for_saved + proptest in crates/modplayer-core/tests/device_policy.rs"
Task: "output_device_name round-trip + legacy fixture in crates/modplayer-core/tests/settings.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup
2. Phase 3: User Story 1 (anchor change + SC-001 geometry test)
3. **STOP and VALIDATE**: `rtk cargo test -p modplayer-ui --test notification_stack`; confirm the library tab strip / settings category list / plugin table headers are never covered
4. This alone fixes `UX-05` (P0) — ship it even before cap/severity/wording land

### Incremental Delivery

1. Setup → baseline green
2. US1 → stack out of the way → validate → shippable
3. US2 → cap at three + overflow control → validate → shippable
4. US3 → severity legible by colour/icon → validate → shippable
5. US4 → human wording, truncation, Details, `output_device_name` → validate → shippable
6. Polish → full gate + manual M1–M11 sign-off

### Notes

- [P] tasks touch different files with no unmet dependency
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently
- `crates/modplayer-ui/src/notifications.rs` and `crates/modplayer-ui/tests/notification_stack.rs` are each edited across US1→US2→US3→US4 by design (one widget, incrementally enhanced) — sequence those tasks in ID order even within "independent" stories to avoid conflicting edits

---

## Manual Scenario Results

*(Filled in during Phase 7, T040 — one line per scenario: M1–M11, pass/fail, evidence path under `target/manual-walk/019/`.)*

All of M1–M11: **NOT EXECUTED (2026-09-25)** — macOS GUI session was locked
(`CGSSessionScreenIsLocked = 1`) for the whole attempt window; the launched
`modplayer` process stayed alive but formed no window (`Quartz.
CGWindowListCopyWindowInfo` never listed a `modplayer`-owned window across
two polls, ~3 s and ~23 s after launch). No evidence PNGs produced — there
was no window for `screencapture` to target. See T040's note above for the
full account and research precedent (017 R18/R19/R21). Every mechanism
M1–M11 would visually confirm is covered by this feature's automated
suites (all green — see T039): `notification_stack.rs` (SC-001 placement,
SC-002 cap/overflow, S3 60% scroll cap), `notifications.rs` (SC-003
severity colours ×3 themes, SC-004 truncation/no-id-substring, S5/S6
toggles, SC-006 actions), `fluent_keys.rs` (SC-005, new/rewritten keys),
`accessibility.rs`, `credential_leak.rs`, and the core `notifications`/
`device_policy`/`settings` suites (C1–C5, K1–K7). Re-attempt M1–M11 once
the session is unlocked.
