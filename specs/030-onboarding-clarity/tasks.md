---

description: "Task list for 030-onboarding-clarity"
---

# Tasks: Onboarding and Sign-In Clarity

**Input**: Design documents from `/specs/030-onboarding-clarity/`

**Prerequisites**: plan.md, spec.md, research.md (R1–R15), data-model.md, contracts/onboarding-screens.md, contracts/fluent-strings.md, quickstart.md

**Tests**: INCLUDED. plan.md (Constitution VIII) requires every contract rule (M, W, N, P, S, D, V, G, A, K, F) to map to a named test written before the implementation (red → green). The new integration test file is `crates/modplayer-ui/tests/onboarding_clarity.rs`.

**Organization**: Grouped by user story. UX items: UX-07 to UX-12. Spec clarifications C1–C16 and FR-001–FR-014 are cited per task. Each task also cites, after "— Source:", the docs/ModPlayer-Software-Specification.md IDs it implements (FR-1.1.1, FR-1.1.2, FR-1.1.3, FR-1.2.1, FR-1.2.3, FR-1.2.4, FR-1.3.1, FR-1.4.1, NFR-6.1, NFR-6.2, NFR-7.1), following the FR → source mapping in spec.md › Requirements.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4, mapping to spec.md
- All commands run from the worktree root with `env -u RUSTUP_TOOLCHAIN` (quickstart.md Prerequisites).

## Path Conventions

Cargo workspace. Paths are relative to the repo root.

- UI: `crates/modplayer-ui/src/`, tests in `crates/modplayer-ui/tests/`
- Core: `crates/modplayer-core/src/`
- Strings: `locales/en-US/`
- **Must not change**: `crates/modplayer-account/src/launch_flow.rs`, `locales/en-US/disclosure.ftl` (FR-014, C2)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add every new en-US string and the new test file, so all stories compile and resolve keys.

- [X] T001 [P] Add `privacy-back-to-welcome = Back to welcome` and `privacy-back-to-about = Back to About` to `locales/en-US/app.ftl`, using the comment and wording in contracts/fluent-strings.md (C5) — Source: FR-1.1.1, NFR-7.1
- [X] T002 [P] Add `signin-waiting-indicator`, `signin-browser-hint`, `signin-waiting-elapsed-seconds` and `signin-waiting-elapsed-minutes` (Fluent `[one]` / `*[other]` on `$count`), plus `signin-summary-title`, `signin-summary-identity`, `signin-summary-continue` and `signin-summary-switch`, to `locales/en-US/account.ftl`, with exact text from contracts/fluent-strings.md (C6, C7, C16) — Source: FR-1.2.1, FR-1.2.4, NFR-7.1
- [X] T003 [P] Add `device-check-back = Back` to `locales/en-US/device-check.ftl` (C10, C16) — Source: FR-1.3.1, NFR-7.1
- [X] T004 [P] Add `setting-show-getting-started`, `setting-show-getting-started-desc` and `setting-show-getting-started-visible` to `locales/en-US/settings.ftl` (C14) — Source: FR-1.4.1, NFR-7.1
- [X] T005 Extend `crates/modplayer-ui/tests/fluent_keys.rs` (F1–F4): — Source: NFR-7.1, FR-1.1.3
  - add an `ONBOARDING_030_KEYS` list of every argument-free new key, each resolving to a non-key value;
  - add `tr_args` cases: `signin-waiting-elapsed-seconds` with count 1 and 2, `signin-waiting-elapsed-minutes` with count 1 and 2, and `signin-summary-identity` with name "Alex";
  - assert no new key is defined in `disclosure.ftl`;
  - include the three `setting-show-getting-started*` keys in the settings.ftl exercised-keys check.
  Depends on T001–T004.
- [X] T006 Create `crates/modplayer-ui/tests/onboarding_clarity.rs` with its shared harness: — Source: NFR-6.1, NFR-6.2
  - use `tests/common` (`active_account`) and `FakeBackend`, `SyntheticHost`, `MemorySecureStore`, `FakeAuthorizationService`;
  - add a headless `egui::Context` runner at 960×640 and 1600×900 with AccessKit enabled and an injectable `RawInput::time`;
  - add helpers for finding nodes by role or name and for asserting prose-node width ≤ `theme::body_measure(&ctx) + 1.0` (M1, M2, research R13).
  The file compiles with no tests yet.
- [X] T007 Run `git diff --exit-code main -- crates/modplayer-account/src/launch_flow.rs locales/en-US/disclosure.ftl` and `cargo test -p modplayer-account` to record a clean baseline (W6, FR-014) — Source: FR-1.1.3, FR-1.2.1, FR-1.2.3

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Small shared pieces that more than one story needs.

**⚠️ CRITICAL**: US1–US4 wait on this phase.

- [X] T008 Confirm the workspace builds and the existing UI tests pass before any behaviour change: `cargo test -p modplayer-ui` and `cargo test -p modplayer-core`. Record failures that already exist, so they are not blamed on this feature. — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3, FR-1.2.1, FR-1.2.3, FR-1.2.4, FR-1.3.1, FR-1.4.1, NFR-6.1, NFR-6.2, NFR-7.1
  - Baseline note: first run had one failure, `controller_section_loop::loop_toggle_requests_focus_and_arms_same_action` (flaky). Two reruns with `--no-fail-fast` passed: 1810 passed, 0 failed.

**Checkpoint**: strings resolve, the test harness exists, and the baseline is clean.

---

## Phase 3: User Story 1 - Understand and decide on the welcome screen (Priority: P1) 🎯 MVP

**Goal**: A heading, one lead paragraph, one framed secondary-text legal/Premium block with real links, exactly one Primary control, and an origin-aware privacy back control (UX-07, UX-08).

**Independent Test**: `cargo test -p modplayer-ui --test onboarding_clarity welcome_` and `... privacy_` (quickstart A1, A2). Manual M1–M3.

### Tests for User Story 1 (write first, confirm they FAIL)

- [X] T009 [P] [US1] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `welcome_*` tests at both sizes: — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3
  - W1 order: heading, then lead, then compact block, then action row;
  - W2: one framed group, three `SECONDARY` lines, terms and privacy nodes with `Role::Link`;
  - W3: exactly one `Variant::Primary` control (`welcome-acknowledge`) and `welcome-decline` as `Variant::Default` to its right;
  - W4: Tab order via `ctx.memory(|m| m.focused())` (research R15);
  - W5: acknowledge records `DisclosureAcknowledgement::now(DISCLOSURE_BUNDLE_VERSION)` and returns `WelcomeOutcome::Acknowledged`, and decline writes nothing;
  - M1/M2 for the disclosure view.
- [X] T010 [P] [US1] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `privacy_*` tests: — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3
  - N1: back label per `PrivacyOrigin::Welcome` and `PrivacyOrigin::About`;
  - N2: click returns `true`;
  - N3: `settings.disclosure` is identical before and after;
  - N4: body satisfies M1 at 1600 px from both origins.

### Implementation for User Story 1

- [X] T011 [US1] In `crates/modplayer-ui/src/privacy_notice.rs`, add `pub enum PrivacyOrigin { Welcome, About }` (`Copy`, doc comment with a runnable example) and change `show(ui, origin) -> bool` to draw `privacy-back-to-welcome` or `privacy-back-to-about`. Cap the body at `theme::body_measure` (C1, C5, FR-002, FR-004). — Source: FR-1.1.1, FR-1.1.2, FR-1.2.1, FR-1.3.1
- [X] T012 [US1] In `crates/modplayer-ui/src/welcome.rs`, restructure the disclosure view (C2, C3, C4, FR-001, FR-003): — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3, NFR-6.2
  - order: `welcome-title` heading, `welcome-description` lead, one `Frame::group` containing `disclosure-unofficial`, `disclosure-premium-required` and `disclosure-terms-apply` in `SECONDARY` / `text_secondary`, then a row with the terms and privacy links (`egui::Hyperlink` / `egui::Link`, `Role::Link`, no button fill);
  - action row: `welcome-acknowledge` as `Variant::Primary` first, then `welcome-decline` as `Variant::Default`;
  - cap every prose block at `body_measure`;
  - pass `PrivacyOrigin::Welcome` to the privacy notice;
  - leave the Acknowledge and Decline outcomes unchanged.
- [X] T013 [US1] In `crates/modplayer-ui/src/settings/about.rs`, pass `PrivacyOrigin::About` to `privacy_notice::show` (C5). The restore row is added in US4, so keep this edit minimal. Depends on T011. — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3
- [X] T014 [US1] Update the existing `privacy_notice::show` call sites and the tests that use `welcome` or the privacy notice (`crates/modplayer-ui/tests/first_launch.rs` and any others the compiler flags) so they compile with the new signature. — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3
- [X] T015 [US1] Run `cargo test -p modplayer-ui --test onboarding_clarity welcome_ privacy_` plus `--test first_launch` and `cargo test -p modplayer-account`, then confirm the `disclosure.ftl` / `launch_flow.rs` diff is empty (W5, W6). — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3

**Checkpoint**: Story 1 is complete and testable on its own. This is the MVP.

---

## Phase 4: User Story 2 - Know what sign-in is doing and why (Priority: P1)

**Goal**: A pending view with a spinner, browser hint and elapsed hint, Primary "Open the browser again" with Default "Cancel", and every sign-in reason drawn as one shared status message above the actions (UX-11, UX-12).

**Independent Test**: `cargo test -p modplayer-ui --test onboarding_clarity pending_` and `... status_` (A3, A4). Manual M4, M5.

### Tests for User Story 2 (write first, confirm they FAIL)

- [X] T016 [P] [US2] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `pending_*` tests: — Source: FR-1.2.1, FR-1.2.3, NFR-6.2
  - P1: order of heading, spinner, browser hint, elapsed hint, resumed line, then actions;
  - P2: `ProgressIndicator` node named `signin-waiting-indicator` on the first frame;
  - P3: elapsed hint at 0, 1, 59, 60, 119 and 120 s via injected `RawInput::time`;
  - P4: `repaint_delay` ≤ 1 s;
  - P5: a new `attempt_id`, or leaving and re-entering `Authorizing`, resets the count;
  - P6: `signin-open-again` is Primary and first, `signin-cancel` is Default and second, with their behaviour unchanged (`AccountEvent::BrowserUrlReady`, `cancel_sign_in()`);
  - P7: no timeout is added.
- [X] T017 [P] [US2] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `status_*` tests: — Source: FR-1.2.1, FR-1.2.3, NFR-6.2
  - S2: for each row of the coverage table, exactly one `Role::Status` node with the given text, and zero such nodes for `SignedOut{note: None}`, `Authorizing`, `Checking`, the summary view and the device check;
  - S3: the status node's `bounds.y1` ≤ every action button's `y0` and ≥ the heading's `y1`;
  - S4: `signin-explanation` is a plain label drawn after the status node;
  - S5: Retry, `retry_store_read`, `recheck_tier` and Continue behaviour is unchanged;
  - M1 for each branch.
- [X] T018 [P] [US2] Add a unit test for `sign_in::elapsed_hint(secs)` in `crates/modplayer-ui/src/sign_in.rs`'s test module (plural boundaries, as data-model §1.3) and a paint test for `widgets::status_message` in `crates/modplayer-ui/src/widgets/status.rs`. — Source: FR-1.2.1, FR-1.2.3, NFR-6.2

### Implementation for User Story 2

- [X] T019 [P] [US2] Create `crates/modplayer-ui/src/widgets/status.rs` with `pub fn status_message(ui: &mut Ui, text: &str) -> Response` (S1, C8): — Source: FR-1.2.1, FR-1.2.3, NFR-6.2
  - a framed row (divider stroke, `radius::SM`);
  - a leading bar `space::XS` wide filled with `roles.warning`;
  - text in the `body` style at `roles.text_primary`, capped at measure;
  - AccessKit `Role::Status` with `Live::Polite` and the text as its name;
  - theme roles only, no colour literals;
  - a doc comment with a runnable example.
  Export it from `crates/modplayer-ui/src/widgets/mod.rs`.
- [X] T020 [US2] In `crates/modplayer-ui/src/sign_in.rs`, add `authorizing_since: Option<(u64, f64)>` to `SignInScreen` with the data-model §1.2 transitions evaluated at the start of every `show` frame (clamp elapsed at 0, never persist), and add `pub fn elapsed_hint(secs: u64) -> String` (R1, R2, C7). Both carry doc comments with runnable examples. — Source: FR-1.2.1, FR-1.2.3, NFR-6.2
- [X] T021 [US2] In `crates/modplayer-ui/src/sign_in.rs`, rebuild the pending view (`show_authorizing`) per P1 (C6, FR-005, FR-006): — Source: FR-1.2.1, NFR-6.2
  - an `egui::Spinner` labelled via `WidgetInfo::labeled(WidgetType::ProgressIndicator, true, tr("signin-waiting-indicator"))` (R6);
  - `signin-browser-hint`;
  - the elapsed hint;
  - `signin-waiting-resumed` when `resumed`;
  - actions `signin-open-again` as `Variant::Primary`, then `signin-cancel` as `Variant::Default`;
  - `ctx.request_repaint_after(Duration::from_secs(1))` (R3);
  - prose capped at `body_measure`.
  Depends on T020.
- [X] T022 [US2] In `crates/modplayer-ui/src/sign_in.rs`, route every reason branch through `widgets::status_message`, in the order heading → status → instructions → actions (R4, R5, C8, C9, FR-007). The branches are: — Source: FR-1.2.3, NFR-6.2
  - each `SignInNote`;
  - the probe-failure view;
  - `StoreUnreadable`, where `signin-store-unavailable{store}` and the remedy line form **one** status message;
  - the Free and Unknown tier results.
  Keep the Retry, `retry_store_read`, `recheck_tier` and Continue behaviour unchanged. Depends on T019.
- [X] T023 [US2] Update `crates/modplayer-ui/tests/section_memory.rs` and any other `SignInScreen` callers the compiler flags (plan: Extended tests). — Source: FR-1.2.1, FR-1.2.3, NFR-6.2
- [X] T024 [US2] Run `cargo test -p modplayer-ui --test onboarding_clarity pending_ status_` and `cargo test -p modplayer-ui sign_in`, then `cargo test -p modplayer-account` (P7, FR-014). — Source: FR-1.1.3, FR-1.2.1, FR-1.2.3

**Checkpoint**: Stories 1 and 2 both work on their own.

---

## Phase 5: User Story 3 - Go back from the device check and understand the buffer choice (Priority: P2)

**Goal**: A Back control on the device check (to Settings › Audio, or to a sign-in summary during first launch), and a labelled, explained buffer preset field (UX-09).

**Independent Test**: `cargo test -p modplayer-ui --test onboarding_clarity device_check_` and `... summary_` (A5, A6). Manual M6, M7.

**Depends on**: US2 for the edits to `sign_in.rs` (the same file). `widgets::status_message` is not needed here.

### Tests for User Story 3 (write first, confirm they FAIL)

- [X] T025 [P] [US3] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `device_check_*` tests: — Source: FR-1.3.1, FR-1.2.4
  - D1: a `Variant::Default` Back (`device-check-back`) in the normal and zero-devices layouts, from either origin, keyboard-focusable;
  - D2: Back from Settings returns `DeviceCheckOutcome::Back`, calls `skip_device_check()`, shows Settings › Audio next, and leaves `device_confirmed`, `buffer_preset` and the preferred device unchanged;
  - D3: Back during launch calls no controller method, the next frame shows the summary, and `device_check` keeps the same `selected` and `preset`;
  - D4: the preset combo is drawn through `field::row` with label `setting-buffer-preset`, the same accessible name, help `setting-buffer-preset-desc` directly below within measure, and unchanged preset labels;
  - D5: the order, keys, variants and calls of the answers are unchanged: `confirm_device` + `Closed`, `preview_device`, `skip_device_check` + `Closed`;
  - M1 for the normal and zero-devices layouts.
- [X] T026 [P] [US3] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `summary_*` tests: — Source: FR-1.3.1, FR-1.2.4
  - V1: order of heading, identity line (`display_name`, or `account-identity-unavailable` when blank), then Primary Continue and Default Switch;
  - V2: a before/after snapshot of `account.toml`, the secure store, `settings.toml` and the controller device state is equal;
  - V3: Continue clears `sign_in_summary` and the device check shows with the stored selection;
  - V4: Switch opens the `signout-confirm-*` modal with Cancel focused first. Confirm leads to `SignedOut`, step `SignIn`, flag cleared and `signin-title` shown. Cancel, Esc and backdrop keep the summary;
  - V5: the same rules hold for `SessionState::Expired`;
  - M1.

### Implementation for User Story 3

- [X] T027 [US3] In `crates/modplayer-ui/src/device_check.rs`: — Source: FR-1.3.1, FR-1.2.4
  - add `DeviceCheckOrigin { LaunchFlow, Settings }` and `DeviceCheckOutcome { Open, Closed, Back }`;
  - add the origin to `DeviceCheckScreen::new`, and make `show` return `DeviceCheckOutcome` instead of `bool`;
  - draw a `Variant::Default` Back (`device-check-back`) above the device list in both layouts (D1);
  - on `Settings` origin, Back calls `controller.skip_device_check()`; on `LaunchFlow` origin it calls no controller method;
  - Back never confirms a device;
  - keep the question, tone button, radio list and the Yes / "No, try another" / "Skip for now" row unchanged (C12).
  New pub items carry doc comments with runnable examples.
- [X] T028 [US3] In `crates/modplayer-ui/src/device_check.rs`, draw the buffer-preset combo with `settings::field::row` using `FieldSpec::new("device_check.buffer_preset", tr("setting-buffer-preset"))`, `help = tr("setting-buffer-preset-desc")` and `ResetState::Hidden`, mirroring `settings/audio.rs`. Keep `from_id_salt("device-check-buffer-preset")` and the preset label format. Cap prose at measure (R11, C11, FR-009). Same file as T027, so do it after T027. — Source: FR-1.3.1, NFR-6.2
- [X] T029 [P] [US3] In `crates/modplayer-ui/src/settings/account.rs`, change `identity_text`, `open_sign_out_modal` and `show_sign_out_modal` to `pub(crate)` with no behaviour change (R10, plan Complexity Tracking). — Source: FR-1.3.1, FR-1.2.4
- [X] T030 [US3] In `crates/modplayer-ui/src/sign_in.rs`, add `pub fn show_summary(...) -> SummaryOutcome` and `pub enum SummaryOutcome { None, Continue, Events(Vec<AccountEvent>) }` (V1–V5, C16, data-model §1.7): — Source: FR-1.3.1, FR-1.2.4
  - heading `signin-summary-title`;
  - identity line `signin-summary-identity { $name }`, with the `account-identity-unavailable` fallback when the name is blank or the account is `None`;
  - row with `signin-summary-continue` (`Variant::Primary`) then `signin-summary-switch` (`Variant::Default`);
  - Switch reuses the existing sign-out confirmation (`signout-confirm-*`);
  - nothing is persisted.
  Depends on T029 and on US2's edits to the same file (T020–T022).
- [X] T031 [US3] In `crates/modplayer-ui/src/app.rs` (data-model §1.6): — Source: FR-1.3.1, FR-1.2.4
  - add `sign_in_summary: bool`, default `false`;
  - construct `DeviceCheckScreen` with `LaunchFlow` in `App::new` and `show_launch_gate_device_check`;
  - handle `DeviceCheckOutcome`: `Back` on `LaunchFlow` sets the flag; `Back` on `Settings` drops the screen so Settings › Audio shows;
  - at each frame start, clear the flag when `launch_step() != DeviceCheck`;
  - while the flag is set and the step is `DeviceCheck`, draw `sign_in::show_summary` instead of the device check;
  - route `SummaryOutcome::Continue` and `Events` (through `handle_account_event`);
  - do not touch `launch_flow::next_step`.
  Depends on T027 and T030.
- [X] T032 [US3] In `crates/modplayer-ui/src/settings/audio.rs`, construct `DeviceCheckScreen::new(.., DeviceCheckOrigin::Settings)` for "Test output device" (D2). — Source: FR-1.3.1, FR-1.2.4
- [X] T033 [US3] Update the existing device-check call sites and tests from `bool` to `DeviceCheckOutcome` (`crates/modplayer-ui/tests/first_launch.rs`, any other device-check tests the compiler flags), keeping them green (D5). — Source: FR-1.3.1, FR-1.2.4
- [X] T034 [US3] Run `cargo test -p modplayer-ui --test onboarding_clarity device_check_ summary_`, `--test first_launch` and `--test settings_account`. Confirm `launch_flow.rs` has no diff (V6). — Source: FR-1.3.1, FR-1.2.4

**Checkpoint**: Stories 1–3 work on their own.

---

## Phase 6: User Story 4 - Scan the getting-started card and bring it back (Priority: P2)

**Goal**: The card keeps one line per bundled plugin and the shortcut and tutorial links, with no chord prose. Settings › About gets a searchable "Show getting-started card" button (UX-10).

**Independent Test**: `cargo test -p modplayer-ui --test onboarding_clarity getting_started_ about_restore_` and `cargo test -p modplayer-core settings_registry restore_getting_started` (A7, A8). Manual M8.

### Tests for User Story 4 (write first, confirm they FAIL)

- [X] T035 [P] [US4] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `getting_started_*` tests: — Source: FR-1.4.1
  - G1: nodes are the title heading, exactly one line per bundled plugin (`getting-started-section-loop`, `getting-started-key-tempo`), links `getting-started-shortcuts` and `getting-started-tutorial` with `Role::Link`, and button `getting-started-dismiss`;
  - G2: no text node contains "Shortcuts:" or any host action's default chord label from 029's `key_label` on either platform;
  - G3: activating the shortcuts link returns `GettingStartedOutcome::OpenShortcuts`.
- [X] T036 [P] [US4] In `crates/modplayer-ui/tests/onboarding_clarity.rs`, add `about_restore_*` tests: — Source: FR-1.4.1
  - A1: `field::row` id `about.show_getting_started` with a button labelled `setting-show-getting-started`;
  - A2: enabled when dismissed is `true`; disabled with description `setting-show-getting-started-visible` when `false`;
  - A3: click yields `AboutOutcome::RestoreGettingStarted`, `getting_started_dismissed()` becomes `false`, a fresh `SettingsStore::load()` agrees, and the card shows;
  - A5: two interactions from Settings;
  - A6: persistence across a rebuilt controller, both for dismiss and for restore.
- [X] T037 [P] [US4] In `crates/modplayer-core/src/settings_registry.rs`'s unit tests, add an A4 test that `search("getting")` returns `about.show_getting_started`, and update `placeholder_only_categories_contribute_no_descriptors` to drop `About` (data-model §3). In `crates/modplayer-core/src/controller.rs`'s tests, add a `restore_getting_started` test covering the shadow flag, persistence and idempotence. — Source: FR-1.4.1

### Implementation for User Story 4

- [X] T038 [P] [US4] In `crates/modplayer-core/src/controller.rs`, add `PlaybackController::restore_getting_started()`: set the shadow to `false`, then `persist_settings(|s| s.getting_started_dismissed = false)`. On a save failure raise the existing `settings-save-failed` (Warning) and keep the shadow `false`, mirroring `dismiss_getting_started`. No `unwrap`/`expect`. Add a doc comment with a runnable example (R12, C14). — Source: FR-1.4.1
- [X] T039 [P] [US4] In `crates/modplayer-core/src/settings_registry.rs`, add a `SettingDescriptor` with category `SettingsCategory::About`, id `about.show_getting_started`, `title_key` `setting-show-getting-started` and `description_key` `setting-show-getting-started-desc`. Keep `every_descriptor_id_is_unique` green. — Source: FR-1.4.1
- [X] T040 [US4] In `crates/modplayer-ui/src/settings/about.rs`, add `pub enum AboutOutcome { None, RestoreGettingStarted }` and a `field::row` with id `about.show_getting_started`. The button uses the label `setting-show-getting-started` and the help `setting-show-getting-started-desc`. It is disabled, with accesskit description `setting-show-getting-started-visible`, while the card is visible. Accept the dismissed flag as a parameter. Depends on T013 (the same file). — Source: FR-1.4.1
- [X] T041 [US4] In `crates/modplayer-ui/src/settings/mod.rs`, pass `getting_started_dismissed` to the About screen and map `AboutOutcome::RestoreGettingStarted` to `controller.restore_getting_started()`. Depends on T038 and T040. — Source: FR-1.4.1
- [X] T042 [P] [US4] In `crates/modplayer-ui/src/getting_started.rs`, fix the doc comment so it carries no chord prose. The card content stays unchanged (C13). — Source: FR-1.4.1
- [X] T043 [US4] Run `cargo test -p modplayer-ui --test onboarding_clarity getting_started_ about_restore_`, `cargo test -p modplayer-core` and `cargo test -p modplayer-ui --test settings_search_highlight`. — Source: FR-1.4.1

**Checkpoint**: All four stories work on their own.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Accessibility sweep, theme checks, quality gates and manual sign-off.

- [X] T044 [P] Extend `crates/modplayer-ui/tests/accessibility.rs`'s sweep (K1, K2): the privacy link and both privacy back labels, device-check Back, summary Continue and Switch, the About restore button, the pending spinner and the status message. Each has a non-empty accessible name, is Tab-reachable and activates with Space or Enter. — Source: NFR-6.1, NFR-6.2
- [X] T045 [P] Extend `crates/modplayer-ui/tests/high_contrast.rs` so the status bar and text stay legible in both high-contrast themes, and confirm `design_token_literals.rs` stays green (S1). — Source: NFR-6.2
- [X] T046 Run the full gates from quickstart.md §1, one at a time. Fix anything they flag, without suppressing lints: — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3, FR-1.2.1, FR-1.2.3, FR-1.2.4, FR-1.3.1, FR-1.4.1, NFR-6.1, NFR-6.2, NFR-7.1
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace`
  - `cargo test --workspace --doc`
  - `cargo deny check`
- [X] T047 Run `git diff --exit-code main -- crates/modplayer-account/src/launch_flow.rs locales/en-US/disclosure.ftl`, then `cargo test -p modplayer-account` and the targeted scenarios A1–A11 in quickstart.md §2 (W6, V6, FR-014). — Source: FR-1.1.3, FR-1.2.1, FR-1.2.3
- [X] T048 Manual Scenario Sign-Off: run M1–M9 from quickstart.md §3 on macOS (real Keychain, live Premium account, `MODPLAYER_CONFIG_DIR=$(mktemp -d)`, screenshots in `target/manual-walk/030-Mx-*.png`). Record pass or deviation for each scenario **in this task's notes** below, and record any difference from the spec in `specs/030-onboarding-clarity/research.md`. — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3, FR-1.2.1, FR-1.2.3, FR-1.2.4, FR-1.3.1, FR-1.4.1, NFR-6.1, NFR-6.2, NFR-7.1
- [X] T049 Final traceability check: every contract rule (M, W, N, P, S, D, V, G, A, K, F) has a named passing test, and every new pub item has a doc comment with a runnable example (Constitution VII, VIII). — Source: FR-1.1.1, FR-1.1.2, FR-1.1.3, FR-1.2.1, FR-1.2.3, FR-1.2.4, FR-1.3.1, FR-1.4.1, NFR-6.1, NFR-6.2, NFR-7.1
- [X] T050 Build the launch-flow device check only while the launch gate is `DeviceCheck`: drop the eager build in `App::new`, add `app::retains_device_check` and `DeviceCheckScreen::origin`, and drop a `LaunchFlow` screen whenever the step leaves `DeviceCheck`. Test `launch_device_check_is_dropped_outside_the_gate` in `crates/modplayer-ui/tests/onboarding_clarity.rs` (research R16). — Source: FR-1.3.1

### Manual scenario evidence (fill in during T048)

| Scenario | Result | Evidence |
|---|---|---|
| M1 Welcome at minimum size | Pass (note) | `030-M1-welcome-min.png`. Order heading → lead → framed block with two links; one Primary button. Decline is `Variant::Default` (grey fill), which matches C4/FR-003; the quickstart's "outlined" wording is loose. Lead line is ~87 glyphs but within measure (72 × `'0'` advance, contract M1). Default window opens at 1200 × 820; 960 × 640 is the minimum, so the window was resized for this capture. |
| M2 Wide window | Pass | `030-M2-wide.png`: column and block keep the same width at 1600 px. |
| M3 Privacy back | Pass | `030-M3-privacy.png`, `030-M3-back-welcome.png` ("Back to welcome" returns to unchanged Welcome); `030-M3-about-privacy.png` ("Back to About"). |
| M4 Pending sign-in | Not run | The browser already had a Spotify session with consent, so OAuth finished in under 4 s and the pending view never stayed on screen. Needs a browser profile that is signed out of Spotify. |
| M5 Status messages | Not run | Depends on M4 (Cancel during pending). The locked-Keychain step was not run: `security lock-keychain` locks the operator's whole login keychain. |
| M6 Device check Back during launch | **Deviation** | `030-M6-safe.png` → `030-M6-summary.png`. The tier check never finished (`tier = "unknown"`, empty name; Re-check subscription changed nothing and logged nothing), so `next_step` returned `Main`, not `DeviceCheck`. The `LaunchFlow` device check that `App::new` builds was drawn as the `show_main` overlay, and its Back dropped the screen and went to Library instead of the C16 summary (`app.rs` `show_main`). See research.md R16. **Fixed by T050**: the stray overlay is gone, but M6 itself still needs a Premium-verified launch to re-run. |
| M7 Device check Back from Settings | Pass (partial) | `030-M7-audio.png`, `030-M7-changed.png`, `030-M7-back.png`: Back returns to Settings › Audio with fields unchanged; "Buffer preset" label plus explanation. No device was confirmed beforehand because of the M6 tier issue. |
| M8 Getting-started card | Pass (note) | `030-M8-card.png`, `030-M8-shortcuts.png`, `030-M8-relaunch-hidden.png`, `030-M8-about-restored.png`, `030-M8-card-back.png`. Dismiss persisted (`getting_started_dismissed = true`), hidden after relaunch, restore set it back to `false`, card returned, button disabled. At 960 px About sits under "More", so restore takes 3 interactions, not ≤ 2 (C14). See research.md R16. Relaunch after restore checked through `settings.toml` only. |
| M9 Keyboard-only pass | Pass (partial) | `030-M9-tab1..3.png`, `030-M9-privacy-backfocus.png`, `030-M9-back-enter.png`: Tab order terms → privacy → Continue (→ Decline), ring visible, Space opens privacy, Enter on "Back to welcome" returns. M6 by keyboard not run (blocked as above). |

> 2026-10-13 walk on macOS 12.6, real Keychain and live account. T048 signed off by the operator with these known exceptions (see research.md R16):
> - **M4, M5 not run**: the browser already had Spotify consent, so the pending view never stayed on screen. The locked-Keychain step was skipped on purpose.
> - **M6 not re-run**: the tier check never returns Premium in this environment, so the launch gate never reaches `DeviceCheck`. The stray overlay it exposed is fixed (T050).
> - **C14 restore takes 3 interactions at 960 px** (About under "More"). Accepted as is.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: starts immediately. T005 needs T001–T004.
- **Foundational (Phase 2)**: needs Phase 1 and blocks all stories.
- **US1 and US2 (both P1)**: independent of each other after Phase 2. They touch different files, apart from the sign-in callers in `tests/`.
- **US3 (P2)**: shares `sign_in.rs` with US2, so start it after T020–T022. It shares `app.rs` with nothing else.
- **US4 (P2)**: independent of US2 and US3. It edits `settings/about.rs` after US1's T013.
- **Polish (Phase 7)**: after all desired stories.

### Within Each Story

- Tests first, and confirm they fail.
- Types and widgets before the screens that use them.
- Screens before `app.rs` wiring, then call-site updates and the targeted test run.

### Story Completion Order

US1 → US2 → US3 → US4 is the sequential order. US1, US2 and US4's core/registry tasks (T037–T039) can run in parallel.

## Parallel Opportunities

- Phase 1: T001, T002, T003 and T004 touch different .ftl files.
- US1: T009 and T010 (the same test file, so write them in separate edits).
- US2: T016, T017 and T018; T019 runs alongside the tests.
- US3: T025 and T026; T029 alongside T027.
- US4: T035, T036 and T037; then T038, T039 and T042 (different files).
- Polish: T044 and T045.

### Parallel example: User Story 4

```text
Task: "Add restore_getting_started() in crates/modplayer-core/src/controller.rs"      (T038)
Task: "Add about.show_getting_started descriptor in crates/modplayer-core/src/settings_registry.rs"  (T039)
Task: "Fix doc comment in crates/modplayer-ui/src/getting_started.rs"                 (T042)
```

## Implementation Strategy

### MVP First (User Story 1)

1. Phase 1 Setup, then Phase 2.
2. Phase 3 (US1): welcome hierarchy, link roles, a single Primary, and origin-aware privacy back.
3. Stop and validate with A1, A2 and M1–M3.

### Incremental Delivery

1. Add US2 (sign-in clarity), the other P1 story. Validate with A3, A4, M4, M5.
2. Add US3 (device-check Back and summary view). Validate with A5, A6, M6, M7.
3. Add US4 (getting-started restore). Validate with A7, A8, M8.
4. Polish, gates, and manual sign-off (T044–T049).

## Notes

- [P] tasks touch different files with no incomplete dependency.
- Do not edit `disclosure.ftl` or `launch_flow.rs`. The hash pin and `next_step` stay as they are (FR-014).
- Only `App` calls `open_url`.
- en-US strings only. pt-BR falls back (C15).
- Commit after each task or logical group.
