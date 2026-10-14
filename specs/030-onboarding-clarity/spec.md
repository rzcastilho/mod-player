# Feature Specification: Onboarding and Sign-In Clarity

**Feature Branch**: `feature/030-onboarding-clarity`

**Created**: 2026-10-13

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/011-browse-and-manage-polish/006-onboarding-clarity.md (id 006, onboarding-clarity)."

## Clarifications

### Session 2026-10-13 (clarify reviewer)

Sources: constitution v1.1.1 (X: keyboard-operable, accessible names, externalised strings; VII/VIII), breakdown `011-browse-and-manage-polish/006-onboarding-clarity.md` (scope boundary), UI/UX review §3.1–3.2 and §6 table (UX-07…UX-12), and existing code (`welcome.rs`, `privacy_notice.rs`, `sign_in.rs`, `device_check.rs`, `getting_started.rs`, `theme/tokens.rs`, `theme/controls.rs`, `modplayer-account` `launch_flow.rs`/`session.rs`, `locales/en-US/disclosure.ftl`).

- **C1 Readable measure (derived: `theme::body_measure`, research R17, review §5.1).** "Readable measure" means the existing `theme::body_measure` token: 72 × the `body` role's `'0'` advance. Every prose block on the welcome screen, privacy notice, sign-in screen and device check is laid out with `max_width = min(available_width, body_measure)`. Verification uses the 960 × 640 minimum inner window size set in `crates/modplayer/src/main.rs` and also a 1600 px-wide window; no laid-out text row may be wider than `body_measure`. (Replaces the "~75 characters" figure.)
- **C2 Disclosure text is not edited (derived: scope boundary + `disclosure.ftl` hash pin).** `locales/en-US/disclosure.ftl` stays byte-identical, so `DISCLOSURE_EN_US_SHA256` and `DISCLOSURE_BUNDLE_VERSION` do not change and the welcome screen is not re-shown to existing users. The heading is `welcome-title`; the lead paragraph is `welcome-description`; the compact legal/Premium block is `disclosure-unofficial`, `disclosure-premium-required`, `disclosure-terms-apply` plus the terms link and the privacy link, all with their existing wording. Every new string (back labels, sign-in hints, elapsed hint, buffer explanation, restore control) goes in a non-hashed file (`app.ftl`, `account.ftl`, `device-check.ftl`, `settings.ftl`); the existing `privacy-back` key stays where it is.
- **C3 Compact legal block (default: conventional callout).** The three disclosure facts and the two links are drawn inside one framed group below the lead paragraph, in the `secondary` text role, with no heading of their own. The terms and privacy controls have the link role (accesskit `Role::Link`) and are not buttons with a fill.
- **C4 Action emphasis (derived: 015 control variants).** "Primary" means `controls::Variant::Primary`. The welcome screen has exactly one `Primary` control ("I understand, continue"); "Decline" is `Variant::Default`, in the same row, after the primary in reading and Tab order. On the pending sign-in screen "Open the browser again" is `Primary` and "Cancel" is `Default`. Enter on the welcome screen does nothing beyond egui's default focus activation (no new accelerator).
- **C5 Privacy-notice back label (derived: two existing entry points).** The privacy notice is reachable from Welcome and from Settings › About. Its back control reads "Back to welcome" when opened from Welcome and "Back to About" when opened from Settings › About, and returns to that screen. `privacy_notice::show` takes the origin as a parameter. Going back never changes the recorded disclosure acknowledgement.
- **C6 Pending sign-in content (default).** While `SessionState::Authorizing`, the screen shows, in this order: heading (`signin-waiting`); an `egui::Spinner` with accesskit `ProgressIndicator` role and accessible name "Waiting for your browser"; the browser hint "A Spotify sign-in page should have opened in your default browser. Finish signing in there, then come back."; the elapsed hint; the existing `signin-waiting-resumed` line when `resumed`; then the action row. The spinner's activity is also shown by the text of the elapsed hint, so it does not rely on animation alone.
- **C7 Elapsed hint (default).** Time is counted from when the app entered `Authorizing` in this process (a resumed attempt counts from relaunch). The hint reads "Waiting for {n} seconds" while under 60 s, updating every second, and "Waiting for {m} minute(s)" from 60 s on, with Fluent plural forms. The UI asks for a repaint at least once per second while the hint is shown. The existing protocol timeout (`PENDING_ATTEMPT_TTL`, 5 min → `SignInNote::TimedOut`) is unchanged, and no new timeout is added. The elapsed time is UI-only state and is not persisted.
- **C8 Sign-in status message (default; derived for which reasons apply).** Every reason the sign-in step can be showing is drawn with one shared status-message style: a leading accent bar in the `warning` role, the text in the `body` role at `text_primary`, inside a framed row, and an accesskit live-region/status role so screen readers announce it. That covers every `SignInNote` (`PreviousDidNotFinish`, `Cancelled`, `TimedOut`, `ServiceError`, `Revoked`), the credential-store-unreadable and probe-failure views (`signin-store-unavailable` plus the remedy line), and the Free/Unknown tier-result explanation ("account without Premium"). It is never shown in the instruction text style. Layout order: heading → status message → instructions → action row, so the reason is always above Retry/Continue.
- **C9 One reason at a time (derived: `SessionState`).** `SessionState::SignedOut { note }` carries at most one note, and the store-unreadable, probe-failure and tier-result views are mutually exclusive branches in `sign_in::show`. So exactly one reason can show, by construction, and no precedence rule is needed.
- **C10 Device-check back from Settings (derived: Settings › Audio entry point).** When the device check was opened from Settings › Audio › "Test output device", Back returns to Settings › Audio. It confirms no device, persists nothing, and leaves the stream exactly as "Skip for now" leaves it (`skip_device_check` semantics).
- **C11 Buffer label and explanation (default: reuse existing Settings copy).** The preset combo carries the visible label `setting-buffer-preset` ("Buffer preset"), associated as the combo's accessible name, and directly below it the explanation `setting-buffer-preset-desc`, capped at `body_measure`. Reusing the keys keeps the device check and Settings › Audio consistent. The preset labels ("Balanced (~5.8 ms)") are unchanged.
- **C12 Device check otherwise unchanged (derived: breakdown).** The question text, test-tone button, device radio list and the Yes / "No, try another" / "Skip for now" buttons keep their keys, order, variants and controller calls. The zero-devices empty state also gets the Back control.
- **C13 Getting-started card content (derived: current code + 029).** The card keeps exactly one line per bundled plugin (`getting-started-section-loop`, `getting-started-key-tempo`), the "Keyboard shortcuts" link (opens 029's shortcut sheet), the tutorial link, and Dismiss. No shortcut keys are spelled out in the card text. A test asserts that the card's text contains no key-chord prose.
- **C14 Restoring the card (default).** Settings › About gains a "Show getting-started card" button, registered in the settings registry so Settings search finds it. Activating it sets `getting_started_dismissed = false` through the controller and persists it with `persist_settings` (same `[onboarding]` key as dismissal). When the card is already visible the button is disabled, with an accessible description saying so. That makes the call idempotent and gives no error. Reaching it takes ≤ 2 interactions from the Settings screen (select About, click).
- **C15 Localisation (derived: scope boundary + current tree).** This feature adds en-US strings only. `locales/pt-BR` today has no `disclosure.ftl`/`device-check.ftl`, so pt-BR falls back to en-US for those keys until 007-operations-and-polish/003-localization-pt-br. Every new key is added to `tests/fluent_keys.rs`, so it is checked to resolve in en-US.
- **C16 Device-check Back during the launch flow (operator answer, round 1: Q1 = A).** When the device check is shown by the launch gate (`LaunchStep::DeviceCheck`), Back opens a new **sign-in summary view** in the sign-in slot. Opening it changes no account, credential, disclosure or device state, and persists nothing. The view shows, in this order: the heading "Signed in" (new key in `account.ftl`); the line "Signed in as {name}", where `{name}` is the session's `display_name`, or the existing `account-identity-unavailable` text when that is empty; then an action row with "Continue" (`Variant::Primary`) followed by "Use a different account" (`Variant::Default`). "Continue" returns to the device check with its state unchanged (selected device, preset). "Use a different account" runs the existing sign-out command (`sign_out()`) through the existing sign-out confirmation dialog (`signout-confirm-*`). On confirm, the session becomes `SignedOut` and the launch gate shows the normal sign-in screen. On cancel, the summary view stays. The summary view is a UI-side override that `next_step` does not compute: it is held as UI-only state, is not persisted, and is cleared by "Continue", by a completed sign-out, or by app restart (a relaunch reaches the device check again). `launch_flow::next_step` is unchanged (FR-014). The view follows C1 (measure), and both controls have accessible names and are keyboard operable. This holds the same way when the session is `Expired`. The device check is only gated after sign-in, so the summary always has a session to show.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Understand and decide on the welcome screen (Priority: P1)

A first-time user opens ModPlayer and sees a welcome screen with a clear heading, one short lead paragraph saying what the app does, and the legal and Premium facts grouped as a compact block below it. Body text is held to a comfortable reading width rather than spanning the whole window. "I understand, continue" is the one visually primary action; "Decline" sits beside it as an ordinary action. The user can open the privacy notice as a step whose back control names where it returns to ("Back to welcome"; "Back to About" when opened from Settings › About — C5).

**Why this priority**: It is the first screen every user sees and carries the consent decision; it is the highest-leverage clarity fix (UX-07, UX-08).

**Independent Test**: Launch with no recorded disclosure acceptance at the minimum window size; verify the hierarchy, line measure and the single primary action; open the privacy notice and verify its back control label and body measure.

**Acceptance Scenarios**:

1. **Given** the welcome screen at the minimum window size, **When** it is shown, **Then** it has a heading, one lead paragraph and a compact legal/Premium block, no paragraph exceeds the readable measure, and exactly one action is styled as primary.
2. **Given** the welcome screen, **When** the user activates "I understand, continue" or "Decline", **Then** the existing disclosure outcomes occur unchanged.
3. **Given** the welcome screen, **When** the user opens the privacy notice, **Then** its back control names the welcome screen and returns to it, and the notice body respects the same readable measure on wide windows.

---

### User Story 2 - Know what sign-in is doing and why (Priority: P1)

While authorization is pending, the user sees an activity indicator, a statement of which browser page should have opened, and how long the app has been waiting. "Open the browser again" is the primary action with "Cancel" beside it as an ordinary one. When sign-in is being requested again because of an unfinished previous attempt, an unreadable credential store, or an account without Premium, the reason is shown as a visually distinct status message, separate from the instructions, above the retry action.

**Why this priority**: A blank-looking "Waiting for your browser…" screen can strand users (UX-11); an undifferentiated failure line hides why they are here (UX-12).

**Independent Test**: With a fake authorization service, enter the pending state and verify indicator, browser hint and elapsed hint; force each of the three reasons and verify the distinct status message precedes the retry action.

**Acceptance Scenarios**:

1. **Given** authorization is pending, **When** the screen is shown, **Then** an activity indicator, the browser-page hint and an elapsed-time hint are visible, and the elapsed hint advances while waiting.
2. **Given** authorization is pending, **When** the user chooses the primary action, **Then** the authorization page is reopened; **When** they choose Cancel, **Then** the pending attempt is cancelled as before.
3. **Given** the credential store cannot be read, **When** the sign-in screen is shown, **Then** the reason appears as a distinct status message above the retry action.
4. **Given** each of the other reasons (unfinished previous attempt, account without Premium), **When** the screen is shown, **Then** that reason appears in the same distinct status style, visually different from instruction text.

---

### User Story 3 - Go back from the device check and understand the buffer choice (Priority: P2)

On the device check, the user can return to the previous step (from Settings › Audio: back to Settings › Audio, C10; during first launch: a "Signed in as {name}" summary of the completed sign-in step, with Continue and "Use a different account", C16). The buffer-size choice has a visible label and a short explanation of what it trades off. The check keeps its test tone and its three answers (Yes / No, try another / Skip for now).

**Why this priority**: The device check currently dead-ends and presents an unexplained control (UX-09), but affects fewer users than the welcome and sign-in screens.

**Independent Test**: Reach the device check; activate the back control and verify the previous step is shown; verify the buffer choice has a label and explanation and the three answers still behave as before.

**Acceptance Scenarios**:

1. **Given** the device check is shown, **When** the user activates the back control, **Then** the previous step is shown.
2. **Given** the device check during the first-launch flow, **When** the user activates Back, **Then** the sign-in summary "Signed in as {name}" is shown with "Continue" (primary) and "Use a different account", and no account, credential or device state has changed; **When** they choose Continue, **Then** the device check is shown again with its prior selections; **When** they choose "Use a different account" and confirm the sign-out dialog, **Then** the credential is cleared and the normal sign-in screen is shown.
3. **Given** the device check is shown, **When** the user views the buffer choice, **Then** it carries a label and a short explanation.
4. **Given** the device check, **When** the user answers Yes, "No, try another" or "Skip for now", **Then** each behaves as before.

---

### User Story 4 - Scan the getting-started card and bring it back (Priority: P2)

Once in the library, the user sees a short scannable getting-started card: one line per bundled plugin and a link to the keyboard shortcut sheet instead of inlined shortcut prose. If the user dismissed the card, they can show it again from settings.

**Why this priority**: Improves discoverability of core workflow (UX-10) and removes an irreversible dismissal, but only after the launch gates.

**Independent Test**: Reach the library with the card visible and verify one line per bundled plugin and the shortcuts link; dismiss it, restart, open settings and restore it.

**Acceptance Scenarios**:

1. **Given** the card is shown, **When** the user reads it, **Then** it has one line per bundled plugin and no inlined shortcut lists.
2. **Given** the card is shown, **When** the user activates the shortcuts link, **Then** the keyboard shortcut sheet opens.
3. **Given** the card was dismissed, **When** the user uses the restore control in settings, **Then** the card is shown again, and the dismissed state persists across restarts until restored.

---

### Edge Cases

- Welcome or sign-in screen narrower than the readable measure: text wraps to the available width with no horizontal scroll or clipping.
- Elapsed hint at 0–59 s reads in seconds and from 60 s in minutes with correct plurals (C7). At the existing 5-minute protocol timeout the screen moves to the `TimedOut` status message unchanged.
- Several sign-in reasons: by construction only one can be active (C9); exactly one status message is shown.
- Back from the privacy notice or device check must not alter or lose the disclosure/device state already recorded. Back from the launch-flow device check also keeps the credential; only an explicitly confirmed "Use a different account" signs out (C16). Cancelling the sign-out dialog leaves the summary view showing.
- While the card is visible, the Settings › About restore button is disabled; restoring is idempotent (C14).
- Screen reader and keyboard users: the primary action, status message and indicator have accessible names; activity is not conveyed by animation alone.
- New strings exist in en-US and resolve under `fluent_keys.rs`; pt-BR falls back to en-US until the localization feature (C15). `disclosure.ftl` is byte-identical (C2).

## Requirements *(mandatory)*

### Functional Requirements

This feature implements these requirements from docs/ModPlayer-Software-Specification.md: FR-1.1.1, FR-1.1.2, FR-1.1.3 (welcome and disclosure), FR-1.2.1, FR-1.2.3, FR-1.2.4 (sign-in, tier check, sign-out), FR-1.3.1 (device check), FR-1.4.1 (getting-started panel), NFR-6.1, NFR-6.2 (keyboard and accessibility) and NFR-7.1 (externalized strings). Each FR below lists its source IDs after "Source:".

- **FR-001**: The welcome screen MUST present a heading (`welcome-title`), one lead paragraph (`welcome-description`), and the legal and Premium facts plus the terms and privacy links as one framed compact block below it, in the `secondary` text role (C2, C3). `disclosure.ftl` MUST remain byte-identical. Source: FR-1.1.1, FR-1.1.3.
- **FR-002**: Prose on the welcome screen, privacy notice, sign-in screen and device check MUST be laid out no wider than `theme::body_measure` (72 body-`0` advances), at every window width from the 960 × 640 minimum upward (C1). Source: FR-1.1.1, FR-1.2.1, FR-1.3.1.
- **FR-003**: The welcome screen MUST show exactly one `Variant::Primary` control ("I understand, continue"). "Decline" MUST be `Variant::Default` beside it, after it in Tab order. Their outcomes MUST be unchanged (C4). Source: FR-1.1.2, NFR-6.2.
- **FR-004**: The privacy notice's back control MUST read "Back to welcome" when opened from Welcome and "Back to About" when opened from Settings › About, and MUST return there without changing the disclosure acknowledgement (C5). Source: FR-1.1.1, FR-1.1.2.
- **FR-005**: While `Authorizing`, the sign-in screen MUST show a spinner with an accessible name, the browser-page hint, and an elapsed hint: seconds under 60 s, updated every second, then minutes. Elapsed time is counted from entering `Authorizing` in this process (C6, C7). Source: FR-1.2.1, NFR-6.2.
- **FR-006**: While `Authorizing`, "Open the browser again" MUST be `Variant::Primary` and "Cancel" `Variant::Default` beside it. Their behaviour is unchanged (C4). Source: FR-1.2.1.
- **FR-007**: Every sign-in reason MUST be drawn in the shared status-message style (warning-role accent bar, framed row, accessible status role), placed between the heading and the instructions and so above the retry/continue actions. Reasons: each `SignInNote`, store-unreadable / store-unavailable, Free/Unknown tier result (C8, C9). Source: FR-1.2.3, NFR-6.2.
- **FR-008**: The device check MUST provide a Back control. Opened from Settings › Audio, it returns to Settings › Audio with `skip_device_check` semantics (C10). During the launch flow it MUST open the sign-in summary view ("Signed in as {name}", "Continue" `Variant::Primary`, "Use a different account" `Variant::Default`), and opening it MUST change no account, credential or device state. "Continue" MUST return to the device check unchanged. "Use a different account" MUST go through the existing sign-out confirmation and `sign_out()`, then show the normal sign-in screen. `launch_flow::next_step` MUST NOT change (C16). In every case Back never confirms a device and never changes the disclosure acknowledgement. Source: FR-1.3.1, FR-1.2.4.
- **FR-009**: The device check's buffer-preset combo MUST carry the visible label `setting-buffer-preset`, used as its accessible name, followed by the explanation `setting-buffer-preset-desc` (C11). Source: FR-1.3.1, NFR-6.2.
- **FR-010**: The device check MUST keep its test tone and its three answers with unchanged behaviour. Source: FR-1.3.1.
- **FR-011**: The getting-started card MUST show exactly one line per bundled plugin, the "Keyboard shortcuts" link to 029's sheet, the tutorial link and Dismiss, and no key-chord prose (C13). Source: FR-1.4.1.
- **FR-012**: Settings › About MUST offer a "Show getting-started card" button (searchable through the settings registry) that sets `getting_started_dismissed = false` and persists it. The button is disabled while the card is visible. Dismissed/shown state persists across restarts (C14). Source: FR-1.4.1.
- **FR-013**: All new user-visible strings MUST be Fluent keys in en-US non-hashed files and covered by `fluent_keys.rs` (pt-BR owned by the localization feature, C15). All new controls MUST have accessible names and be keyboard operable (constitution X). Source: NFR-6.1, NFR-6.2, NFR-7.1.
- **FR-014**: The authorization protocol (including the 5-minute pending-attempt timeout), disclosure text/hash/versioning and the Premium check MUST remain unchanged. Source: FR-1.1.3, FR-1.2.1, FR-1.2.3.

### Key Entities

- **Sign-in reason**: Why the sign-in step is showing: a `SignInNote`, store unreadable/unavailable, or a Free/Unknown tier result. At most one exists at a time, by construction (C9).
- **Elapsed wait**: UI-only instant at which `Authorizing` was entered in this process; not persisted (C7).
- **Privacy-notice origin / device-check origin**: UI-only value (Welcome | Settings › About; LaunchFlow | Settings › Audio) that picks the back label and destination (C5, C10).
- **Sign-in summary override**: UI-only flag set by device-check Back during the launch flow. While set, the sign-in slot shows the summary view instead of the device check. It is cleared by Continue, by a completed sign-out or by restart, and it is never persisted (C16).
- **Getting-started card state**: The existing `[onboarding] getting_started_dismissed` setting. Restore sets it to `false` (C14).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: At 960 × 640 and at 1600 px wide, 0 prose rows on the welcome screen or privacy notice are wider than `body_measure` (72 body-`0` advances), and exactly 1 control on the welcome screen is `Variant::Primary`.
- **SC-002**: While `Authorizing`, the spinner and the elapsed hint are visible on the first frame. The hint's text changes every second under 60 s and switches to minutes at 60 s (testable with an injected clock).
- **SC-003**: For every `SignInNote`, for store-unreadable/unavailable, and for the Free/Unknown tier results, the reason is drawn in the status-message style and its rect lies above the retry/continue action's rect.
- **SC-004**: From the device check, the previous step is reachable in one action, and the buffer choice always shows a label and explanation.
- **SC-005**: A user who dismissed the getting-started card can show it again in no more than 2 interactions from the settings screen.
- **SC-006**: In a usability walkthrough, a first-time user can state what the app does and which action continues within 10 seconds of seeing the welcome screen (manual scenario in quickstart.md).

## Assumptions

- The nav rail's behaviour during onboarding (UX-06) is out of scope; only presentation, copy and feedback of the launch flow change.
- The keyboard shortcut sheet from feature 029 exists and can be opened from the getting-started card.
- The readable measure is the existing `theme::body_measure` token (72 characters, C1).
- The device check's "previous step" during the launch flow is a non-destructive sign-in summary view (operator decision Q1 = A, C16). From Settings it is Settings › Audio (C10).
- The elapsed hint is coarse (seconds, then minutes) rather than a precise timer (C7).
- No automatic timeout or behaviour change is added to pending authorization; the screen only reports elapsed time.
- Translation of new strings into pt-BR is delivered by the localization feature. This feature supplies en-US source strings only (C15).
- Authorization protocol, disclosure versioning and the Premium check are out of scope (owned by the first-launch-and-sign-in feature).
