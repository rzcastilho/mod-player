# Quickstart: validating First Launch Disclosure and Sign-In

Automated gates run in CI on all three OSes with test doubles
(`MemorySecureStore`, `FakeAuthorizationService`, `FakeClock`); no live
service or unlocked keychain is needed. Manual scenarios cover the real OS
store and the real authorization server.

## Prerequisites

- Toolchain from `rust-toolchain.toml` (1.95), `cargo-deny` installed.
- For manual scenarios: a Spotify account (Premium for M3), a default browser,
  an unlocked OS credential store.

## Automated gates (must all pass)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Tests that prove each requirement (names are the contract; `tasks.md` creates them):

| Test | Location | Proves |
|---|---|---|
| `disclosure_text_is_pinned_to_bundle_version` | `modplayer-account/tests/disclosure.rs` | FR-004 / SC-007 — hash of `disclosure.ftl` matches `DISCLOSURE_EN_US_SHA256` |
| `launch_flow_orders_welcome_signin_devicecheck_main` | `modplayer-account/src/launch_flow.rs` | FR-001, FR-010, FR-012 |
| `acknowledgement_survives_sign_out_and_revocation` | `modplayer-ui/tests/first_launch.rs` | FR-003 |
| `decline_writes_nothing` | `modplayer-ui/tests/first_launch.rs` | FR-002 / SC-006 — no `settings.toml`, no `account.toml`, empty store |
| `sign_in_probes_store_before_opening_browser` | `modplayer-account/tests/sign_in.rs` | FR-017 / SC-008 — with `set_unavailable(true)` no `BrowserUrlReady`, no files written |
| `callback_with_code_stores_credential_then_checks_tier` | `modplayer-account/tests/sign_in.rs` | FR-008, FR-009 |
| `cancel_timeout_and_error_return_to_sign_in_with_no_state` | `modplayer-account/tests/sign_in.rs` | FR-016 / SC-005 |
| `stray_callback_requests_do_not_consume_attempt` | `modplayer-account/src/listener.rs` | authorization-service.md callback table |
| `starting_a_new_attempt_discards_the_previous` | `modplayer-account/tests/sign_in.rs` | FR-016 (single pending) |
| `relaunch_resumes_young_attempt_and_discards_old` | `modplayer-account/tests/launch.rs` | FR-018 |
| `tier_free_and_unknown_gate_playback` | `modplayer-account/tests/tier.rs` | FR-011 |
| `tier_check_times_out_after_30s_keeping_credential` | `modplayer-account/tests/tier.rs` | FR-008 |
| `refresh_is_scheduled_5min_before_expiry_or_half_lifetime` | `modplayer-account/src/refresh.rs` | FR-013 |
| `refresh_backoff_and_signin_again_after_3_failures` | `modplayer-account/tests/refresh.rs` | FR-014 |
| `expiry_without_refresh_enters_expired_and_retains_credential` | `modplayer-account/tests/refresh.rs` | FR-021 |
| `invalid_grant_takes_revocation_path` | `modplayer-account/tests/refresh.rs` | FR-019 |
| `sign_out_clears_registry_stores_and_reports_categories` | `modplayer-account/tests/sign_out.rs` | FR-015 / SC-003 |
| `sign_out_abandons_in_flight_work` | `modplayer-account/tests/sign_out.rs` | FR-015 |
| `store_unreadable_at_launch_clears_nothing` | `modplayer-account/tests/launch.rs` | FR-017 |
| `credential_never_appears_outside_secure_store` | `modplayer-ui/tests/credential_leak.rs` | FR-020 / SC-004 — runs the full flow, then greps the config dir, a captured log sink and every notification for the token bytes |
| `credential_payload_fits_windows_blob_limit` | `modplayer-account/src/credential.rs` | R5 |
| `account_toml_round_trips_any_session` (proptest) | `modplayer-account/src/state_store.rs` | Constitution VIII state-serialization proptest — FR-009 / FR-015: any `AccountSession` survives save → load |
| `credential_payload_round_trips_any_token_set` (proptest) | `modplayer-account/src/credential.rs` | Constitution VIII state-serialization proptest — FR-009 / FR-020: any token set survives `SessionCredential` → payload → `SessionCredential`, within the R5 blob cap |
| `every_account_and_disclosure_key_resolves` | `modplayer-ui/tests/fluent_keys.rs` | FR-023 |
| `every_account_widget_has_an_accessible_name` | `modplayer-ui/src/*` (accesskit walk, like 001) | FR-023 |
| `no_service_trademark_in_branding_keys` | `modplayer-ui/tests/trademark.rs` | FR-006 |
| `keyring_round_trip` `#[ignore = "manual: needs unlocked OS store"]` | `modplayer-secure-store/tests/live.rs` | R5 live verification |

Run one area quickly:

```bash
cargo test -p modplayer-account
cargo test -p modplayer-ui --test credential_leak
```

## Manual scenarios (fresh config dir each time)

```bash
export MODPLAYER_CONFIG_DIR=$(mktemp -d)
cargo run -p modplayer
```

**M1 — First launch and decline (US1)**  
Expect the Welcome screen before anything else: description, three
disclosure statements, terms link, privacy link, "I understand, continue"
enabled immediately, Decline. Press Decline → explanation + Quit; press Quit →
app exits. `ls $MODPLAYER_CONFIG_DIR` is empty. Relaunch → Welcome again.

**M2 — Acknowledge, privacy notice, version bump (US1)**  
Open the privacy notice from Welcome, go back, acknowledge. `settings.toml`
now has `[disclosure] acknowledged_version = 1`. Relaunch → sign-in step, no
Welcome. Edit the file to `acknowledged_version = 0`, relaunch → Welcome
again (simulates a bump).

**M3 — Sign in with a Premium account (US2)**  
Press Sign in → browser opens Spotify's page (no password field in the app);
the app shows "Waiting for your browser…" with Cancel / Open the browser
again. Complete in the browser → tab shows "You can close this tab" → app
shows "Checking your account" → Device Check (first time) → main window.
Verify: `account.toml` has `credential_ref = "session-credential"` and no
token text; the OS store has an entry `ModPlayer` / `session-credential`
(Keychain Access / Credential Manager / Seahorse). `grep -r <token prefix>
$MODPLAYER_CONFIG_DIR` finds nothing.

**M4 — Sign in with a Free account (US2)**  
Same as M3 until the check → "playback requires Premium" screen with Open
upgrade page / Continue → main window without Device Check. Settings › Account
shows tier Free and Re-check subscription.

**M5 — Cancel, timeout, closed tab (US4)**  
(a) Press Cancel while waiting → sign-in step with "cancelled" note + Retry.
(b) Start again, close the browser tab, wait 5 minutes → "timed out" note.
(c) Start again and quit the app; relaunch within 5 minutes → "Waiting for
your browser…" resumes; reload the browser's failed redirect tab (or press
Open the browser again) → signed in. Relaunch after > 5 minutes → note "Your
previous sign-in didn't finish". In every case the store has no
`pending-authorization` entry afterwards.

**M6 — Locked store (US4)**  
macOS: `security lock-keychain`; Linux: lock the default collection in
Seahorse; Windows: not lockable — skip. Press Sign in → refused with the
store name and one remediation line + Retry; no browser opened; no file
written. Unlock and Retry → proceeds. With a signed-in session, lock the
store and relaunch → sign-in step with warning "Couldn't read your saved
sign-in — unlock your <store> and retry"; unlock + Retry → signed in, nothing
was cleared.

**M7 — Revocation and expiry (US4)**  
Revoke access at spotify.com/account/apps → within the next refresh (or press
Re-check subscription) the app shows the critical "session was revoked"
notice, the sign-in step, and the store entry is gone. Expiry: with the
network disconnected, wait past `expires_at` → main window stays, critical
"Session expired — sign in again" notice; reconnect → refresh succeeds and the
notice clears or Sign in again replaces the credential without re-showing
Welcome.

**M8 — Sign out (US3)**  
Settings › Account › Sign out → confirmation lists "your sign-in credential"
and "account details" → confirm → sign-in step within seconds, Info notice
"Signed out. Deleted: …". `account.toml` gone, store entry gone,
`settings.toml` (device, disclosure) unchanged, Welcome not shown.

## Expected outcome

All automated gates green on ubuntu/macos/windows; M1–M8 behave as written
on the developer's platform. Any deviation is a blocking defect for this
slice.
