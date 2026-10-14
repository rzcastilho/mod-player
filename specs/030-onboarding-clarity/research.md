# Research: Onboarding and Sign-In Clarity

**Feature**: 030-onboarding-clarity | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

The spec's clarifications (C1–C16) already settle *what* each screen shows. This file settles *how*: which existing code each change uses, and which test seam proves it. Every decision was checked against the code in this worktree (`crates/modplayer-ui/src/{welcome,privacy_notice,sign_in,device_check,getting_started,app}.rs`, `settings/{about,audio,account,field,mod}.rs`, `modplayer-core` `controller.rs` / `settings_registry.rs` / `i18n.rs`, `modplayer-account` `launch_flow.rs` / `session.rs`).

With these decisions, every Technical Context item in plan.md is resolved.

---

## R1. Clock source for the elapsed hint (C7, SC-002)

- **Decision**: Read time from egui's input clock, `ui.input(|i| i.time)` (seconds as `f64`, set from `RawInput::time`). `SignInScreen` stores `authorizing_since: Option<(u64 /*attempt_id*/, f64 /*time*/)>`. On the first frame it sees `SessionState::Authorizing { attempt_id, .. }` with a different (or no) stored attempt, it records `(attempt_id, now)`. On any frame that is not `Authorizing`, it clears the value. Elapsed = `floor(now − since)` whole seconds, clamped at 0.
- **Rationale**: Headless tests already drive `Context::run` with a hand-built `RawInput`. Setting `RawInput::time` to 0.0, 59.0, 60.0 … gives a fully deterministic injected clock without adding a parameter or a trait. On a real frame, eframe fills `time` from a monotonic clock. The value lives only in UI state and is never persisted (C7). A relaunch builds a fresh `SignInScreen`, so a resumed attempt counts from relaunch, as C7 requires.
- **Alternatives considered**:
  - `std::time::Instant::now()` in the view: not injectable, so SC-002 could only be tested with sleeps. Rejected.
  - Add an `authorizing_since` timestamp to `SessionState::Authorizing` or use `AccountService`'s `Clock`: this changes the account crate's state type and its tests. FR-014 keeps the protocol out of scope, and the timestamp is a presentation concern. Rejected.
  - A new `Clock` trait in the UI crate: it would have a single implementor besides the test one, and constitution X forbids that. Rejected.

## R2. Elapsed hint wording and plurals (C7)

- **Decision**: Add a pure function `sign_in::elapsed_hint(secs: u64) -> String`. Under 60 it resolves `signin-waiting-elapsed-seconds` with `count = secs`. From 60 on it resolves `signin-waiting-elapsed-minutes` with `count = secs / 60` (floor). Both messages use Fluent `[one]` / `*[other]` selectors on `$count`.
- **Rationale**: `modplayer_core::tr_args` already passes an argument named `count` as a `FluentValue` number, so plural selectors apply (see `i18n.rs`; `library.ftl` and `app.ftl` already use this). Keeping the function pure lets a unit test cover the 0/1/59/60/61/119/120/299 boundaries without egui.
- **Alternatives considered**: a single message with nested selectors. It is harder to read and gives translators no benefit. Rejected.

## R3. Repaint cadence while pending (C7)

- **Decision**: While the elapsed hint is drawn, `show_authorizing` calls `ui.ctx().request_repaint_after(Duration::from_secs(1))`.
- **Rationale**: `App::ui` already requests a repaint every `REPAINT_INTERVAL`, which is faster than 1 s. C7 still requires an explicit request so the guarantee does not depend on that global loop. egui keeps the earliest pending request, so the extra call costs nothing.
- **Alternatives considered**: relying on the global loop alone. It meets the cadence today but leaves C7 unenforced if the loop changes. Rejected.

## R4. Shared status-message style (C8, FR-007, SC-003)

- **Decision**: Add a new widget `widgets::status_message(ui, text: &str) -> Response` in a new file `crates/modplayer-ui/src/widgets/status.rs`, exported from `widgets/mod.rs`. It draws:
  - a `Frame` with the `divider_color` stroke and `radius::SM` corners and `space::SM` inner margin;
  - a leading bar `space::XS` wide in `roles.warning`, painted with the frame's painter over the full row height;
  - the text as a wrapping `Label` in the `body` text style and `roles.text_primary`, capped at `body_measure`.

  It returns the frame's response. The frame's node gets accesskit `Role::Status` with `Live::Polite` (the precedent is `search_view.rs`'s `set_live`) and the label as its name.
- **Rationale**: One function means one style, so SC-003 can check the style by finding the `Status` role node, and the bar colour can be checked in the `design_token_*` suites. `Role::Status` is implicitly polite in AccessKit. Setting `Live::Polite` explicitly matches 026. The warning bar plus the frame meets "visually distinct" without colour alone: the shape is different too, so the high-contrast case is covered.
- **Alternatives considered**:
  - Reusing the notification card from 019: those are dismissible toasts with severity icons and actions, and the message here is an inline, non-dismissible reason. Rejected.
  - A text colour change only: it fails the "not colour alone" rule and is too weak in high contrast. Rejected.

## R5. Which sign-in branches use the status message (C8, C9)

- **Decision**: The four branches of `sign_in::show` that carry a reason use the new widget. Each one draws heading → `status_message` → instructions (if any) → action row:

  | Branch | Reason text | Instruction text | Actions |
  |---|---|---|---|
  | `show_signed_out` with `Some(note)` | `tr(note_key(note))` | `signin-explanation` | Retry |
  | `show_store_unavailable` / `show_store_unreadable` (shared `show_store_message`) | `signin-store-unavailable` + `" "` + the remedy line, as one status message | — | Retry |
  | `show_tier_result(Free)` | `tier-free-explanation` | — | Open upgrade page, Continue |
  | `show_tier_result(Unknown)` | `tier-unknown-explanation` | — | Retry, Continue |

  `show_signed_out` with `None` (a plain first sign-in) has no reason and stays an instruction line.
- **Rationale**: This follows C8's list exactly. The branches are mutually exclusive by construction (C9): early returns on `tier_result` and `store_unavailable`, then a `match` on one `SessionState`. So "exactly one status message" holds structurally, and a test per branch confirms it.
- **Alternatives considered**: drawing the store reason and its remedy as two status messages. That gives two live-region announcements for one reason, and C8 says "the reason". The remedy goes in the same message, as a second sentence. Rejected as two messages.

## R6. Spinner accessibility while pending (C6)

- **Decision**: Copy `show_checking`'s pattern: `ui.add(Spinner::new())`, then `response.widget_info(|| WidgetInfo::labeled(WidgetType::ProgressIndicator, true, tr("signin-waiting-indicator")))`. The new key is "Waiting for your browser". The elapsed-hint text gives the non-animated signal of activity.
- **Rationale**: It is already proven in the same file and already swept by `tests/accessibility.rs`.
- **Alternatives considered**: none. It is the existing pattern.

## R7. Welcome hierarchy and compact legal block (C2, C3, C4, FR-001, FR-003)

- **Decision**:
  - Heading stays `welcome-title` in the `DISPLAY` style.
  - The lead is `welcome-description` alone, in a `body_measure`-capped scope.
  - Below it, a `Frame::group` capped at `body_measure` holds:
    - the three disclosure labels in `theme::text::SECONDARY` and `text_secondary`;
    - a `horizontal_wrapped` row with the terms `ui.hyperlink_to` (an egui `Hyperlink`, which is `WidgetType::Link` → `Role::Link`) and the privacy control as `egui::Link::new(tr("disclosure-privacy-link"))` (also `WidgetType::Link`; `clicked()` switches to `View::Privacy`).
  - The action row is `button(ui, Variant::Primary, welcome-acknowledge)` followed by `button(ui, Variant::Default, welcome-decline)`.
- **Rationale**: Today the privacy control is a `ui.button`, so it has a fill and the button role, which violates C3. `egui::Link` is the stock link widget and needs no custom accesskit override. Building the Decline button through `widgets::controls::button(Variant::Default)` makes "exactly one Primary" checkable through the existing 015 variant tests, instead of depending on what `ui.button` happens to look like. `disclosure.ftl` is not touched, so `DISCLOSURE_EN_US_SHA256` holds (C2).
- **Alternatives considered**:
  - Keeping the privacy control as a button with a `Role::Link` override, like the getting-started tutorial: it still draws a button fill, which C3 explicitly rules out. Rejected.
  - Giving the legal block a heading: C3 says "no heading of their own". Rejected.

## R8. Privacy-notice origin and back label (C5, FR-004)

- **Decision**: Add `pub enum PrivacyOrigin { Welcome, About }`. `privacy_notice::show(ui, origin) -> bool`. The back key is `privacy-back-to-welcome` for `Welcome` and `privacy-back-to-about` for `About`. Both are new keys in `app.ftl`. `welcome.rs` passes `Welcome`; `settings/about.rs` passes `About`.
- **Rationale**: The origin is known statically at each call site, so no runtime state is needed. Return semantics stay unchanged (`true` → the caller pops its own sub-view), and the disclosure acknowledgement is never touched on that path. The existing `privacy-back` ("Back") key stays in `disclosure.ftl` and is still used by About's read-only disclosure view, so the hashed file is unchanged (C2).
- **Alternatives considered**:
  - One key with a `$destination` argument: it splits a sentence into fragments for translators. Rejected.
  - Storing the origin inside a stateful `PrivacyNoticeScreen`: it adds state the call sites already know. Rejected.

## R9. Device-check origin, Back control, outcome type (C10, C12, C16, FR-008)

- **Decision**:
  - `DeviceCheckScreen` gains `origin: DeviceCheckOrigin { LaunchFlow, Settings }`, set in `new(initial_device, initial_preset, origin)`.
  - `show` returns `DeviceCheckOutcome { Open, Closed, Back }` instead of `bool`.
  - A Back control (`Variant::Default`, key `device-check-back`, "Back") is drawn first on the screen, above the content, in both the normal and the zero-devices layouts.
  - On click:
    - `Settings` origin: call `controller.skip_device_check()` (C10's "skip semantics", which is a documented no-op on persisted state) and return `Back`. `show_main` drops the screen, so the Settings › Audio category that was on screen before is shown again (`self.settings.category()` is untouched).
    - `LaunchFlow` origin: return `Back` without calling any controller method. `App` keeps the screen in `self.device_check`, so the selected device and preset survive, and sets `sign_in_summary = true`.
  - Construction sites: `App::new` and `show_launch_gate_device_check` → `LaunchFlow`; `settings/audio.rs` "Test output device" → `Settings`.
- **Rationale**: The origin decides the Back destination, so it belongs on the screen that draws Back. `App` already holds both entry points in one `Option<DeviceCheckScreen>`, and the origin field tells them apart without a second slot. A three-state outcome replaces a boolean that could not express "back". Placing Back first, above the content, follows the screen-step convention and keeps it apart from the Yes / No / Skip answer row, which stays unchanged (C12, FR-010).
- **Alternatives considered**:
  - Inferring the origin from `launch_step()` at draw time: a Settings-opened check can be open while `next_step` says `Main` even with `device_confirmed = false`, so `should_show_device_check()` alone is ambiguous. Rejected.
  - Putting Back in the answer row: it mixes navigation with the three answers and makes the "unchanged answers" contract harder to pin. Rejected.

## R10. Sign-in summary view and sign-out reuse (C16, FR-008)

- **Decision**:
  - Add `sign_in::show_summary(ui, account) -> SummaryOutcome` with `SummaryOutcome { None, Continue, Events(Vec<AccountEvent>) }`. It draws:
    - the heading `signin-summary-title` ("Signed in");
    - the line `signin-summary-identity` ("Signed in as { $name }"), where `name = identity_text(display_name)`. This reuses 028's `settings::account::identity_text`, made `pub(crate)`, which falls back to `account-identity-unavailable`;
    - a row with `button(Primary, signin-summary-continue)` and `button(Default, signin-summary-switch)`.
  - "Use a different account" calls `settings::account::open_sign_out_modal(ui)`. On every frame the function also calls `settings::account::show_sign_out_modal(ui, account)`. Both become `pub(crate)`, so it is the *same* dialog, keys and Cancel-first focus.
  - `App` gains `sign_in_summary: bool`. In `LaunchStep::DeviceCheck`, if the flag is set, `App` draws the summary instead of the device check. `Continue` clears the flag. `Events` go through `handle_account_event`, which already runs `clear_for_sign_out`.
  - At the top of `App::ui`, the flag is cleared whenever `launch_step() != DeviceCheck`. That covers a completed sign-out (→ `SignIn`) and an expiry that ends the gate. A restart builds a fresh `App`, so the flag starts `false`.
- **Rationale**: `next_step` stays untouched (FR-014, C16). The override is a UI-only flag that sits on top of the computed step. Reusing the 028 modal avoids a second sign-out confirmation with different wording. The modal `Id` cannot collide, because Settings › Account is unreachable while a launch gate is up (020 chrome hides the nav rail).
- **Alternatives considered**:
  - Adding a `LaunchStep::SignInSummary`: this changes `next_step`, which FR-014 / C16 forbid. Rejected.
  - Calling `sign_out()` directly without the confirmation: C16 requires the existing dialog. Rejected.
  - Moving the modal into its own module: a larger refactor than needed. Making the two functions `pub(crate)` is enough. Rejected for now.

## R11. Buffer-preset label and explanation (C11, FR-009)

- **Decision**: Draw the device-check combo through 028's `settings::field::row` with `FieldSpec::new("device_check.buffer_preset", tr("setting-buffer-preset"))`, `help = Some(tr("setting-buffer-preset-desc"))` and `reset = ResetState::Hidden`. This is the same call shape `settings/audio.rs` uses for `audio.buffer_preset`. The combo keeps `from_id_salt("device-check-buffer-preset")` and its `PRESETS` order and labels.
- **Rationale**: `field::row` already draws the visible label, links it as the control's accessible name (`labelled_by`) and draws the help text capped at `body_measure`, so it satisfies C11 with zero new layout code. The device check and Settings › Audio then render the same field.
- **Alternatives considered**: `ComboBox::from_label`. It places the label *after* the combo, gives no help text and is inconsistent with Settings. Rejected.

## R12. Getting-started restore (C13, C14, FR-011, FR-012)

- **Decision**:
  - **Core.** Add `PlaybackController::restore_getting_started()`, the mirror of `dismiss_getting_started`: it sets the shadow flag to `false` and calls `persist_settings(|s| s.getting_started_dismissed = false)`, so a failed save raises `settings-save-failed` and the in-memory flag still flips.
  - **Registry.** Add the descriptor `SettingDescriptor { category: About, id: "about.show_getting_started", title_key: "setting-show-getting-started", description_key: "setting-show-getting-started-desc" }` to `settings_registry::DESCRIPTORS`. Update `placeholder_only_categories_contribute_no_descriptors` to drop `About` from its list, and update the "About is descriptor-free" doc comment.
  - **UI.**
    - `about::show(ui, screen, getting_started_dismissed: bool, focus: Option<&str>) -> AboutOutcome { None, RestoreGettingStarted }`.
    - The main view draws a `field::row` with `id = "about.show_getting_started"` and `label_in_control = true`. Its control is `ui.add_enabled(dismissed, Button::new(..))`.
    - When it is disabled, the response's accesskit description is `setting-show-getting-started-visible` ("The getting-started card is already showing").
    - `settings::show_content` maps the outcome to `controller.restore_getting_started()`.
  - **Card.** The card text stays as it is today: the 029 "Shortcuts: …" prose was already removed. A test asserts that no host action's default chord label (through 029's `key_label` display text) and no `"Shortcuts:"` prefix appears in the card's accesskit labels.
- **Rationale**: About gets its first searchable entry, which C14 requires. Registering it costs one descriptor and one test edit. Returning an outcome keeps `about.rs` non-generic over `PlaybackController<B, H>`, matching how `settings::audio` hands back a `DeviceCheckScreen`. A disabled control with a description gives the idempotency C14 asks for, with no error path. Two interactions from Settings: select About, click.
- **Alternatives considered**:
  - A switch "Show getting-started card" bound to `!dismissed`: it is a persistent boolean, so the 015 inventory would require `switch`. But C14 asks for a one-shot button, and a switch would also let the user dismiss from Settings, which widens scope. Rejected.
  - Putting the restore control in Settings › Appearance: C14 names About. Rejected.

## R13. Measure verification (C1, FR-002, SC-001)

- **Decision**: Use the same seam as `tests/search_view.rs`. Render each screen in a headless `egui::Context` with `RawInput::screen_rect` at 960 × 640 and 1600 × 900, enable accesskit, and assert that every `Label`/text node's `bounds` width is ≤ `theme::body_measure(&ctx) + 1.0`. The screens are welcome, the privacy notice from both origins, every sign-in branch, the summary view, and the device check (normal and zero-devices).
- **Rationale**: It is already proven in this crate and measures what is actually laid out, not the code that caps it.
- **Alternatives considered**: a source-text grep for `set_max_width`. It proves the cap is written, not that it applies (for example inside a `horizontal`). Rejected.

## R14. Localisation scope (C15, FR-013)

- **Decision**: Add en-US strings only, to `app.ftl` (privacy back labels), `account.ftl` (sign-in hints, elapsed hint, indicator name, summary view), `device-check.ftl` (Back) and `settings.ftl` (restore control). Extend `tests/fluent_keys.rs` with a `ONBOARDING_030_KEYS` list (plain `tr`) and the `count` / `name` argument cases through `tr_args`. The byte-identity of `disclosure.ftl` (C2) is already enforced by `crates/modplayer-account/tests/disclosure.rs`, which pins `DISCLOSURE_EN_US_SHA256`. No new test is needed, and that test must keep passing unchanged. No pt-BR parity test is added, and pt-BR falls back to en-US.
- **Rationale**: This follows C2 and C15 exactly. The 024/025 parity tests apply per feature and opt in only when that feature ships pt-BR.
- **Alternatives considered**: adding pt-BR now. That is owned by 007-operations-and-polish/003-localization-pt-br. Rejected.

## R15. Tab / reading order (C4)

- **Decision**: Draw controls in reading order: Primary first, then Default, in the same `ui.horizontal`. egui's focus chain follows creation order, so Tab order matches. A test sends `Tab` key events and reads `ctx.memory(|m| m.focused())` against the two button ids (the precedent is 015's focus tests in `control_variants.rs`).
- **Rationale**: This is the existing convention (`welcome.rs` comment, `shell.rs`), and it needs no new accelerator. Enter keeps egui's default "activate focused" behaviour (C4).
- **Alternatives considered**: none.

## R16. Manual sign-off deviations (T048, 2026-10-13)

Found while walking quickstart.md §3 on macOS with the real Keychain and a live account. These are recorded, not decided.

- **Device-check Back outside the launch gate (C16, M6).** `App::new` builds a `DeviceCheckOrigin::LaunchFlow` screen whenever `controller.should_show_device_check()` is true. If the tier is not `Premium` (here the tier check never finished, so the tier stayed `Unknown`), `launch_flow::next_step` returns `Main`, and `show_main` draws that screen as an overlay. In `show_main` the screen is only kept while the outcome is `Open`, so Back drops it and the user lands in Library. The C16 summary view is only reachable from `show_launch_gate_device_check`. Back confirms nothing and persists nothing, so no state is corrupted, but the summary promised by C16 never appears on this path. Options: route `Back` from a `LaunchFlow` screen in `show_main` to the same summary, or build the screen in `App::new` only when the gate is `DeviceCheck`. **Resolved (operator chose the second option):** `App::new` no longer builds a device check; the gate builds its own on its first `DeviceCheck` frame, and `app::retains_device_check` drops a `LaunchFlow` screen on any frame whose step is not `DeviceCheck` (Settings previews are kept). Test: `launch_device_check_is_dropped_outside_the_gate`. Live check: a signed-in launch with an unverified tier now goes straight to Library (`030-R16-after-continue.png`).
- **Tier check does not finish (environment).** After a successful sign-in, `account.toml` kept `tier = "unknown"` with an empty `id` and `display_name`, and Settings › Account › Re-check subscription changed nothing and logged nothing. This is outside 030's scope, but it blocks M6 and the Premium launch path.
- **Restore control needs 3 interactions at 960 px (C14, SC-005).** At the minimum width, About is in the Settings category overflow ("More"), so the path is More → About → Show getting-started card. C14 assumes "select About, click". At wider windows, where About is shown inline, it is 2.
- **Quickstart wording.** M1 says "at the default 960 × 640 size" and calls Decline "outlined". The default inner size is 1200 × 820 (960 × 640 is the minimum, `crates/modplayer/src/main.rs`), and Decline is `Variant::Default` as C4 specifies. The spec stands; the quickstart wording is what is off.
- **Not exercised.** M4 and M5 need a browser that is not already signed in to Spotify, because with existing consent the OAuth round trip finished in under 4 s. The locked-Keychain step of M5 was skipped because `security lock-keychain` locks the operator's whole login keychain.
