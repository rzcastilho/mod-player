# Tasks: First Launch Disclosure and Sign-In

**Input**: Design documents from `/specs/002-first-launch-and-sign-in/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included — spec.md's clarifications and quickstart.md name specific tests per requirement (FR-020/SC-004 leak test, FR-004/SC-007 snapshot guard, etc.); these are treated as explicitly requested.

**Organization**: Tasks are grouped by user story (spec.md priorities P1/P1/P2/P2). Foundational work that must exist before any story compiles/tests sits in Phase 2.

## Format: `[ID] [P?] [Story] Description *(requirement IDs)*`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 (disclosure), US2 (sign-in/tier), US3 (sign-out), US4 (interruption/revocation recovery)
- File paths are exact, relative to repository root
- **Requirement IDs** (trailing italics, constitution Governance traceability rule): spec.md `FR-0nn` / `SC-0nn`, source-document `C-n` / `NFR-` / `GOV-` / `DM-` / `EC-` / `INT-` IDs where spec.md cites them, and `Constitution <principle>` for work that exists only to satisfy a constitution principle (quality gates, test doubles, proptests)

---

## Phase 1: Setup

**Purpose**: Workspace plumbing so the two new crates exist and build.

- [X] T001 Add workspace dependencies (`keyring = "4.2"`, `ureq = "3.4"`, `serde_json = "1"`, `sha2 = "0.10"`, `base64 = "0.22"`, `getrandom = "0.3"`, `time = "0.3"`, and dev-only `proptest = "1"`) to root `Cargo.toml` *(FR-007, FR-009, NFR-4.1; Constitution VII/VIII/X)*
- [X] T002 [P] Add `CDLA-Permissive-2.0` (webpki-roots) and `Apache-2.0 AND ISC` (ring) allow entries with crate-naming comments to `deny.toml` *(Constitution VII)*
- [X] T003 [P] Add `crates/modplayer-secure-store/` entry to `CODEOWNERS` *(Constitution VI; Governance)*
- [X] T004 Create `crates/modplayer-secure-store/Cargo.toml` (depends on `keyring`, `thiserror`; `#![forbid(unsafe_code)]`; add to workspace members) *(FR-009, FR-017, NFR-4.1, C-3; Constitution VII/X)*
- [X] T005 Create `crates/modplayer-account/Cargo.toml` (depends on `modplayer-secure-store`, `ureq`, `serde_json`, `sha2`, `base64`, `getrandom`, `time`, `thiserror`; `[dev-dependencies] proptest`; `#![forbid(unsafe_code)]`; add to workspace members) *(FR-007, FR-009; Constitution IV/VII/VIII)*
- [X] T006 Add `modplayer-account` and `modplayer-secure-store` dependencies to `crates/modplayer-ui/Cargo.toml` *(FR-001, FR-007; Constitution VII)*
- [X] T007 [P] Create `locales/en-US/disclosure.ftl` with SPDX header (empty message table, filled in US1) *(FR-001, FR-005, FR-023, NFR-7.1)*
- [X] T008 [P] Create `locales/en-US/account.ftl` with SPDX header (empty message table, filled in US2/US3/US4) *(FR-023, NFR-7.1)*

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared traits, doubles, crate skeletons, and App wiring every user story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T009 [P] Define `SecureStore` trait, `EntryName`, `SecureStoreError` in `crates/modplayer-secure-store/src/store.rs` (contracts/secure-store.md) *(FR-009, FR-017, FR-018, NFR-4.1; Constitution VI)*
- [X] T010 [P] Implement `KeyringSecureStore` in `crates/modplayer-secure-store/src/keyring_store.rs` (depends on T009) *(FR-009, FR-017, NFR-4.1, C-3)*
- [X] T011 [P] Implement `MemorySecureStore` (`set_unavailable`, `set_fail_writes`, `entries()`, `written_bytes_log()`) in `crates/modplayer-secure-store/src/memory_store.rs` (depends on T009) *(FR-020, SC-004, SC-008; Constitution VIII)*
- [X] T012 Wire `crates/modplayer-secure-store/src/lib.rs` public exports (depends on T009-T011) *(FR-009; Constitution VII)*
- [X] T013 [P] `#[ignore = "manual"]` live round-trip test `keyring_round_trip` in `crates/modplayer-secure-store/tests/live.rs` (depends on T012) *(FR-009, NFR-4.1 (research R5))*
- [X] T014 [P] Define `Clock` trait, `SystemClock`, `FakeClock` (`advance(d)`) in `crates/modplayer-account/src/clock.rs` *(FR-013, FR-014, FR-016, FR-018; Constitution VIII)*
- [X] T015 [P] Define `AuthorizationService` trait, `ClientConfig`, `TokenSet`, `Profile`, `AuthError` (redacting `Debug`) in `crates/modplayer-account/src/auth_service.rs` *(FR-007, FR-020; Constitution IV/VI)*
- [X] T016 [P] Scaffold `ClientConfig` defaults (librespot Keymaster `client_id`, `MODPLAYER_OAUTH_CLIENT_ID` env override, `redirect_path = "/login"`, scopes, endpoint URLs) and `SpotifyAuthorizationService` struct with stub trait-method bodies in `crates/modplayer-account/src/spotify.rs` (depends on T015) *(FR-007, INT-1 (research R4))*
- [X] T017 [P] Implement `FakeAuthorizationService` (scripted results, call log, per-call delay, `browser` hook) in `crates/modplayer-account/src/fake_auth.rs` (depends on T015) *(FR-020, SC-004, SC-005; Constitution IV/VIII)*
- [X] T018 [P] Define `Tier` enum + serde mapping in `crates/modplayer-account/src/session.rs` *(FR-008, FR-011, DM-1)*
- [X] T019 Define `SessionState`, `SignInNote`, `AccountSession` (`credential_ref`, `playback_permitted()`) in `crates/modplayer-account/src/session.rs` (depends on T018, same file) *(FR-009, FR-011, FR-012, FR-021, DM-1)*
- [X] T020 [P] Define `SessionCredential` + private `CredentialPayload` wire struct with redacting `Debug`, no `Display` in `crates/modplayer-account/src/credential.rs` *(FR-009, FR-020, NFR-4.1; Constitution VI)*
- [X] T021 [P] Define `PendingAuthorization` with redacting `Debug` in `crates/modplayer-account/src/pending.rs` *(FR-018, FR-020, EC-1.1)*
- [X] T022 [P] Define disclosure bundle constants (`DISCLOSURE_BUNDLE_VERSION = 1`, `DISCLOSURE_EN_US_SHA256` placeholder, `TERMS_URL`, `UPGRADE_URL`) in `crates/modplayer-account/src/disclosure.rs` *(FR-001, FR-004, SC-007, NFR-11.4)*
- [X] T023 [P] Implement `LaunchStep` enum + `launch_flow::next_step()` in `crates/modplayer-account/src/launch_flow.rs` (data-model.md §2.4) *(FR-001, FR-010, FR-012)*
- [X] T024 [P] Add `disclosure: Option<DisclosureAcknowledgement>` to the settings domain struct and `[disclosure]` section (`acknowledged_version`, `acknowledged_at`, `#[serde(default)]`) to `RawSettings` in `crates/modplayer-core/src/settings/model.rs` *(FR-003, FR-004, DM-27)*
- [X] T025 [P] Define `AccountScopedStore` trait in `crates/modplayer-account/src/registry.rs` *(FR-015, FR-019, NFR-5.4)*
- [X] T026 [P] Implement `account.toml` load/save/delete (tmp + `sync_all` + rename) as `AccountStateStore` in `crates/modplayer-account/src/state_store.rs` *(FR-009, FR-015; Constitution VIII (state serialization))*
- [X] T027 Scaffold `AccountService` struct (`secure`, `auth`, `clock`, `state_store`, `registry`, channels) + `new()` + minimal `launch()`/`tick()` (SignedOut-only path) in `crates/modplayer-account/src/service.rs` (depends on T009-T026) *(FR-012, FR-013, DM-1)*
- [X] T028 [P] Define notification key constants module in `crates/modplayer-account/src/notify.rs` *(FR-014, FR-015, FR-017, FR-019, FR-021, FR-023)*
- [X] T029 Wire `crates/modplayer-account/src/lib.rs` public exports (depends on T014-T028) *(Constitution VII)*
- [X] T030 [P] Add `NotificationAction` enum (`SignIn`), `action: Option<NotificationAction>` field, `dismiss_by_key()` to `crates/modplayer-core/src/notifications.rs` *(FR-014, FR-019, FR-021)*
- [X] T031 Render the notification action button in `crates/modplayer-ui/src/notifications.rs` (depends on T030) *(FR-014, FR-019, FR-021, FR-023, NFR-6.1)*
- [X] T032 Wire `crates/modplayer/src/main.rs` to construct `KeyringSecureStore`, `SpotifyAuthorizationService`, `SystemClock`, `AccountService`, call `launch()`, pass to `App` (depends on T012, T016, T027) *(FR-007, FR-009, FR-013, C-3)*
- [X] T033 `App` owns `AccountService`, evaluates `launch_flow::next_step()` every frame, gates central-panel rendering (Welcome/SignIn/DeviceCheck/Main early-return) in `crates/modplayer-ui/src/app.rs` (depends on T023, T027, T032) *(FR-001, FR-010, FR-012, SC-001)*

**Checkpoint**: Workspace builds; app launches straight to a placeholder sign-in gate. No story-specific screen exists yet.

---

## Phase 3: User Story 1 - First-launch disclosure (Priority: P1) 🎯 MVP

**Goal**: Welcome screen with disclosure text gates everything; decline exits cleanly and writes nothing; acknowledgement persists device-scoped across sign-out/revocation and re-prompts on a version bump.

**Independent Test**: Fresh install, launch with no local state — welcome screen with disclosure text and acknowledgement action appears before any account/playback UI; declining exits the app with an explanation and leaves no signed-in state.

### Tests for User Story 1

- [X] T034 [P] [US1] Test `disclosure_text_is_pinned_to_bundle_version` in `crates/modplayer-account/tests/disclosure.rs` *(FR-004, SC-007, NFR-11.4)*
- [X] T035 [P] [US1] Test `launch_flow_orders_welcome_signin_devicecheck_main` (inline `#[cfg(test)]`) in `crates/modplayer-account/src/launch_flow.rs` *(FR-001, FR-010, FR-012, SC-001)*
- [X] T036 [P] [US1] Test `decline_writes_nothing` in `crates/modplayer-ui/tests/first_launch.rs` *(FR-002, SC-006)*

### Implementation for User Story 1

- [X] T037 [US1] Write full `disclosure.ftl` content: product description, unofficial-connection disclosure, Premium-required statement, terms-apply statement, terms link, privacy link, `welcome-acknowledge`, `welcome-decline`, `decline-explanation`, `decline-quit`, `privacy-title`, `privacy-body`, `privacy-back` in `locales/en-US/disclosure.ftl` *(FR-001, FR-005, FR-006, NFR-5.5, NFR-11.3, GOV-6.1)*
- [X] T038 [US1] Set `DISCLOSURE_EN_US_SHA256` to the SHA-256 of `disclosure.ftl` in `crates/modplayer-account/src/disclosure.rs` (depends on T037) *(FR-004, SC-007)*
- [X] T039 [P] [US1] Implement Welcome + Decline views in `crates/modplayer-ui/src/welcome.rs` *(FR-001, FR-002, FR-023, SC-001)*
- [X] T040 [P] [US1] Implement Privacy Notice view (scrollable, readable offline) in `crates/modplayer-ui/src/privacy_notice.rs` *(FR-005, NFR-5.5)*
- [X] T041 [US1] Wire Welcome/Decline/Privacy-Notice rendering into the launch gate in `crates/modplayer-ui/src/app.rs` (depends on T039, T040) *(FR-001, FR-002, FR-005, SC-001)*
- [X] T042 [US1] Implement the acknowledge action: record `DisclosureAcknowledgement` (current version + timestamp) via the existing atomic `settings.toml` save, raise `settings-save-failed` warning and still proceed on write failure, in `crates/modplayer-ui/src/welcome.rs` (depends on T024, T041) *(FR-003, DM-27)*
- [X] T043 [US1] Implement the decline action: write nothing, `ctx.send_viewport_cmd(ViewportCommand::Close)` in `crates/modplayer-ui/src/welcome.rs` *(FR-002, SC-006)*
- [X] T044 [P] [US1] Add `DISCLOSURE_KEYS` table + `tr(key) != key` assertions to `crates/modplayer-ui/tests/fluent_keys.rs` *(FR-023, NFR-7.1)*
- [X] T045 [US1] Round-trip test of the `[disclosure]` settings section in `crates/modplayer-core/tests/settings.rs` *(FR-003, FR-004; Constitution VIII)*
- [X] T046 [P] [US1] Add `welcome-title` no-trademark assertion to `crates/modplayer-ui/tests/trademark.rs` (extended again in US2 for `about-product`) *(FR-006, NFR-11.3, C-8)*

**Checkpoint**: US1 fully functional and independently testable — welcome/decline/privacy notice work end to end; nothing downstream exists yet.

---

## Phase 4: User Story 2 - Sign-in and tier verification (Priority: P1)

**Goal**: Browser-based PKCE sign-in with no password field anywhere; credential stored only in the OS secure store; tier check gates Device Check (Premium) vs. a browse-only explanation (Free/Unknown); credential refreshed before expiry with backoff.

**Independent Test**: From the post-disclosure state, trigger sign-in, complete authorization in a browser, confirm the app reaches Device Check (Premium) or the browse-only screen (non-Premium), credential only in the OS secure store.

### Tests for User Story 2

- [X] T047 [P] [US2] Test `sign_in_probes_store_before_opening_browser` in `crates/modplayer-account/tests/sign_in.rs` *(FR-017, SC-008)*
- [X] T048 [P] [US2] Test `callback_with_code_stores_credential_then_checks_tier` in `crates/modplayer-account/tests/sign_in.rs` *(FR-008, FR-009)*
- [X] T049 [P] [US2] Test `starting_a_new_attempt_discards_the_previous` in `crates/modplayer-account/tests/sign_in.rs` *(FR-016)*
- [X] T050 [P] [US2] Test `stray_callback_requests_do_not_consume_attempt` (inline) in `crates/modplayer-account/src/listener.rs` *(FR-016, FR-018 (contracts/authorization-service.md callback table))*
- [X] T051 [P] [US2] Test `tier_free_and_unknown_gate_playback` in `crates/modplayer-account/tests/tier.rs` *(FR-011)*
- [X] T052 [P] [US2] Test `tier_check_times_out_after_30s_keeping_credential` in `crates/modplayer-account/tests/tier.rs` *(FR-008)*
- [X] T053 [P] [US2] Test `refresh_is_scheduled_5min_before_expiry_or_half_lifetime` (inline) in `crates/modplayer-account/src/refresh.rs` *(FR-013)*
- [X] T054 [P] [US2] Test `refresh_backoff_and_signin_again_after_3_failures` in `crates/modplayer-account/tests/refresh.rs` *(FR-014)*
- [X] T055 [P] [US2] Test `credential_payload_fits_windows_blob_limit` (inline) in `crates/modplayer-account/src/credential.rs` *(FR-009, NFR-4.1 (research R5))*
- [X] T056 [P] [US2] Test `credential_never_appears_outside_secure_store` in `crates/modplayer-ui/tests/credential_leak.rs` *(FR-020, NFR-4.1, SC-004)*
- [X] T107 [P] [US2] Proptest `account_toml_round_trips_any_session` (inline `#[cfg(test)]`, `proptest!` over arbitrary `AccountSession` field values incl. every `Tier`, optional `last_validated_at`, arbitrary RFC 3339 timestamps and Unicode display names: save → load == original) in `crates/modplayer-account/src/state_store.rs` *(FR-009, FR-015; Constitution VIII — state serialization proptest, plan.md Complexity Tracking)*
- [X] T108 [P] [US2] Proptest `credential_payload_round_trips_any_token_set` (inline `#[cfg(test)]`, `proptest!` over arbitrary token/refresh-token/scope strings and timestamps: `SessionCredential` → JSON payload → `SessionCredential` == original, and the payload never exceeds the Windows blob cap for inputs within the maximal realistic lengths) in `crates/modplayer-account/src/credential.rs` *(FR-009, FR-020; Constitution VIII — state serialization proptest, plan.md Complexity Tracking)*

### Implementation for User Story 2

- [X] T057 [US2] Implement PKCE verifier/challenge/state generation (`getrandom`, `sha2`, `base64`) in `crates/modplayer-account/src/pkce.rs` *(FR-007; Constitution VI (research R2))*
- [X] T058 [US2] Implement the loopback `TcpListener` thread, request-line parser, HTML response page, and callback routing table (code/error/state-mismatch/other-path) in `crates/modplayer-account/src/listener.rs` (depends on T057) *(FR-007, FR-016, FR-018; Constitution VI)*
- [X] T059 [US2] Implement `SpotifyAuthorizationService` method bodies (`authorization_url`, `exchange_code`, `refresh`, `fetch_profile` via `ureq`, 30 s timeout, one 429 retry) in `crates/modplayer-account/src/spotify.rs` (depends on T016, T057) *(FR-007, FR-008, FR-013, FR-019, INT-1)*
- [X] T060 [US2] Implement `start_sign_in()`/`cancel_sign_in()`/`browser_url()` commands and `Authorizing`/`Checking` transitions in `crates/modplayer-account/src/service.rs` (depends on T058, T059) *(FR-007, FR-016, FR-017)*
- [X] T061 [US2] Implement the `account-exchange` worker: write credential → write `account.toml` (tier `Unknown`) → delete `pending-authorization` → emit `Authorized`, in `crates/modplayer-account/src/service.rs` (depends on T060) *(FR-008, FR-009, FR-018)*
- [X] T062 [US2] Implement the `account-tier` worker: `fetch_profile` → map `Tier` → emit `TierChecked`/`TierCheckFailed`, in `crates/modplayer-account/src/service.rs` (depends on T061) *(FR-008, FR-010, FR-011)*
- [X] T063 [US2] Implement `RefreshScheduler` (`due_at`, exponential backoff 1s→60s cap, `consecutive_failures`, `RefreshFailing`/`RefreshRecovered`) in `crates/modplayer-account/src/refresh.rs` (depends on T059) *(FR-013, FR-014)*
- [X] T064 [US2] Wire `tick()` to drain the worker channel, advance the refresh scheduler, and return `Vec<AccountEvent>` in `crates/modplayer-account/src/service.rs` (depends on T062, T063) *(FR-013, FR-014, FR-016)*
- [X] T065 [US2] Write sign-in/tier/account Fluent keys (`signin-*`, `tier-*`, `account-*`) in `locales/en-US/account.ftl` *(FR-007, FR-008, FR-011, FR-022, FR-023, NFR-7.1)*
- [X] T066 [US2] Implement the Sign-in screen sub-states (SignedOut/Authorizing/Checking/Tier-Free/Tier-Unknown; no `TextEdit` anywhere) in `crates/modplayer-ui/src/sign_in.rs` (depends on T065) *(FR-007, FR-008, FR-011, FR-012, FR-023)*
- [X] T067 [US2] Implement the Settings › Account signed-in view (display name, tier, last validated, Re-check subscription) in `crates/modplayer-ui/src/settings/account.rs` (depends on T065) *(FR-011, FR-022)*
- [X] T068 [US2] Add Account/About descriptor title keys to `locales/en-US/settings.ftl` *(FR-005, FR-022, FR-023)*
- [X] T069 [US2] Add Account (Re-check subscription, Sign out) settings registry descriptors in `crates/modplayer-core/src/settings_registry.rs` (depends on T068) *(FR-011, FR-015, FR-022)*
- [X] T070 [US2] Update the `placeholder_only_categories` test to exclude Account and About in `crates/modplayer-core/src/settings_registry.rs` (depends on T069) *(FR-022)*
- [X] T071 [US2] Map `BrowserUrlReady` (→ `ctx.open_url`), `Authorized`, `TierChecked`, `TierCheckFailed`, `RefreshFailing` (→ `signin-again` warning), `RefreshRecovered` (→ dismiss) events to notifications/screens in `crates/modplayer-ui/src/app.rs` (depends on T064, T066) *(FR-007, FR-010, FR-011, FR-014)*
- [X] T072 [US2] Implement the Settings › About screen (product description, version, Privacy notice, read-only Disclosure) in `crates/modplayer-ui/src/settings/about.rs` (depends on T065) *(FR-005, FR-006, NFR-5.5)*
- [X] T073 [US2] Wire `crates/modplayer-ui/src/settings/mod.rs` to add the Account and About panels (depends on T067, T072) *(FR-005, FR-022)*
- [X] T074 [P] [US2] Extend `ACCOUNT_KEYS` in `crates/modplayer-ui/tests/fluent_keys.rs` with sign-in/tier/account/about keys *(FR-023, NFR-7.1)*
- [X] T075 [P] [US2] Extend `crates/modplayer-ui/tests/trademark.rs` to check `about-product` *(FR-006, NFR-11.3)*
- [X] T076 [US2] Extend the accessibility walk (accessible name/role/state for every new widget) in `crates/modplayer-ui/src/shell.rs` *(FR-023, NFR-6.1, NFR-6.2; Constitution X)*
- [X] T077 [US2] Run `cargo test -p modplayer-account` and `cargo test -p modplayer-ui --test credential_leak`; confirm all US2 tests (including proptests T107/T108) pass with `MemorySecureStore` + `FakeAuthorizationService` + `FakeClock` *(SC-004, SC-005, SC-008; Constitution VIII)*

**Checkpoint**: US1 + US2 fully functional — sign-in, tier check, refresh, and Settings › Account/About all work end to end. This is the MVP.

---

## Phase 5: User Story 3 - Sign-out (Priority: P2)

**Goal**: Explicit, confirmed sign-out that removes the credential and all account-scoped state within 10 seconds, leaving plugins, non-account settings, and the disclosure acknowledgement untouched.

**Independent Test**: From a signed-in state, trigger sign-out, confirm the listed deletions, verify within seconds that the credential and account-scoped state are gone while plugin installs and non-account settings remain.

### Tests for User Story 3

- [X] T078 [P] [US3] Test `sign_out_clears_registry_stores_and_reports_categories` in `crates/modplayer-account/tests/sign_out.rs` *(FR-015, SC-003)*
- [X] T079 [P] [US3] Test `sign_out_abandons_in_flight_work` in `crates/modplayer-account/tests/sign_out.rs` *(FR-015)*

### Implementation for User Story 3

- [X] T080 [US3] Implement `CredentialStore` as `AccountScopedStore` (clears `session-credential` + `pending-authorization`) in `crates/modplayer-account/src/registry.rs` (depends on T025) *(FR-015, FR-019)*
- [X] T081 [US3] Implement `AccountScopedStore` for `AccountStateStore` (`clear()` deletes `account.toml`) in `crates/modplayer-account/src/state_store.rs` (depends on T025, T026) *(FR-015, FR-019)*
- [X] T082 [US3] Implement `sign_out()`: bump `attempt_id`, `clear()` every registry store (collecting per-store failures), delete `pending-authorization`, state `SignedOut { None }`, emit `SignedOut { categories }` / `SignOutIncomplete { category }` in `crates/modplayer-account/src/service.rs` (depends on T080, T081) *(FR-015, SC-003, NFR-5.4)*
- [X] T083 [US3] Implement `signout_categories()` (`Vec<&'static str>` in registry order) in `crates/modplayer-account/src/service.rs` (depends on T082) *(FR-015)*
- [X] T084 [US3] Add `signout-confirm-title`, `signout-confirm-intro`, `signout-category-credential`, `signout-category-account-details`, `signout-confirm`, `signout-cancel` Fluent keys to `locales/en-US/account.ftl` *(FR-015, FR-023)*
- [X] T085 [US3] Implement the sign-out confirmation modal (`egui::Modal`, category bullets from `signout_categories()`, destructive Sign out / default-focus Cancel) in `crates/modplayer-ui/src/settings/account.rs` (depends on T083, T084) *(FR-015, FR-023, NFR-6.1)*
- [X] T086 [US3] Map `SignedOut` (Info, joined category labels) and `SignOutIncomplete` (Warning) events to notifications in `crates/modplayer-ui/src/app.rs` (depends on T082) *(FR-015)*
- [X] T087 [US3] Implement the signed-out Settings › Account view (`account-signed-out` label, `account-sign-in` button → sign-in step) in `crates/modplayer-ui/src/settings/account.rs` *(FR-012, FR-022)*

**Checkpoint**: US1 + US2 + US3 all independently functional — sign-out works end to end without disturbing the disclosure acknowledgement or non-account settings.

---

## Phase 6: User Story 4 - Interrupted or revoked authorization recovery (Priority: P2)

**Goal**: Cancel, timeout, app-closed-mid-flow, locked/unavailable store, remote revocation, and expiry all resolve to a defined, explained state with no partial data.

**Independent Test**: Individually simulate cancellation, timeout, app-closed-during-auth, and remote revocation; confirm each resolves to a clean, well-explained state with no orphaned session or cache data.

### Tests for User Story 4

- [X] T088 [P] [US4] Test `cancel_timeout_and_error_return_to_sign_in_with_no_state` in `crates/modplayer-account/tests/sign_in.rs` *(FR-016, SC-005)*
- [X] T089 [P] [US4] Test `relaunch_resumes_young_attempt_and_discards_old` in `crates/modplayer-account/tests/launch.rs` *(FR-018, SC-005)*
- [X] T090 [P] [US4] Test `store_unreadable_at_launch_clears_nothing` in `crates/modplayer-account/tests/launch.rs` *(FR-017)*
- [X] T091 [P] [US4] Test `expiry_without_refresh_enters_expired_and_retains_credential` in `crates/modplayer-account/tests/refresh.rs` *(FR-021, SC-005)*
- [X] T092 [P] [US4] Test `invalid_grant_takes_revocation_path` in `crates/modplayer-account/tests/refresh.rs` *(FR-019, SC-005)*

### Implementation for User Story 4

- [X] T093 [US4] Implement cancel/timeout/definitive-error handling → `SignedOut(note)` + delete `pending-authorization`, at most one attempt pending in `crates/modplayer-account/src/service.rs` *(FR-016, SC-005)*
- [X] T094 [US4] Implement `launch()` resume logic: pending younger than 5 min + port re-bindable → `Authorizing { resumed: true }`; otherwise delete + `SignedOut(PreviousDidNotFinish)` in `crates/modplayer-account/src/service.rs` (depends on T093) *(FR-018, SC-005)*
- [X] T095 [US4] Implement `StoreUnreadable` detection at launch (session exists, secure store read fails, nothing cleared, not treated as revocation) in `crates/modplayer-account/src/service.rs` *(FR-017)*
- [X] T096 [US4] Implement the store-unavailable/unreadable sign-in UI (`{ $store } = tr(platform_name_key)`, one remediation line, Retry) in `crates/modplayer-ui/src/sign_in.rs` (depends on T095) *(FR-017, FR-023, SC-008)*
- [X] T097 [US4] Implement the revocation path (delete credential → delete `pending-authorization` → `clear()` every registry store → `SignedOut(Revoked)` → emit `SessionRevoked`) in `crates/modplayer-account/src/service.rs` (depends on T080, T081) *(FR-019, SC-005)*
- [X] T098 [US4] Implement the expiry path (`now >= expires_at` with no successful refresh → `Expired`, credential retained, emit `SessionExpired` once) in `crates/modplayer-account/src/refresh.rs` (depends on T063) *(FR-021, SC-005)*
- [X] T099 [US4] Add `signin-note-cancelled`/`-timed-out`/`-service-error`/`-previous-unfinished`/`-revoked`, `store-name-keychain`/`-credential-manager`/`-secret-service`, `signin-store-remedy-<platform>`, `session-expired`, `session-revoked`, `store-unreadable` Fluent keys to `locales/en-US/account.ftl` *(FR-016, FR-017, FR-018, FR-019, FR-021, FR-023)*
- [X] T100 [US4] Map `SessionExpired` (Critical, `signin-again-action`), `SessionRevoked` (Critical, `signin-action`), `StoreUnavailable`/`StoreUnreadable` (Warning, `signin-retry`) events to notifications in `crates/modplayer-ui/src/app.rs` (depends on T097, T098) *(FR-017, FR-019, FR-021)*

**Checkpoint**: All four user stories independently functional. Every interruption/revocation/expiry scenario in SC-005 resolves cleanly.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T101 [P] Test `acknowledgement_survives_sign_out_and_revocation` in `crates/modplayer-ui/tests/first_launch.rs` (depends on Phase 5 sign-out + Phase 6 revocation both existing) *(FR-003, DM-27)*
- [X] T102 [P] `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean across all new/changed files *(Constitution VII)*
- [X] T103 [P] `cargo deny check` passes with the new licence allow-list entries *(Constitution VII)*
- [X] T104 [P] `scripts/check-license-headers.sh` passes; SPDX headers present on every new file *(Constitution VII)*
- [X] T105 Execute quickstart.md manual scenarios M1–M8 against a real build and the real OS secure store *(SC-001, SC-002, SC-003, SC-005, SC-006, SC-007, SC-008)* — driven 2026-09-15 via `env -u RUSTUP_TOOLCHAIN MODPLAYER_CONFIG_DIR=$(mktemp -d) cargo run -p modplayer` on macOS, synthetic input (Python Quartz `CGEventPost` mouse clicks, computed from `CGWindowListCopyWindowInfo` bounds — real Keychain, no test doubles) plus `screencapture` window captures. Results:
  - **M1 PASS** — fresh config dir: Welcome shown first with description, 3 disclosure statements, terms/privacy links, "I understand, continue" enabled immediately, Decline; Decline → explanation + Quit; Quit → process exits; config dir empty throughout; relaunch → Welcome again.
  - **M2 PASS** — privacy notice opened (readable, Back works) and acknowledged; `settings.toml` got `[disclosure] acknowledged_version = 1`; relaunch → sign-in step, no Welcome; editing the file to `acknowledged_version = 0` and relaunching → Welcome shown again.
  - **M5(a) NOT RELIABLY OBSERVABLE in this environment** — this machine's browser already holds an authenticated Spotify session, so pressing Sign in completes the full browser round trip (redirect → loopback callback → token exchange) in well under a second, leaving no observable "Waiting for your browser…/Cancel" window to click Cancel against. Deferred to a human tester with control over the browser session (e.g. a logged-out browser) who can reliably land the click inside the waiting window.
  - **M6 PARTIAL PASS** — pre-sign-in half verified: with the login Keychain locked (`security lock-keychain`) and no session, pressing Sign in triggered the OS "modplayer wants to use the login keychain" prompt; cancelling it produced the app's own screen "ModPlayer couldn't reach your Keychain. Unlock your Mac's Keychain, then retry." with a Retry button, no browser opened, and no file written to the config dir — matches spec. The second half ("with a signed-in session, lock the store and relaunch") could not be cleanly isolated: pressing Retry unexpectedly reached the OS keychain (this host's Keychain does not actually block CLI/API access while showing "locked" — confirmed independently by successfully writing/reading unrelated Keychain items right after `security lock-keychain`), and because the browser already had a live Spotify session (see M5(a)), Retry cascaded into a real, complete sign-in (credential in Keychain, `account.toml` written, reached Device Check) rather than exercising the intended locked-relaunch warning path. Not re-attempted further to avoid repeatedly creating real sessions; this half is deferred to a human tester with a genuinely lockable store.
  - **M8 FAIL (defect found, reproduced twice independently)** — from a real signed-in session, Settings › Account › Sign out → confirmation modal renders with **no category bullets** ("This deletes the following from this device:" followed directly by Cancel/Sign out, nothing listed — contradicts FR-015/T085's `signout_categories()` bullets); confirming shows an Info toast **"Signed out. Deleted:"** with an empty category list; the central panel does not return to the sign-in step (it keeps showing the pre-existing "Couldn't verify your subscription" tier screen); and, checked immediately and again after a few seconds, **neither `account.toml` nor the OS Keychain `ModPlayer`/`session-credential` entry were actually deleted** (`security find-generic-password` still returned the original entry; `account.toml` on disk was byte-identical to before sign-out). This contradicts SC-003 and the M8 acceptance criteria. Filed here as a confirmed P1 defect for follow-up; not silently marked as passing. The stray real Keychain entry and temp config dirs created while investigating this were manually removed afterward (`security delete-generic-password`, `rm -rf` on the mktemp dirs) so no state was left on the shared machine.
  - **M3, M4, M7 DEFERRED to human sign-off**, per operator instruction — no isolated headless Spotify Premium/Free test credentials were provided for this session. Note for the human tester: this host's browser already carries a live, auto-approving Spotify session, which made the OAuth round trip complete near-instantly and without any visible login form whenever Sign in was pressed (see M5(a)/M6 notes); the tier resolved unverified/unknown in the runs observed here (real network tier-check calls did not reliably resolve during this session), so M3/M4's Premium-vs-Free branching and M7's revocation/expiry paths still need a deliberate, credentialed pass — and the M8 sign-out defect above should be fixed and re-verified before those runs, since M7's revocation path shares the same `clear()`-every-store code path.
- [X] T106 [P] Final accessibility sweep across Welcome, Privacy Notice, Sign-in, Settings › Account, Settings › About, and the sign-out modal (every control keyboard-operable with an accessible name) *(FR-023, NFR-6.1, NFR-6.2; Constitution X)*

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. BLOCKS all user stories — `AccountService`, `launch_flow`, and the App launch gate must exist before any story's UI can render.
- **User Story 1 (Phase 3)**: Depends on Foundational only.
- **User Story 2 (Phase 4)**: Depends on Foundational only (not on US1 code, though in practice it is built after US1 since both are P1).
- **User Story 3 (Phase 5)**: Depends on Foundational (`AccountScopedStore`, `AccountStateStore`) and on US2's `AccountService`/credential plumbing (T057-T064) being in place — sign-out has nothing to clear otherwise.
- **User Story 4 (Phase 6)**: Depends on Foundational and on US2's sign-in/refresh machinery (T057-T064) — it hardens paths US2 created. Independently testable once US2 exists, without requiring US3.
- **Polish (Phase 7)**: Depends on all four stories (T101 specifically needs US3 + US4).

### Within Each User Story

- Tests before implementation (T034-T036 before T037+; T047-T056 + T107-T108 before T057+; T078-T079 before T080+; T088-T092 before T093+).
- Traits/data before services; services before UI; UI before notification mapping.
- Each story's checkpoint is a fully working increment.

### Parallel Opportunities

- Setup: T002, T003, T007, T008 in parallel; T001/T004/T005/T006 are sequential (same/adjacent Cargo.toml edits).
- Foundational: T009, T014, T015, T018, T020, T021, T022, T023, T024, T025, T028, T030 can start in parallel once their prerequisites (if any) land; T010/T011 in parallel after T009; T016/T017 in parallel after T015.
- Within each story, all `[P]`-marked test tasks run in parallel (different files); UI view files (e.g., T039/T040) run in parallel.
- **US3 and US4 can be staffed in parallel** once US2's checkpoint is reached — they touch different primary files (`registry.rs`/`settings/account.rs` vs. `service.rs`/`sign_in.rs`) aside from the shared `app.rs` event-mapping and `account.ftl` additions, which should be sequenced or merged carefully.

---

## Parallel Example: User Story 1

```bash
# Tests together:
Task: "Test disclosure_text_is_pinned_to_bundle_version in crates/modplayer-account/tests/disclosure.rs"
Task: "Test launch_flow_orders_welcome_signin_devicecheck_main in crates/modplayer-account/src/launch_flow.rs"
Task: "Test decline_writes_nothing in crates/modplayer-ui/tests/first_launch.rs"

# Views together (after T037/T038 land the disclosure text + hash):
Task: "Implement Welcome + Decline views in crates/modplayer-ui/src/welcome.rs"
Task: "Implement Privacy Notice view in crates/modplayer-ui/src/privacy_notice.rs"
```

## Parallel Example: User Story 2

```bash
# US2 tests together (T047-T056, T107, T108 all touch different files or inline modules):
Task: "Test sign_in_probes_store_before_opening_browser in crates/modplayer-account/tests/sign_in.rs"
Task: "Test callback_with_code_stores_credential_then_checks_tier in crates/modplayer-account/tests/sign_in.rs"
Task: "Test starting_a_new_attempt_discards_the_previous in crates/modplayer-account/tests/sign_in.rs"
Task: "Test tier_free_and_unknown_gate_playback in crates/modplayer-account/tests/tier.rs"
Task: "Test tier_check_times_out_after_30s_keeping_credential in crates/modplayer-account/tests/tier.rs"
Task: "Test refresh_backoff_and_signin_again_after_3_failures in crates/modplayer-account/tests/refresh.rs"
Task: "Test credential_never_appears_outside_secure_store in crates/modplayer-ui/tests/credential_leak.rs"
Task: "Proptest account_toml_round_trips_any_session in crates/modplayer-account/src/state_store.rs"
Task: "Proptest credential_payload_round_trips_any_token_set in crates/modplayer-account/src/credential.rs"
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 — both P1)

1. Phase 1: Setup
2. Phase 2: Foundational (CRITICAL — blocks everything)
3. Phase 3: User Story 1 — **STOP and VALIDATE** (M1/M2 quickstart scenarios)
4. Phase 4: User Story 2 — **STOP and VALIDATE** (M3/M4 quickstart scenarios)
5. This is the MVP: a user can disclose-acknowledge, sign in, get tier-verified, and reach the main window or a browse-only screen.

### Incremental Delivery

1. Setup + Foundational → foundation ready, app builds.
2. + US1 → welcome/disclosure/decline/privacy notice, independently demoable.
3. + US2 → sign-in, tier check, refresh; MVP complete.
4. + US3 → sign-out, independently demoable.
5. + US4 → interruption/revocation/expiry hardening, independently demoable.
6. Polish → CI gates, manual M1-M8 pass, accessibility sweep.

### Suggested Team Split (after Foundational)

- Developer A: US1 (small, self-contained, unblocks legal sign-off early).
- Developer B: US2 (largest slice — PKCE, listener, Spotify client, refresh scheduler, four screens).
- After US2's checkpoint: Developer A → US3, Developer B → US4, in parallel (see Parallel Opportunities above for the shared-file caveat on `app.rs`/`account.ftl`).

---

## Notes

- `[P]` tasks touch different files and have no incomplete-task dependency.
- `[Story]` labels map every implementation/test task to US1-US4 for traceability; Setup/Foundational/Polish carry no story label by design.
- Every FR/SC named in quickstart.md's test table has exactly one task creating it above.
- Every task line ends with the requirement IDs it implements (constitution Governance rule); T107/T108 are numbered after T106 because they were added in remediation, and sit in US2 where the serialized state first exists.
- Verify each new test fails (or does not compile) before writing the implementation that makes it pass.
- Commit after each task or logical group (per repository convention, not auto-committed by this command).
- Stop at any checkpoint to run the corresponding quickstart.md manual scenario(s) before continuing.
- Avoid: vague tasks, two tasks editing the same file marked `[P]`, cross-story dependencies that break independent testability (US3/US4 depending on US2's plumbing is intentional and noted above, not accidental coupling).
