# Contract: Fluent Strings (en-US)

**Feature**: 030-onboarding-clarity. This feature adds en-US strings only (C15). pt-BR falls back to en-US until 007-operations-and-polish/003-localization-pt-br. `locales/en-US/disclosure.ftl` is **not edited** (C2).

Every key below MUST:

- resolve through `modplayer_core::tr` or `tr_args` to a value that differs from the key;
- be listed in `crates/modplayer-ui/tests/fluent_keys.rs`;
- be defined in exactly one en-US file.

## New keys

### `locales/en-US/app.ftl`

```ftl
# 030-onboarding-clarity (C5): the privacy notice's origin-specific back control.
privacy-back-to-welcome = Back to welcome
privacy-back-to-about = Back to About
```

### `locales/en-US/account.ftl`

```ftl
# 030-onboarding-clarity (C6, C7): pending sign-in.
signin-waiting-indicator = Waiting for your browser
signin-browser-hint = A Spotify sign-in page should have opened in your default browser. Finish signing in there, then come back.
signin-waiting-elapsed-seconds = { $count ->
    [one] Waiting for { $count } second
   *[other] Waiting for { $count } seconds
}
signin-waiting-elapsed-minutes = { $count ->
    [one] Waiting for { $count } minute
   *[other] Waiting for { $count } minutes
}

# 030-onboarding-clarity (C16): sign-in summary view (device-check Back during first launch).
signin-summary-title = Signed in
signin-summary-identity = Signed in as { $name }
signin-summary-continue = Continue
signin-summary-switch = Use a different account
```

### `locales/en-US/device-check.ftl`

```ftl
# 030-onboarding-clarity (C10, C16).
device-check-back = Back
```

### `locales/en-US/settings.ftl`

```ftl
# 030-onboarding-clarity (C14): Settings › About.
setting-show-getting-started = Show getting-started card
setting-show-getting-started-desc = Bring back the card on the Library screen that introduces the bundled plugins and links to keyboard shortcuts.
setting-show-getting-started-visible = The getting-started card is already showing.
```

## Reused keys (no change)

| Key | File | New use |
|---|---|---|
| `welcome-title`, `welcome-description`, `disclosure-unofficial`, `disclosure-premium-required`, `disclosure-terms-apply`, `disclosure-terms-link`, `disclosure-privacy-link` | `disclosure.ftl` (hashed) | Welcome hierarchy (W1/W2) |
| `privacy-back` | `disclosure.ftl` (hashed) | still used by About's read-only disclosure view |
| `signin-waiting`, `signin-waiting-resumed`, `signin-open-again`, `signin-cancel`, `signin-note-*`, `signin-store-*`, `tier-*` | `account.ftl` | pending view / status message |
| `account-identity-unavailable` | `account.ftl` | summary identity fallback |
| `signout-confirm-*`, `signout-cancel` | `account.ftl` | summary "Use a different account" dialog |
| `setting-buffer-preset`, `setting-buffer-preset-desc` | `settings.ftl` | device-check buffer field (C11) |

## Test obligations (`tests/fluent_keys.rs`)

- **F1**: a `ONBOARDING_030_KEYS` list holding every argument-free key above. Each resolves through `tr` to a non-key value.
- **F2**:
  - `tr_args("signin-waiting-elapsed-seconds", [("count","1")])` = "Waiting for 1 second", and with `"2"` → "… 2 seconds".
  - The same pattern holds for `-minutes` ("1 minute" / "2 minutes").
  - `tr_args("signin-summary-identity", [("name","Alex")])` contains "Alex".
- **F3**: none of the new keys is defined in `disclosure.ftl` (a guard against C2 drift). The existing `DISCLOSURE_EN_US_SHA256` test in `modplayer-account/tests/disclosure.rs` passes unchanged.
- **F4**: the existing "every key in settings.ftl is exercised" check, where it applies, includes the three new `setting-show-getting-started*` keys.
