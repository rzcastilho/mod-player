# Contract: UI Surface — screens, widgets, Fluent keys, accessibility

Extends 001's [ui-surface.md](../../001-walking-skeleton/contracts/ui-surface.md).
All strings are Fluent keys in `locales/en-US/disclosure.ftl` (disclosure +
privacy notice, hash-pinned) and `locales/en-US/account.ftl` (everything
else). Every widget's visible text is `tr(key)`, which is its accessible name
(001 convention); every screen is keyboard-operable (Tab/Shift-Tab/Space/Enter).
en-US is the only locale (FR-023).

## Launch gate order (App)

`App` evaluates `launch_flow::next_step(...)` each frame and renders the
central panel exclusively for `Welcome`, `SignIn`, and `DeviceCheck` (the
existing early-return pattern in `app.rs`); the nav rail and notification
area render as before. Order: **Welcome → SignIn → DeviceCheck → Main**.

## Screens

### Welcome (`welcome.rs`) — shown when `acknowledged_version != DISCLOSURE_BUNDLE_VERSION`

| Widget | Kind | Key |
|---|---|---|
| Heading | label | `welcome-title` |
| Product description (one paragraph) | label | `welcome-description` |
| Unofficial-connection disclosure | label | `disclosure-unofficial` |
| Premium required | label | `disclosure-premium-required` |
| Subject to service terms | label | `disclosure-terms-apply` |
| Service terms link | hyperlink (opens `TERMS_URL` in system browser) | `disclosure-terms-link` |
| Privacy notice link | button → Privacy Notice screen | `disclosure-privacy-link` |
| Acknowledge | button, enabled immediately, default focus | `welcome-acknowledge` ("I understand, continue") |
| Decline | button | `welcome-decline` |

Decline view (same screen, replaces content): label `decline-explanation`,
button `decline-quit` (single action → `ctx.send_viewport_cmd(ViewportCommand::Close)`).
Nothing is written.

### Privacy Notice (`privacy_notice.rs`) — reachable from Welcome and Settings › About

Scrollable text: `privacy-title`, `privacy-body` (multi-paragraph Fluent
message), button `privacy-back`. Identical content from both entry points.

### Sign-in step (`sign_in.rs`) — renders `SessionState` sub-states

| Sub-state | Widgets (keys) |
|---|---|
| SignedOut / Expired / StoreUnreadable | `signin-title`, `signin-explanation`, optional note label (`signin-note-cancelled`, `signin-note-timed-out`, `signin-note-service-error`, `signin-note-previous-unfinished`, `signin-note-revoked`), button `signin-start` ("Sign in") or `signin-retry` ("Retry") when a note is present |
| Store unavailable (after probe failure) | `signin-store-unavailable` with `{ $store }` = `tr(store_name_key)`, `signin-store-remedy-<platform>` (one remediation line), button `signin-retry` |
| Authorizing | `signin-waiting` ("Waiting for your browser…"), `signin-waiting-resumed` line when resumed, buttons `signin-open-again` ("Open the browser again"), `signin-cancel` ("Cancel") |
| Checking | `signin-checking` ("Checking your account"), spinner (`WidgetType::ProgressIndicator` with accessible name `signin-checking`) |
| Tier result: Free | `tier-free-title`, `tier-free-explanation`, buttons `tier-open-upgrade` (opens `UPGRADE_URL`), `tier-continue` |
| Tier result: Unknown | `tier-unknown-title` ("Couldn't verify your subscription"), `tier-unknown-explanation`, buttons `tier-retry`, `tier-continue` |

Premium result shows nothing: the flow advances to Device Check / Main.
The tier-result view is shown once after `TierChecked`/`TierCheckFailed`
following sign-in or launch validation; a re-check from Settings shows its
result inline in Settings › Account instead.

No `TextEdit` exists anywhere on this screen (FR-007: no password field).

### Settings › Account (`settings/account.rs`)

Signed in:

| Widget | Key |
|---|---|
| Display name | `account-display-name` `{ $name }` |
| Tier | `account-tier` + `tier-premium` / `tier-free` / `tier-unknown` |
| Last validated | `account-last-validated` `{ $when }` / `account-never-validated` |
| Re-check subscription | button `account-recheck` |
| Sign out | button `account-sign-out` → confirmation |

Signed out: label `account-signed-out`, button `account-sign-in` (goes to the
sign-in step — the whole window, since signed-out states never show Main).

Sign-out confirmation (modal `egui::Modal`): `signout-confirm-title`,
`signout-confirm-intro`, one bullet per registry category
(`signout-category-credential`, `signout-category-account-details`),
buttons `signout-confirm` (destructive), `signout-cancel` (default focus).

Settings registry descriptors added (searchable): `account-recheck`,
`account-sign-out` (category Account). The `placeholder_only_categories…`
test is updated to exclude Account and About.

### Settings › About (`settings/about.rs`)

`about-product` (same text as `welcome-description`), `about-version` `{ $version }`
(`env!("CARGO_PKG_VERSION")`), button `about-privacy` → Privacy Notice, button
`about-disclosure` → read-only disclosure view (`disclosure-*` labels, button
`privacy-back`; no acknowledge/decline).

## Notifications (keys in `account.ftl`, constants in `modplayer-account::notify`)

| Key | Severity | Args | Raised on |
|---|---|---|---|
| `signin-again` | Warning | — | `RefreshFailing` (action button `signin-again-action` → sign-in step) |
| `session-expired` | Critical | — | `SessionExpired` (action `signin-again-action`) |
| `session-revoked` | Critical | — | `SessionRevoked` (action `signin-action`) |
| `signed-out` | Info | `{ $categories }` | `SignedOut` |
| `signout-incomplete` | Warning | `{ $category }` | `SignOutIncomplete` |
| `store-unreadable` | Warning | `{ $store }` | launch `StoreUnreadable` (action `signin-retry`) |
| `settings-save-failed` (001) | Warning | — | acknowledgement or account.toml write failure |

Notification **actions** are new to the notification UI: `Notification`
gains `action: Option<NotificationAction>` (an enum in `modplayer-core`:
`SignIn`) rendered as a second button; the UI maps `SignIn` to "go to the
sign-in step / start sign-in". `NotificationCenter` gains
`dismiss_by_key(key)` so `RefreshRecovered` can clear `signin-again`.
The accessibility test's button count in `shell.rs` is updated accordingly.

## Fluent key test

`crates/modplayer-ui/tests/fluent_keys.rs` gains `ACCOUNT_KEYS` and
`DISCLOSURE_KEYS` tables listing every key above; the existing test asserts
`tr(key) != key` for each.

## Trademark rule (FR-006)

Product name "ModPlayer", window title, application identifier
(`ProjectDirs::from("", "ModPlayer", "ModPlayer")`) and any icon contain no
service name or mark. Body text keys may say "Spotify" descriptively; a test
asserts that no key whose name starts with `nav-`, `app-`, `welcome-title`,
`about-product` resolves to text containing the service name.
