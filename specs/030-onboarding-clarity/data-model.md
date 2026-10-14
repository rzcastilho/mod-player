# Data Model: Onboarding and Sign-In Clarity

**Feature**: 030-onboarding-clarity | **Date**: 2026-10-13 | **Research**: [research.md](./research.md)

This feature adds **no persisted field and no schema change**. Every new value is UI-only state held in `modplayer-ui`, apart from one controller method that writes an *existing* settings field. Names below are the planned Rust identifiers.

## 1. UI-only state

### 1.1 `PrivacyOrigin` (new, `privacy_notice.rs`) — C5, FR-004

| Variant | Back label key | Back destination | Set by |
|---|---|---|---|
| `Welcome` | `privacy-back-to-welcome` | `WelcomeScreen` `View::Disclosure` | `welcome.rs` |
| `About` | `privacy-back-to-about` | `AboutScreen` `View::Main` | `settings/about.rs` |

- `Copy`, stateless, and passed as an argument. It is never stored or persisted.
- **Invariant**: going back never writes `settings.disclosure`.

### 1.2 `SignInScreen` (extended, `sign_in.rs`) — C6, C7

| Field | Type | Meaning |
|---|---|---|
| `store_unavailable` | `Option<&'static str>` | *existing* |
| `tier_result` | `Option<TierResult>` | *existing* |
| `authorizing_since` | `Option<(u64, f64)>` **new** | `(attempt_id, egui input time in s)` of the frame `Authorizing` was first seen in this process |

**Transitions** of `authorizing_since`, evaluated at the start of every `sign_in::show` frame:

| Current `SessionState` | Stored value | New value |
|---|---|---|
| `Authorizing { attempt_id: a, .. }` | `None` or `Some((b, _))` with `b ≠ a` | `Some((a, now))` |
| `Authorizing { attempt_id: a, .. }` | `Some((a, t))` | unchanged |
| anything else | any | `None` |

- `elapsed_secs = max(0, floor(now − t))`.
- Validation: `now` is monotone in production (eframe). A `now < t` from a test clock clamps to 0, so it never panics.
- Never persisted. A fresh `SignInScreen` per process means a resumed attempt counts from relaunch.

### 1.3 Elapsed hint (derived, pure) — C7

`elapsed_hint(secs: u64) -> String`:

| `secs` | Key | `$count` |
|---|---|---|
| 0 – 59 | `signin-waiting-elapsed-seconds` | `secs` |
| ≥ 60 | `signin-waiting-elapsed-minutes` | `secs / 60` (floor) |

The existing `PENDING_ATTEMPT_TTL` (5 min) still ends the attempt with `SignInNote::TimedOut`. The hint never reaches "6 minutes" in practice, and it is not capped either.

### 1.4 Sign-in reason (derived, not stored) — C8, C9

Exactly one of these, or none, is shown per frame. They are mutually exclusive by the branch order of `sign_in::show`:

1. `screen.tier_result = Some(Free | Unknown)`
2. `screen.store_unavailable = Some(store)`
3. `SessionState::StoreUnreadable`
4. `SessionState::SignedOut { note: Some(n) }`, `n ∈ {Cancelled, TimedOut, ServiceError, PreviousDidNotFinish, Revoked}`

Each of these is drawn with `widgets::status_message` ([contracts/onboarding-screens.md](./contracts/onboarding-screens.md) §S). No reason applies to `SignedOut { note: None }`, `Authorizing`, `Checking`, `Active` or `Expired`.

### 1.5 `DeviceCheckOrigin` + `DeviceCheckOutcome` (new, `device_check.rs`) — C10, C16, FR-008

`DeviceCheckScreen { selected, preset, origin: DeviceCheckOrigin }`

| `DeviceCheckOrigin` | Constructed by | Back does |
|---|---|---|
| `LaunchFlow` | `App::new`, `App::show_launch_gate_device_check` | returns `Back` and calls no controller method. `App` keeps the screen (selection and preset preserved) and sets `sign_in_summary = true` |
| `Settings` | `settings/audio.rs` "Test output device" | calls `controller.skip_device_check()` and returns `Back`. `App::show_main` drops the screen, and Settings › Audio shows again |

`DeviceCheckOutcome`:

- `Open`: keep showing.
- `Closed`: Yes or Skip was chosen. This is the old `true`.
- `Back`.

**Invariant**: Back never calls `confirm_device`, never writes `device_confirmed`, `buffer_preset` or `disclosure`, and never changes `preview_device` state.

### 1.6 `App::sign_in_summary` (new, `app.rs`) — C16

| Field | Type | Default |
|---|---|---|
| `sign_in_summary` | `bool` | `false` |

| Event | Effect |
|---|---|
| Device-check Back with `origin = LaunchFlow` | `true` |
| Summary "Continue" | `false`. The stored `DeviceCheckScreen` is drawn again unchanged |
| Summary "Use a different account" → confirm → `sign_out()` events | session → `SignedOut`, so `launch_step() = SignIn`, so the flag is cleared at the next frame start |
| Summary "Use a different account" → Cancel / Esc / backdrop | unchanged (`true`) |
| `launch_step() != DeviceCheck` at any frame start | `false` |
| Process restart | `false` (fresh `App`) |

While the flag is `true` and `launch_step() == DeviceCheck`, the central panel draws `sign_in::show_summary` instead of the device check. `launch_flow::next_step` is not consulted for this and is unchanged. The flag is never persisted.

### 1.7 `SummaryOutcome` (new, `sign_in.rs`) — C16

- `None`
- `Continue`
- `Events(Vec<AccountEvent>)`: from a confirmed `sign_out()`, routed through `App::handle_account_event`.

**Identity text**: `identity_text(&session.display_name)` (028), which gives the name, or `account-identity-unavailable` when the name is blank. `account.account()` is `Some` whenever `next_step` yields `DeviceCheck`, because that requires `Active | Expired`. If it is `None` anyway, the identity line uses `account-identity-unavailable` and the view still renders.

### 1.8 `AboutOutcome` (new, `settings/about.rs`) — C14

- `None`
- `RestoreGettingStarted`

`settings::show_content` maps the outcome to `controller.restore_getting_started()`.

## 2. Persisted state touched (existing fields only)

### 2.1 `[onboarding] getting_started_dismissed: bool` — C14, FR-012

- Already exists (013): `AudioSettings::getting_started_dismissed`, with the controller shadow `getting_started_dismissed`.
- **New** `PlaybackController::restore_getting_started()`: shadow → `false`, then `persist_settings(|s| s.getting_started_dismissed = false)`. On a save failure it raises `settings-save-failed` (Warning) and the shadow stays `false` for the session, mirroring `dismiss_getting_started`.
- Idempotent: the call when already `false` rewrites the same value. The UI disables the control in that state anyway.

| From | Action | To |
|---|---|---|
| `false` (card visible) | card "Dismiss" | `true` (persisted) |
| `true` | Settings › About "Show getting-started card" | `false` (persisted) |
| `false` | Settings › About button | n/a (disabled) |

### 2.2 Unchanged (explicitly)

- `settings.disclosure` (`DisclosureAcknowledgement`), `DISCLOSURE_BUNDLE_VERSION`, `DISCLOSURE_EN_US_SHA256`: C2, FR-014.
- `device_confirmed`, `buffer_preset`, the preferred device: only changed by the existing Yes / Settings › Audio paths.
- `account.toml`, the secure-store credential, `PendingAttempt` / `PENDING_ATTEMPT_TTL`: FR-014. Changed only by the existing confirmed `sign_out()`.

## 3. Registry entry (new descriptor) — C14

| Field | Value |
|---|---|
| `category` | `SettingsCategory::About` |
| `id` | `about.show_getting_started` |
| `title_key` | `setting-show-getting-started` |
| `description_key` | `setting-show-getting-started-desc` |

This entry means the `placeholder_only_categories_contribute_no_descriptors` unit test no longer lists `About`. `every_descriptor_id_is_unique` still holds.
