# Implementation Plan: First Launch Disclosure and Sign-In

**Branch**: `002-first-launch-and-sign-in` | **Date**: 2026-09-15 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-first-launch-and-sign-in/spec.md`

## Summary

Insert two gates ahead of 001's Device Check: a versioned first-launch
**Welcome/disclosure** screen (device-scoped acknowledgement in
`settings.toml`, re-shown when `DISCLOSURE_BUNDLE_VERSION` changes, en-US
text hash-pinned in CI) and a **sign-in step** that runs Spotify's
authorization-code + PKCE flow in the system browser with a loopback
callback, stores the resulting token set only in the OS credential store,
verifies the subscription tier, refreshes the credential before expiry, and
supports confirmed sign-out driven by a registry of account-scoped stores.
Every interruption (cancel, timeout, app closed mid-flow, locked store,
remote revocation, expiry) resolves to a defined state with no partial data.

Technical approach (details in [research.md](research.md)): two new library
crates — `modplayer-secure-store` (adapter over `keyring 4.2`; `SecureStore`
trait with `KeyringSecureStore` + `MemorySecureStore`) and
`modplayer-account` (`AccountService` state machine, PKCE, std-only loopback
listener, `AuthorizationService` trait with `SpotifyAuthorizationService` on
`ureq 3.4` + `FakeAuthorizationService`, refresh scheduler with injectable
`Clock`, `account.toml` state store, account-scoped registry, disclosure
constants). No async runtime; worker threads report over `std::sync::mpsc`
and are drained in `tick()` exactly like 001's device watcher. The UI crate
adds Welcome, Privacy Notice, Sign-in, Settings › Account and Settings › About
screens plus notification actions, and computes the launch order with a pure
`launch_flow::next_step`.

**A-2 spike outcome (spec gate)**: PASS — librespot 0.8 accepts an OAuth
access token via `Credentials::with_access_token` and returns a reusable
receiver credential (research R1). Sign-in design FR-007/FR-009 stands.

Assumptions taken where the spec deferred: loopback listener with dynamic
port rather than a custom URI scheme (R2); "attempt to complete" a pending
authorization on relaunch = re-bind the recorded port and resume waiting
(R2); default `client_id` = librespot's Keymaster id with an env override
(R4, governance note in Complexity Tracking); tier from `/v1/me` `product`
with `unknown` when the field is absent (R3).

## Technical Context

**Language/Version**: Rust 1.95 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), fluent-templates 0.15, serde + toml, directories 6, thiserror/anyhow; **new**: keyring 4.2 (OS credential stores), ureq 3.4 (HTTPS, rustls), serde_json 1, sha2 0.10, base64 0.22, getrandom 0.3, time 0.3, and dev-only proptest 1 (Constitution VIII state-serialization proptests, `modplayer-account` `[dev-dependencies]`) — each justified in research.md § Dependency additions; `deny.toml` gains `CDLA-Permissive-2.0` (webpki-roots) and `Apache-2.0 AND ISC` (ring)

**Storage**: `settings.toml` (001, gains `[disclosure]`), new `account.toml` (non-secret account state, atomic replace, deleted on sign-out), OS secure store entries `ModPlayer/session-credential` and `ModPlayer/pending-authorization` — see [contracts/account-session.md](contracts/account-session.md), [contracts/secure-store.md](contracts/secure-store.md)

**Testing**: `cargo test --workspace` with `MemorySecureStore`, `FakeAuthorizationService`, `FakeClock`, and a real loopback `TcpListener` driven by `TcpStream` in tests; credential-leak test scans files/logs/notifications; hash-pin test for disclosure text; live-store test `#[ignore = "manual"]`; manual scenarios M1–M8 in [quickstart.md](quickstart.md); same CI gates as 001 (fmt, clippy -D warnings, test, deny, licence headers) on ubuntu/macos/windows

**Target Platform**: Desktop macOS (Keychain), Windows 10+ (Credential Manager), Linux (Secret Service via zbus — GNOME Keyring/KWallet); identical behaviour, platform differences confined to `modplayer-secure-store`

**Project Type**: Desktop application — Cargo workspace grows from 7 to 9 crates (2 new library crates)

**Performance Goals**: first launch → signed-in, tier-verified in < 2 min excluding browser typing (SC-002); sign-out completes < 10 s (SC-003, local deletes only); tier check ≤ 30 s budget; browser wait ≤ 5 min; UI thread never blocks on network (all HTTP on worker threads); `tick()` O(events) per frame

**Constraints**: credential never in a plain file, log, notification, or `Debug` output (FR-020, Constitution VI); no password field (FR-007); no fallback storage when the store is unavailable (FR-017); `#![forbid(unsafe_code)]` in both new crates (FFI stays inside keyring's platform crates); no `unwrap`/`expect` outside tests; all strings externalised; every control keyboard-operable with an accessible name; product branding never uses the service's marks (FR-006)

**Scale/Scope**: 2 new crates, ~5 new UI screens/views (Welcome+Decline, Privacy Notice, Sign-in with 6 sub-states, Settings › Account + sign-out modal, Settings › About), 2 new `.ftl` files (~70 keys), 1 state machine with 6 states, 4 worker-thread kinds, ~25 automated tests, single user / single account

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | N/A | No engine code changes. The account service runs on the UI thread and worker threads only; nothing it does touches `modplayer-engine` or the audio callback. Device Check ordering changes are in `App`, off the real-time path. |
| II. Plugins Are Guests | No | N/A | No plugin runtime exists yet. Credential API surface is confined to `modplayer-account`, which the future Capability Gateway will simply not expose (Constitution VI "never reachable by plugins"). |
| III. Host Primitives, Plugin Behaviors | No | N/A | No DSP or transport primitives are added. |
| IV. Audio Source Is Replaceable and Isolated | Yes (adjacent) | ✅ PASS | This slice adds no streaming (INT-2) code; the authorization integration (INT-1) is confined to `modplayer-account::spotify` behind the `AuthorizationService` trait. Every other crate builds and tests with `FakeAuthorizationService`; 003's receiver crate will consume the credential through `AccountService`, keeping the protocol in one crate. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No audio, cache, or sample APIs are introduced. |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | Credential lives only in the OS store (`SecureStore` is the sole writer, FR-009); redacting `Debug`, no `Display`, private wire struct (FR-020); leak test scans files/logs/notifications (SC-004); store-unavailable → refusal, never fallback (FR-017); TLS via rustls for all service calls; no telemetry; PKCE + `state` check on the callback; loopback bound to `127.0.0.1` only. No deviation. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | Two new crates, one per component (secure-store adapter; account service); both `#![forbid(unsafe_code)]` — the constitution's "keychain" exception is needed only transitively inside `keyring`'s platform store crates, recorded here rather than used; `thiserror` errors, no `unwrap`/`expect` outside tests; doc examples on public items; new licences added to `deny.toml` with crate comments; SPDX headers; CI matrix unchanged. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Test-first list in quickstart.md maps each FR/SC to a named test; interruption matrix (SC-005) covered by six tests against doubles; timing rules use `FakeClock` (no sleeps); leak test (SC-004); snapshot guard (SC-007); store-refusal test (SC-008). Property tests (proptest) apply to "state serialization" — `account.toml` and the credential payload get a round-trip proptest (`account_toml_round_trips_any_session`, `credential_payload_round_trips_any_token_set`; tasks T107/T108; see Complexity Tracking for the scope decision). |
| IX. One Plugin API Definition | No | N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | Three new traits, each with two real implementors used across crate boundaries: `SecureStore` (keyring + memory), `AuthorizationService` (Spotify + fake), `AccountScopedStore` (credential + account file, extended by later slices); `Clock` (system + fake) likewise. No feature flags. Platform differences confined to `modplayer-secure-store`. Every new control keyboard-operable with an accessible name; all strings externalised (en-US only, as 001). User override N/A (no plugins). Each new dependency justified against std/existing deps in research.md. |
| Governance: engine/gateway/runtime sign-off | No | N/A | None of those crates change. `CODEOWNERS` gains `crates/modplayer-secure-store/` for the security-sensitive adapter (recommended, not constitution-mandated). |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design artifacts add no unsafe code, no feature flags, no single-implementor traits, and no crate beyond the two listed; one governance-level assumption (default client id) is recorded below.

## Project Structure

### Documentation (this feature)

```text
specs/002-first-launch-and-sign-in/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R13, A-2 spike result, dependency justification
├── data-model.md        # Phase 1: entities, state machines, persistence, validation
├── quickstart.md        # Phase 1: automated gates (named tests) + manual scenarios M1–M8
├── contracts/
│   ├── secure-store.md          # SecureStore trait, entries, errors, doubles
│   ├── authorization-service.md # AuthorizationService trait, PKCE flow, loopback HTTP contract, budgets
│   ├── account-session.md       # AccountService commands/events, scheduler, revocation, account.toml, settings.toml delta, threads
│   └── ui-surface.md            # Screens, widgets, Fluent keys, notifications with actions, trademark rule
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (from 001) is kept; `+` marks new files, `~` modified files.

```text
Cargo.toml                                   ~ add workspace deps: keyring, ureq, serde_json, sha2, base64, getrandom, time
deny.toml                                    ~ allow CDLA-Permissive-2.0 (webpki-roots), Apache-2.0 AND ISC (ring)
CODEOWNERS                                   ~ crates/modplayer-secure-store/
locales/en-US/
├── disclosure.ftl                           + welcome/disclosure/privacy-notice text (hash-pinned)
├── account.ftl                              + sign-in, tier, account settings, about, notification keys
└── settings.ftl                             ~ Account/About descriptor titles
crates/
├── modplayer-secure-store/                  + adapter crate (only importer of keyring)
│   ├── Cargo.toml
│   ├── src/{lib.rs, store.rs (trait, EntryName, error), keyring_store.rs, memory_store.rs}
│   └── tests/live.rs                        #[ignore = "manual"] round-trip on the real store
├── modplayer-account/                       + account/session service crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── disclosure.rs                    DISCLOSURE_BUNDLE_VERSION, SHA-256 pin, TERMS_URL, UPGRADE_URL
│   │   ├── launch_flow.rs                   next_step() pure function
│   │   ├── session.rs                       AccountSession, Tier, SessionState, SignInNote
│   │   ├── credential.rs                    SessionCredential (+ private CredentialPayload), size test
│   │   ├── pending.rs                       PendingAuthorization
│   │   ├── pkce.rs                          verifier/challenge/state generation
│   │   ├── listener.rs                      loopback TcpListener thread, request-line parser, HTML page
│   │   ├── auth_service.rs                  AuthorizationService trait, TokenSet, Profile, AuthError
│   │   ├── spotify.rs                       ClientConfig + SpotifyAuthorizationService (ureq)
│   │   ├── fake_auth.rs                     FakeAuthorizationService (pub, used by ui tests)
│   │   ├── clock.rs                         Clock trait, SystemClock, FakeClock
│   │   ├── refresh.rs                       RefreshScheduler (due_at, backoff, failure count)
│   │   ├── state_store.rs                   account.toml load/save/delete (atomic replace)
│   │   ├── registry.rs                      AccountScopedStore trait, CredentialStore, AccountStateStore
│   │   ├── service.rs                       AccountService: commands, tick(), events, worker threads
│   │   └── notify.rs                        notification key constants
│   └── tests/{disclosure.rs, sign_in.rs, launch.rs, tier.rs, refresh.rs, sign_out.rs}
├── modplayer-core/
│   ├── src/settings/model.rs                ~ DisclosureAcknowledgement field + [disclosure] raw section
│   ├── src/settings_registry.rs             ~ Account descriptors (recheck, sign-out); About stays descriptor-free
│   ├── src/notifications.rs                 ~ NotificationAction enum, action field, dismiss_by_key()
│   └── tests/settings.rs                    ~ round-trip of the new section
├── modplayer-ui/
│   ├── Cargo.toml                           ~ depend on modplayer-account, modplayer-secure-store
│   ├── src/app.rs                           ~ owns AccountService; launch-gate rendering (Welcome/SignIn/DeviceCheck/Main); maps AccountEvents to notifications
│   ├── src/welcome.rs                       + Welcome + Decline views
│   ├── src/privacy_notice.rs                + privacy notice / read-only disclosure view
│   ├── src/sign_in.rs                       + sign-in step with sub-states
│   ├── src/settings/{mod.rs ~, account.rs +, about.rs +}
│   ├── src/notifications.rs                 ~ render action button
│   ├── src/shell.rs                         ~ accessibility test button count
│   └── tests/{fluent_keys.rs ~, first_launch.rs +, credential_leak.rs +, trademark.rs +}
└── modplayer/src/main.rs                    ~ build KeyringSecureStore + SpotifyAuthorizationService + AccountService; launch(); pass to App
```

**Structure Decision**: Keep 001's single Cargo workspace under `crates/`
(one crate per component, Constitution VII) and add two crates:
`modplayer-secure-store` (platform adapter — the only importer of `keyring`,
per Constitution X "platform differences confined to adapter crates") and
`modplayer-account` (the INT-1 account/session service — the only crate that
knows the authorization endpoints, keeping Constitution IV's spirit for
INT-1). Dependency graph stays a strict DAG:
`modplayer → ui → {core, account} ; account → secure-store ; core → {engine, audio-io} → …`.
`modplayer-account` deliberately does **not** depend on `modplayer-core`, so
003's receiver crate can take the credential from `account` alone. Locale
files stay at the repository root `locales/en-US/` (auto-embedded by 001's
`static_loader!`).

## Design notes that tasks must respect

1. **Write order on success**: secure-store credential first, then
   `account.toml`, then delete `pending-authorization` (contracts/authorization-service.md §5). A crash between steps leaves a credential without state — handled by the launch rule "credential readable, account.toml missing → rebuild with tier Unknown".
2. **`attempt_id` on every worker result**; `tick()` drops stale ids. Sign-out and "new attempt" bump the id — this is the whole "abandon in-flight" mechanism (FR-015, FR-016).
3. **Probe before browser** (FR-017): `start_sign_in` calls `secure.probe()` synchronously before binding the listener; a failure emits `StoreUnavailable` and changes no state.
4. **Notifications are the UI's job**: `AccountService` returns events; `App` maps them to `NotificationCenter` calls with the keys in contracts/ui-surface.md, so `modplayer-account` stays free of `modplayer-core`.
5. **Clocks are injected**: no `SystemTime::now()`/`Instant::now()` calls inside `modplayer-account` outside `SystemClock`; timing tests use `FakeClock::advance`.
6. **Redaction**: `SessionCredential`, `PendingAuthorization`, `TokenSet` implement manual `Debug` printing only field names; the leak test also asserts `format!("{:?}", …)` contains no token bytes.
7. **Disclosure text lives in `disclosure.ftl` only**; `disclosure.rs` `include_str!`s it for the hash and the About read-only view uses the same keys — one source.
8. **Launch gate is evaluated every frame** by `App` via `launch_flow::next_step`; no screen "returns" to another — state changes in the service move the flow.
9. **Loopback page** is static HTML with `Content-Length`, no scripts, no external resources, text externalised through Fluent at bind time.
10. **`open_url` only from the UI** (`egui::Context::open_url`), never from the service, so headless tests never launch a browser.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The following assumptions and scope decisions are recorded for traceability:

| Decision / deferral | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Default `client_id` = librespot's Keymaster id, env override `MODPLAYER_OAUTH_CLIENT_ID` (research R4) | Spotify Development-Mode apps are capped at 5 allow-listed users and require the owner to hold Premium; an open-source client cannot ship its own dashboard app. 003's receiver session must use this id regardless (R1). | Shipping our own dashboard app — unusable beyond 5 users. Requiring every user to create a dashboard app — hostile onboarding and still no `product` field. **Governance**: same legal posture as librespot; flagged for the Q-4 legal review; a different outcome changes one `ClientConfig` constant. |
| Loopback listener instead of custom URI scheme (R2); relaunch "attempt to complete" = re-bind recorded port and resume waiting | Spotify permits loopback with a dynamic port; custom schemes need per-OS registration and single-instance IPC, and are documented only for iOS. | Fixed port (librespot's 5588) — fails when the port is busy. Custom scheme — larger surface, unverified dashboard support, still needs the PKCE verifier persisted. |
| Tier from `/v1/me` `product`, `unknown` when absent (R3) | `product` is deprecated and removed for Development-Mode apps in 2026; the spec's `unknown` tier absorbs this; 003's AP login is the authoritative Premium signal. | Blocking on a guaranteed tier signal — none exists before the receiver connects. |
| Two new crates rather than modules in `modplayer-core` (R11) | One crate per component (VII); keeps keyring/ureq/rustls out of every 001 crate's graph; avoids a `core ↔ account` cycle; lets 003 depend on `account` alone. | Modules in core — dependency bloat and INT-1 knowledge spread into the core services crate. |
| Third+fourth trait with a test double (`AuthorizationService`, `SecureStore`, plus `Clock`) | FR-020/SC-004/SC-005/SC-008 require the full flow to run in CI without network or an unlocked keychain; doubles are used across crate boundaries (`modplayer-ui` tests), so `cfg(test)`-only fakes cannot work — the same justification 001 used for `OutputBackend`. | Global mock via `keyring_core::set_default_store` — process-global state breaks parallel tests; HTTP mocking at the socket level — heavier and still leaves timing untestable. |
| Constitution VIII proptest scope: round-trip proptests for `account.toml` and the credential payload only; no proptest for the OAuth wire structs | "State serialization" is the constitution's named target; wire structs are parsed, never serialised, and are covered by fixture tests. | Proptests for every serde struct — low value for parse-only types. |
| 6-month refresh-token lifetime surfaces through the revocation path with "revoked" wording | The token endpoint returns the same `invalid_grant` for expiry and revocation; the spec defines one path for definitive rejection. | A distinct "sign-in expired" message — needs `authorized_at` age heuristics; deferred as a string-only refinement. |
| No Linux keyring daemon in CI for this slice | Gates rely on `MemorySecureStore`; the live round-trip is a manual, `#[ignore]`d test. | Installing gnome-keyring + dbus-run-session in CI now — extra CI surface for one test; can be added when 003 needs live-store coverage. |
