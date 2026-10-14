# Implementation Plan: Onboarding and Sign-In Clarity

**Branch**: `030-onboarding-clarity` (git: `feature/030-onboarding-clarity`) | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/030-onboarding-clarity/spec.md`

## Summary

This feature makes the four launch-flow screens and the getting-started card legible and recoverable. It covers UI/UX review items UX-07 to UX-12. Only presentation, copy and feedback change. The authorization protocol, disclosure text, hash and version, the Premium check and `launch_flow::next_step` stay exactly as they are (FR-014).

- **Welcome** (US1):
  - The screen reads heading → one lead paragraph → one framed, secondary-text legal/Premium block. Terms and Privacy become real links (`egui::Hyperlink` / `egui::Link`, `Role::Link`).
  - "I understand, continue" is the only `Variant::Primary` control. "Decline" becomes `Variant::Default`.
  - `disclosure.ftl` is untouched, so the existing SHA-256 pin holds (C2).
  - The privacy notice takes a `PrivacyOrigin` and its back control reads "Back to welcome" or "Back to About" (C5).
- **Sign-in** (US2):
  - The pending view gains an accessible spinner, a browser hint and an elapsed hint ("Waiting for N seconds" → "N minutes", with Fluent plurals). The hint is driven by egui's input clock, so headless tests inject time through `RawInput::time` (research R1).
  - Every reason the sign-in step can be showing is drawn by one new `widgets::status_message`: a `SignInNote`, store unreadable or unavailable, or a Free/Unknown tier result. It has a warning-role bar, a frame, and `Role::Status` + `Live::Polite`. It sits between the heading and the instructions, so always above Retry/Continue (C8, R4–R5).
- **Device check** (US3):
  - A Back control is added. Opened from Settings › Audio, Back returns there with skip semantics.
  - During first launch, Back opens a non-destructive "Signed in as {name}" summary view, held as a UI-only `App::sign_in_summary` flag over the unchanged `next_step`. From there, "Continue" returns to the device check, and "Use a different account" reuses 028's sign-out confirmation (C16, R9–R10).
  - The buffer combo is drawn through 028's `field::row` with the Settings label and explanation (C11, R11).
- **Getting-started** (US4):
  - The card content is re-pinned by a no-chord-prose test (C13).
  - Settings › About gains a searchable "Show getting-started card" button backed by a new `PlaybackController::restore_getting_started()`. The button is disabled while the card is visible (C14, R12).

**Recorded assumptions and rejected alternatives** (no question left open):

- The elapsed-hint start time is UI-only and keyed by `attempt_id`. Adding a timestamp to `SessionState::Authorizing` was rejected because of FR-014.
- The sign-in summary is an `App` flag, not a new `LaunchStep`. A new step was rejected because `next_step` must not change.
- The device-check Back is drawn above the content rather than in the Yes / No / Skip row, so the answer row stays byte-for-byte as specified (C12).
- About gets its first registry descriptor. This edits the `placeholder_only_categories_contribute_no_descriptors` test (see Complexity Tracking).
- The store-unreadable reason and its remedy line form *one* status message, not two, so there is one live-region announcement.

## Technical Context

**Language/Version**: Rust 1.95.0 (`rust-toolchain.toml`; workspace `rust-version = "1.95"`)

**Primary Dependencies**: All already in the workspace. No new crate dependency.

- egui/eframe 0.36: `Spinner`, `Frame`, `Link`, `Hyperlink`, `Modal`, `WidgetInfo`, AccessKit `Role::Status` / `Live::Polite` / `Role::Link`, `InputState::time`, `request_repaint_after`.
- fluent-templates, through `modplayer_core::{tr, tr_args}`. Plural selection uses the `count` argument.
- Existing crates: `modplayer-account` (`SessionState`, `AccountService::{sign_out, browser_url, cancel_sign_in}`) and `modplayer-core` (`PlaybackController`, `settings_registry`).

**Storage**: No new persisted field. Restore rewrites the existing `[onboarding] getting_started_dismissed` through `persist_settings` (atomic `settings.toml` save). The elapsed start, the privacy and device-check origins and the summary flag are UI-only and never persisted.

**Testing**: `cargo test`, with headless `egui::Context` + AccessKit node inspection at 960×640 and 1600×900 (the `tests/search_view.rs` measure precedent). Injected time comes through `RawInput::time`. Fixtures are `FakeBackend`, `SyntheticHost`, `MemorySecureStore` and `FakeAuthorizationService` (`tests/common::active_account`).

- **New**: `crates/modplayer-ui/tests/onboarding_clarity.rs`, plus unit tests for `elapsed_hint` and the `status_message` paint.
- **Extended**:
  - `fluent_keys.rs`, `accessibility.rs` (new controls in the sweep), `high_contrast.rs` / `design_token_literals.rs` (status bar colours);
  - `first_launch.rs`, and the existing device-check call sites (`bool` → `DeviceCheckOutcome`);
  - `section_memory.rs` (uses `SignInScreen`);
  - core `settings_registry` unit tests, and a core controller test for `restore_getting_started`.

**Target Platform**: macOS, Windows and Linux desktop. Behaviour is identical on all three. The only platform-specific text is the existing store remedy line. Manual scenarios run on macOS per the constitution recipe.

**Project Type**: Desktop application (Cargo workspace, one crate per component).

**Performance Goals**: No measurable UI cost. At most one extra repaint request per second while sign-in is pending, which the existing ≥30 Hz loop already covers. No audio-thread code is touched.

**Constraints**:

- Prose is capped at `theme::body_measure`.
- Theme roles only, no colour literals, legible in high contrast.
- Every new control has an accessible name and is keyboard-operable.
- `disclosure.ftl` stays byte-identical. `launch_flow.rs` has no diff.
- Only `App` calls `open_url`.
- en-US strings only (pt-BR falls back).

**Scale/Scope**: 4 screens, 1 card and 1 Settings row.

- **New files**: 1 widget module (`widgets/status.rs`) and 1 integration test file.
- **Edited files**:
  - UI: `welcome.rs`, `privacy_notice.rs`, `sign_in.rs`, `device_check.rs`, `app.rs`, `settings/{about,audio,account,mod}.rs`, `widgets/mod.rs`;
  - core: `controller.rs`, `settings_registry.rs`;
  - strings: en-US `app.ftl`, `account.ftl`, `device-check.ftl`, `settings.ftl`.
- **Strings**: 14 new en-US keys ([contracts/fluent-strings.md](./contracts/fluent-strings.md)).

All Technical Context items are resolved; no open unknowns remain ([research.md](./research.md) R1–R15).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: **VI** (sign-out reuse, credential handling unchanged), **VII**, **VIII**, **X**, plus Governance (traceability, Manual Scenario Sign-Off). The rest are N/A, for the reasons given below.

- [x] **I. Real-Time Path Is Sacred**: N/A. No engine, effect or audio-I/O code is touched. All new code runs on the UI thread. `skip_device_check` is an existing no-op, and `restore_getting_started` only writes settings.
- [x] **II. Plugins Are Guests**: N/A. The plugin runtime, gateway and plugin-visible API are untouched. The getting-started card only names bundled plugins with host strings.
- [x] **III. Host Primitives, Plugin Behaviors**: N/A. No primitive, DSP or plugin capability is added or changed.
- [x] **IV. Audio Source Replaceable and Isolated**: N/A. No `audio-source-*` crate is touched, and every test runs on `SyntheticHost` / `FakeBackend`.
- [x] **V. No Audio Ever Leaves the Engine**: N/A. No sample buffer, cache or file export is involved.
- [x] **VI. Security and Privacy by Default**: Pass.
  - The summary view shows only `display_name`, which is already shown in Settings › Account. "Use a different account" goes through the existing confirmed `sign_out()`, which clears the credential from the OS store.
  - No credential is read, logged or persisted by any new code. The elapsed time is not persisted. Telemetry is unchanged (zero).
- [x] **VII. Rust Quality Gates**: Pass.
  - No new crate and no new dependency. `forbid(unsafe_code)` is unaffected.
  - No `unwrap`/`expect` in library code. `restore_getting_started` reports a save failure through the existing `settings-save-failed` notification.
  - New pub items carry doc comments with runnable examples: `PrivacyOrigin`, `DeviceCheckOrigin`, `DeviceCheckOutcome`, `SummaryOutcome`, `AboutOutcome`, `widgets::status_message`, `sign_in::elapsed_hint`, `PlaybackController::restore_getting_started`.
  - fmt, clippy `-D warnings`, test, doc and deny are all in [quickstart.md](./quickstart.md) §1.
- [x] **VIII. Test What the NFRs Promise**: Pass.
  - Every contract rule (M, W, N, P, S, D, V, G, A, K, F) maps to a named test in `onboarding_clarity.rs` or an extended suite, written before the implementation (red → green).
  - SC-002 uses an injected clock, with no sleeps.
  - Regression pins cover the existing disclosure outcomes (W5), the hash (W6), the device-check answers (D5) and `next_step` (V6).
  - No NFR-10.3 suite and no real-time crate is affected, so no new benchmark or soak test is required.
- [x] **IX. One Plugin API Definition**: N/A. The plugin API schema and its versioning are untouched.
- [x] **X. Simplicity, Portability, User's Override**: Pass.
  - No new trait. Time comes from egui's input clock rather than a single-implementor `Clock` trait (R1). No feature flag. The summary is a `bool`, not a new launch step.
  - Behaviour is identical across platforms.
  - Every new control (privacy link, back labels, device-check Back, summary Continue/Switch, About restore) has an accessible name and is Tab/Space/Enter-operable. The spinner and status message have roles and names, and activity is also shown as text (FR-013).
  - All strings are Fluent keys. en-US now; pt-BR is deferred to the localization feature (C15). This is a sanctioned sequencing, not a deviation: the pt-BR fallback is en-US.
  - The transport-focus override is unaffected.
- [x] **Governance › Traceability**: Pass. This feature implements the source requirements FR-1.1.1, FR-1.1.2, FR-1.1.3, FR-1.2.1, FR-1.2.3, FR-1.2.4, FR-1.3.1, FR-1.4.1, NFR-6.1, NFR-6.2 and NFR-7.1 from docs/ModPlayer-Software-Specification.md. Each spec FR names its source IDs (spec.md › Requirements). Each contract rule cites its spec clarification and FR (C1–C16, FR-001–FR-014). Tasks cite the same IDs, UX-07 to UX-12, and the source IDs they implement.
- [x] **Governance › Area-maintainer sign-off**: N/A. No engine, Capability Gateway or Plugin Runtime change.
- [x] **Governance › Manual Scenario Sign-Off**: Pass. [quickstart.md](./quickstart.md) §3 defines M1–M9 for the implementing agent to execute itself (macOS recipe, real Keychain, live account), with evidence recorded in `tasks.md`.

**Post-design re-check (after Phase 1)**: The result is unchanged. The data model adds no persisted field (VI holds). The contracts add no trait and no dependency (VII and X hold). Every rule has a test obligation (VIII holds). The only structural edit outside UI views is one controller method and one registry descriptor in `modplayer-core`, both inside existing component boundaries.

## Project Structure

### Documentation (this feature)

```text
specs/030-onboarding-clarity/
├── plan.md                         # This file
├── research.md                     # Phase 0: R1–R15
├── data-model.md                   # Phase 1: UI-only state, outcomes, restore transition
├── quickstart.md                   # Phase 1: gates, automated A1–A11, manual M1–M9
├── contracts/
│   ├── onboarding-screens.md       # Phase 1: rules M, W, N, P, S, D, V, G, A, K
│   └── fluent-strings.md           # Phase 1: new en-US keys, F1–F4
├── checklists/                     # from /speckit-specify / clarify
└── tasks.md                        # Phase 2 (/speckit-tasks; not created here)
```

### Source Code (repository root)

```text
crates/
├── modplayer-ui/
│   ├── src/
│   │   ├── welcome.rs              # hierarchy, compact legal block, Link roles, Default Decline
│   │   ├── privacy_notice.rs       # PrivacyOrigin param, origin-specific back label
│   │   ├── sign_in.rs              # pending spinner/hint/elapsed, status-message branches,
│   │   │                           #   elapsed_hint(), show_summary() + SummaryOutcome
│   │   ├── device_check.rs         # DeviceCheckOrigin, DeviceCheckOutcome, Back, field::row buffer
│   │   ├── getting_started.rs      # doc-comment fix only (no chord prose; content unchanged)
│   │   ├── app.rs                  # sign_in_summary flag, DeviceCheckOutcome handling, origins
│   │   ├── widgets/
│   │   │   ├── mod.rs              # export status_message
│   │   │   └── status.rs           # NEW: shared status-message widget
│   │   └── settings/
│   │       ├── about.rs            # PrivacyOrigin::About, restore row, AboutOutcome
│   │       ├── audio.rs            # DeviceCheckScreen::new(.., DeviceCheckOrigin::Settings)
│   │       ├── account.rs          # identity_text / open_sign_out_modal / show_sign_out_modal → pub(crate)
│   │       └── mod.rs              # pass dismissed flag + map AboutOutcome to controller
│   └── tests/
│       ├── onboarding_clarity.rs   # NEW: contract rules M/W/N/P/S/D/V/G/A
│       ├── fluent_keys.rs          # + ONBOARDING_030_KEYS, plural cases
│       ├── accessibility.rs        # + new controls in the sweep
│       ├── high_contrast.rs        # + status bar/text legibility
│       └── first_launch.rs, section_memory.rs  # call-site updates
├── modplayer-core/
│   └── src/
│       ├── controller.rs           # + restore_getting_started()
│       └── settings_registry.rs    # + about.show_getting_started descriptor; test update
└── modplayer-account/              # unchanged (launch_flow.rs, session.rs, disclosure.rs)
locales/en-US/
├── app.ftl                         # + privacy-back-to-*
├── account.ftl                     # + signin-browser-hint, -indicator, -elapsed-*, signin-summary-*
├── device-check.ftl                # + device-check-back
├── settings.ftl                    # + setting-show-getting-started*
└── disclosure.ftl                  # NOT edited (hash-pinned)
```

**Structure Decision**: This feature uses the existing Cargo workspace layout. Every directory above already exists in this repository: `crates/modplayer-ui/src`, `crates/modplayer-ui/src/widgets`, `crates/modplayer-ui/src/settings`, `crates/modplayer-ui/tests`, `crates/modplayer-core/src`, `crates/modplayer-account/src` and `locales/en-US`.

- **Where the work goes**: Nearly all work is in `modplayer-ui`, the view layer that owns these screens. `modplayer-core` gets one controller method and one registry descriptor, because persistence and settings search live there. `modplayer-account` is deliberately unchanged, so the protocol and `next_step` stay out of scope.
- **New files**: Only two, `crates/modplayer-ui/src/widgets/status.rs` and `crates/modplayer-ui/tests/onboarding_clarity.rs`. No new crate.
- **All Phase 0/1 artifacts are present**: `research.md`, `data-model.md`, `contracts/` and `quickstart.md`.

## Complexity Tracking

No constitution violation needs justifying. The design choices below go against an existing local convention, so each is recorded here with the simpler alternative that was rejected.

| Departure | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Settings › About gets its first `SettingDescriptor`, and the unit test `placeholder_only_categories_contribute_no_descriptors` drops `About` | C14 requires the restore control to be found by Settings search | An unregistered button fails C14 / SC-005's searchability. A separate search hook adds a second search mechanism |
| `settings::account`'s `identity_text`, `open_sign_out_modal` and `show_sign_out_modal` become `pub(crate)` and are called from `sign_in.rs` | C16 requires the *existing* sign-out confirmation | A copied modal would diverge in wording and focus behaviour. Moving the modal to a new module is a larger refactor than this feature needs |
| `DeviceCheckScreen::show` returns a 3-state `DeviceCheckOutcome` instead of `bool`, and its constructor takes an origin | Back has two destinations that depend on the entry point (C10, C16) | Inferring the origin from `launch_step()` is ambiguous: a Settings-opened check can coexist with `device_confirmed = false` (R9) |
| `App::sign_in_summary` overrides what the `DeviceCheck` step draws, without changing `next_step` | C16 / FR-014 forbid changing `launch_flow::next_step` | A `LaunchStep::SignInSummary` variant would change the pure launch function and its tests |
