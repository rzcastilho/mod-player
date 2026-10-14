# Contract: Onboarding Screens (UI surface)

**Feature**: 030-onboarding-clarity. This contract defines the testable rules for every screen this feature changes. Each rule ID maps to at least one named test (see [quickstart.md](../quickstart.md)). The entities are in [data-model.md](../data-model.md) and the strings are in [fluent-strings.md](./fluent-strings.md).

Shared definitions:

- **measure** = `theme::body_measure(ctx)`, which is 72 × the body `'0'` advance.
- **test sizes** = inner window 960 × 640 and 1600 × 900.
- A **prose node** is any accesskit `Label`/text node drawn by the screen, excluding button and link labels.

---

## §M Measure (all four screens + summary view): C1, FR-002, SC-001

- **M1**: At both test sizes, every prose node's bounds width is ≤ measure + 1 px. This applies to: welcome (disclosure view), privacy notice from either origin, each sign-in branch in §S/§P, the summary view (§V), and the device check (normal and zero-devices).
- **M2**: At an available width below measure, prose wraps to the available width. No `ScrollArea` gains a horizontal bar, and no node's right edge exceeds the panel's right edge.

## §W Welcome (disclosure view): C2, C3, C4, FR-001, FR-003

- **W1 Order**, top to bottom:
  1. heading `welcome-title` (`DISPLAY` style);
  2. lead `welcome-description`, the only body-style prose;
  3. compact block;
  4. action row.
- **W2 Compact block**:
  - One `Frame::group`, with no heading.
  - It contains, in this order, `disclosure-unofficial`, `disclosure-premium-required`, `disclosure-terms-apply` (`SECONDARY` text style, `text_secondary` colour), then a row with the terms link (`disclosure-terms-link` → `TERMS_URL`) and the privacy link (`disclosure-privacy-link`).
  - Both links have accesskit `Role::Link`. Neither has a button fill.
- **W3 Primary**: exactly one `Variant::Primary` control on the screen, labelled `welcome-acknowledge`. `welcome-decline` is `Variant::Default`, in the same row, to its right.
- **W4 Tab order**: from no focus, the first Tab that lands in the action row focuses `welcome-acknowledge` and the next one focuses `welcome-decline`. No new keyboard accelerator is registered.
- **W5 Outcomes unchanged**:
  - Acknowledge records `DisclosureAcknowledgement::now(DISCLOSURE_BUNDLE_VERSION)` and returns `WelcomeOutcome::Acknowledged`.
  - Decline switches to the Decline view, and its Quit closes the viewport and writes nothing (the existing `decline_writes_nothing` test stays green).
- **W6 Hash**: `locales/en-US/disclosure.ftl` is byte-identical, and `tests/disclosure.rs` in `modplayer-account` passes unchanged.

## §N Privacy notice: C5, FR-004

- **N1**: `privacy_notice::show(ui, PrivacyOrigin::Welcome)` draws a back control labelled `privacy-back-to-welcome`. `PrivacyOrigin::About` draws `privacy-back-to-about`.
- **N2**: Clicking back returns `true`. The caller then shows Welcome's disclosure view or About's main view respectively.
- **N3**: Opening the notice and going back leaves `settings.disclosure` identical: a load before and after compares equal.
- **N4**: The notice body (`privacy-body`) satisfies M1 at 1600 px.

## §P Pending sign-in (`SessionState::Authorizing`): C4, C6, C7, FR-005, FR-006, SC-002

- **P1 Order**:
  1. heading `signin-waiting`;
  2. spinner;
  3. `signin-browser-hint`;
  4. elapsed hint;
  5. `signin-waiting-resumed`, only when `resumed`;
  6. action row.
- **P2 Indicator**: there is an accesskit node with role `ProgressIndicator` and name `signin-waiting-indicator`, present on the first frame of `Authorizing`.
- **P3 Elapsed hint**: with injected `RawInput::time`, the first `Authorizing` frame at `t₀` and later frames at `t₀ + s` show `elapsed_hint(s)`:
  - s = 0 → "Waiting for 0 seconds"
  - 1 → "… 1 second"
  - 59 → "… 59 seconds"
  - 60 → "Waiting for 1 minute"
  - 119 → "… 1 minute"
  - 120 → "… 2 minutes"
- **P4 Repaint**: while `Authorizing`, the frame's `ViewportOutput::repaint_delay` is ≤ 1 s.
- **P5 Reset**: a new `attempt_id`, or leaving `Authorizing` and returning, restarts the count from 0.
- **P6 Actions**:
  - `signin-open-again` is `Variant::Primary` and comes first.
  - `signin-cancel` is `Variant::Default` and comes second.
  - Their behaviour is unchanged: Open-again emits `AccountEvent::BrowserUrlReady(account.browser_url())`, and Cancel calls `cancel_sign_in()`.
- **P7 Timeout unchanged**: the screen adds no timeout. When the existing 5-minute TTL fires, the state becomes `SignedOut { note: Some(TimedOut) }` and §S applies.

## §S Status message (sign-in reasons): C8, C9, FR-007, SC-003

- **S1 Widget**: `widgets::status_message(ui, text)` draws:
  - a framed row (divider stroke, `radius::SM`);
  - a leading bar `space::XS` wide filled with `roles.warning`;
  - the text in the `body` text style, `roles.text_primary`, capped at measure.

  Its accesskit node has `Role::Status`, `Live::Polite`, and its name is the text. Every colour comes from theme roles: `design_token_literals` stays green, and `high_contrast` checks the bar and text in both high-contrast themes.
- **S2 Coverage**: each of the following renders exactly one `Role::Status` node with the given text, and no other branch renders one:

  | State | Status text |
  |---|---|
  | `SignedOut{note: Cancelled}` | `signin-note-cancelled` |
  | `SignedOut{note: TimedOut}` | `signin-note-timed-out` |
  | `SignedOut{note: ServiceError}` | `signin-note-service-error` |
  | `SignedOut{note: PreviousDidNotFinish}` | `signin-note-previous-unfinished` |
  | `SignedOut{note: Revoked}` | `signin-note-revoked` |
  | probe failure (`store_unavailable`) | `signin-store-unavailable{store}` + remedy |
  | `StoreUnreadable` | `signin-store-unavailable{store}` + remedy |
  | tier result `Free` | `tier-free-explanation` |
  | tier result `Unknown` | `tier-unknown-explanation` |

  `SignedOut{note: None}`, `Authorizing`, `Checking`, the summary view and the device check render zero `Role::Status` nodes.
- **S3 Position**: in each row of S2, the status node's `bounds.y1` is ≤ the `y0` of every button in the action row (Retry / Continue / Open upgrade page), and ≥ the heading's `y1`.
- **S4 Distinct from instructions**: in `SignedOut{note: Some(_)}`, the instruction `signin-explanation` is a plain `Label` (not `Role::Status`), drawn *after* the status node.
- **S5 Behaviour unchanged**:
  - Retry still calls `start_sign_in` (signed-out, probe failure) or `retry_store_read` (`StoreUnreadable`).
  - Tier Retry still calls `recheck_tier`. Continue clears `tier_result`.

## §D Device check: C10, C11, C12, FR-008, FR-009, FR-010, SC-004

- **D1 Back present**: a `Variant::Default` control labelled `device-check-back` is drawn above the device list, in the normal layout and in the zero-devices layout, from either origin. It is keyboard-focusable, and its accessible name is the label.
- **D2 Back from Settings** (`origin = Settings`):
  - `show` returns `DeviceCheckOutcome::Back`, and `skip_device_check()` has been called.
  - The next frame shows Settings with category `Audio`.
  - `device_confirmed`, `buffer_preset` and the preferred device on disk are unchanged.
- **D3 Back during launch** (`origin = LaunchFlow`):
  - `show` returns `Back`, and no controller method was called.
  - `App` shows the summary view (§V) on the next frame.
  - `self.device_check` still holds the screen with the same `selected` and `preset`.
- **D4 Buffer field**:
  - The preset combo is drawn by `field::row` with label `setting-buffer-preset`, which is also the combo's accessible name (`labelled_by`).
  - The help text `setting-buffer-preset-desc` is directly below it, within measure.
  - The preset labels are unchanged (`"{preset} (~{ms} ms)"`).
- **D5 Unchanged**:
  - `device-check-device-list`, the radio list, `device-check-play-tone` and `device-check-question` keep their order.
  - Yes, "No, try another" and "Skip for now" keep their keys, order, variants and calls: `confirm_device` + `Closed`; `preview_device`; `skip_device_check` + `Closed`.
  - Every existing device-check test passes. Only the call sites move from `bool` to `DeviceCheckOutcome`.

## §V Sign-in summary view: C16, FR-008

- **V1 Order**:
  1. heading `signin-summary-title`;
  2. `signin-summary-identity { $name }`, where `name` is `display_name`, or `account-identity-unavailable` when blank;
  3. a row with `signin-summary-continue` (`Variant::Primary`) then `signin-summary-switch` (`Variant::Default`).
- **V2 Non-destructive**: drawing the view and pressing Continue change nothing in `account.toml`, the secure store, `settings.toml` or the controller's device state. A snapshot before and after compares equal.
- **V3 Continue**: clears `sign_in_summary`, and the next frame draws the device check with the stored `selected` / `preset`.
- **V4 Use a different account**: opens 028's sign-out modal (same `signout-confirm-*` keys, Cancel focused first).
  - Confirm → `sign_out()` events are handled. The state becomes `SignedOut`, the next frame's step is `SignIn`, `sign_in_summary` is `false`, and the normal sign-in screen (`signin-title`) shows.
  - Cancel, Esc or a backdrop click → the summary view stays.
- **V5 Expired**: the same rules hold with `SessionState::Expired`.
- **V6 next_step untouched**: `launch_flow.rs` has no diff, and its unit tests are unchanged.

## §G Getting-started card: C13, FR-011

- **G1 Content**: inside the card frame, the accesskit nodes are:
  - heading `getting-started-title`;
  - exactly one line per bundled plugin (`getting-started-section-loop`, `getting-started-key-tempo`);
  - link `getting-started-shortcuts` (`Role::Link`);
  - link `getting-started-tutorial` (`Role::Link`);
  - button `getting-started-dismiss`.
- **G2 No chord prose**: no text node in the card contains `"Shortcuts:"`, and none contains the display label of any host action's default chord as produced by 029's `key_label` for either platform.
- **G3 Shortcut link**: activating `getting-started-shortcuts` returns `GettingStartedOutcome::OpenShortcuts`, and `App` opens 029's sheet. This is existing behaviour, re-pinned.

## §A Settings › About restore: C14, FR-012, SC-005

- **A1 Control**: About's main view draws a `field::row` with id `about.show_getting_started`, label `setting-show-getting-started` and help `setting-show-getting-started-desc`. The control is a button with the same label.
- **A2 Enabled state**:
  - `getting_started_dismissed == true` → enabled.
  - `false` → disabled, with accesskit description `setting-show-getting-started-visible`.
- **A3 Effect**:
  - Click → `AboutOutcome::RestoreGettingStarted` → `controller.restore_getting_started()`.
  - `getting_started_dismissed()` is then `false`, and a fresh `SettingsStore::load()` reads `getting_started_dismissed = false`.
  - Library shows the card on the next frame.
- **A4 Search**: `settings_registry::search("getting")` returns `about.show_getting_started`. Selecting the hit opens About with the row highlighted (028 highlight).
- **A5 Reach**: from the Settings screen it takes 2 interactions: About in the category list, then the button.
- **A6 Persistence**: dismiss → restart (new controller from the same store) → card hidden. Restore → restart → card visible.

## §K Keyboard and accessibility (all new controls): constitution X, FR-013

- **K1**: every new control has a non-empty accessible name and is reachable by Tab: the privacy link, the two privacy back labels, the device-check Back, the summary Continue and Switch, and the About restore. Each one activates with Space/Enter when focused (egui default). They are added to `tests/accessibility.rs`'s sweep.
- **K2**: the spinner (P2) and status message (S1) are announced through role and name, and activity is never signalled by animation alone (P3 text).
