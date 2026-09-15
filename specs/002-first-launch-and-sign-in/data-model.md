# Data Model: First Launch Disclosure and Sign-In

**Feature**: 002-first-launch-and-sign-in | **Phase 1** | Source: [spec.md](spec.md) Key Entities, DM-1, DM-27

All types live in `crates/modplayer-account` unless stated otherwise. Types
are plain Rust structs/enums; serde derives are noted where a type is
persisted. Timestamps are `time::OffsetDateTime` (UTC, RFC 3339 on disk).

## 1. Entities

### 1.1 DisclosureAcknowledgement *(device-scoped — persisted in `settings.toml`, owned by `modplayer-core`)*

| Field | Type | Rules |
|---|---|---|
| `acknowledged_version` | `u32` | Equals `DISCLOSURE_BUNDLE_VERSION` at the time of acknowledgement. `0`/absent = never acknowledged. |
| `acknowledged_at` | `OffsetDateTime` | Set once per acknowledgement; informational. |

- Lives in `AudioSettings` (001's domain struct, to be renamed conceptually
  but not literally) as `disclosure: Option<DisclosureAcknowledgement>` and in
  `RawSettings` as `[disclosure]` with `acknowledged_version`, `acknowledged_at`
  (both `#[serde(default)]`). Schema version stays `1` (additive; older builds
  ignore unknown keys per 001 read rules).
- **Invariant**: sign-out, revocation and expiry never modify it (FR-003).
- **Rule**: welcome screen is shown iff `acknowledged_version != DISCLOSURE_BUNDLE_VERSION` (FR-004).

### 1.2 DisclosureBundle *(compile-time constant, `modplayer-account::disclosure`)*

| Item | Type | Rules |
|---|---|---|
| `DISCLOSURE_BUNDLE_VERSION` | `u32` | Starts at `1`. Bumped only for substantive changes to the disclosure or privacy notice (FR-004). |
| `DISCLOSURE_EN_US_SHA256` | `&str` | SHA-256 hex of `locales/en-US/disclosure.ftl` source. Pinned by test (research R9). |
| `TERMS_URL` | `&str` | Streaming service's terms page (opened in system browser). |
| `UPGRADE_URL` | `&str` | Streaming service's Premium upgrade page. |

### 1.3 AccountSession *(account-scoped — non-secret part persisted in `account.toml`)*

| Field | Type | Rules |
|---|---|---|
| `account_id` | `String` | Service account identifier (`/v1/me` `id`). Non-empty. |
| `display_name` | `String` | `/v1/me` `display_name`, falling back to `account_id` when null. |
| `tier` | `Tier` | `Premium` / `Free` / `Unknown`. |
| `credential_ref` | `String` | Always the secure-store entry name `"session-credential"`. Never the credential value (FR-009). |
| `expires_at` | `OffsetDateTime` | Access-token expiry (`now + expires_in`). |
| `authorized_at` | `OffsetDateTime` | When the browser flow completed (refresh-token lifetime reference, research R4). |
| `last_validated_at` | `Option<OffsetDateTime>` | Last successful online validation (`/v1/me` or refresh). |
| `state` | `SessionState` | In-memory only (derived on load, see §2.1). |

- **Derived**: `playback_permitted() -> bool` = `tier == Tier::Premium` (FR-011).
- **Mirror**: `disclosure_acknowledged_version()` is read from 001's settings,
  not stored here (DM-1 attribute, DM-27 is the record of truth).
- **Invariant**: at most one `AccountSession` exists (FR-012); `account.toml`
  is either absent or holds exactly one `[account]` table.

### 1.4 SessionCredential *(secret — secure store entry `ModPlayer` / `session-credential`)*

| Field | Type | Rules |
|---|---|---|
| `access_token` | `String` | Bearer token. |
| `refresh_token` | `String` | Replaced when a refresh response carries a new one. |
| `expires_at` | `OffsetDateTime` | Duplicated from AccountSession so the secret is self-describing. |
| `scope` | `String` | Space-separated granted scopes. |
| `authorized_at` | `OffsetDateTime` | As above. |

- Serialised as compact JSON via `serde_json`; **size test**: < 2048 bytes
  with maximal realistic token lengths (Windows 2560-byte blob cap, R5).
- Implements a redacting `Debug` (`SessionCredential { .. }`) so it can never
  reach a log via `{:?}` (FR-020). No `Display`. No `Serialize` into any
  format other than the secure-store payload path (enforced by keeping the
  serde derive on a private wire struct `CredentialPayload`).

### 1.5 PendingAuthorization *(transient — secure store entry `ModPlayer` / `pending-authorization`)*

| Field | Type | Rules |
|---|---|---|
| `attempt_id` | `u64` | Monotonic per process, used to discard stale worker results. In-memory only. |
| `pkce_verifier` | `String` | 43–128 chars, URL-safe base64 of 32 random bytes. Secret. |
| `state` | `String` | URL-safe base64 of 16 random bytes; must match the callback. |
| `port` | `u16` | Loopback port bound for this attempt. |
| `started_at` | `OffsetDateTime` | Browser was opened at this time; attempt expires at `started_at + 5 min`. |

- **Invariant**: at most one exists at any time (FR-016/FR-018); creating a
  new one deletes the previous entry first.
- **Lifecycle**: created when the listener binds → deleted on completion,
  cancel, timeout, definitive error, or replacement.
- Redacting `Debug` like §1.4.

### 1.6 AccountScopedStore registry

```rust
pub trait AccountScopedStore {
    /// Fluent key for the confirmation list and the "Deleted:" report.
    fn category_key(&self) -> &'static str;
    /// Remove everything this store holds for the current account. Idempotent.
    fn clear(&mut self) -> Result<(), ClearError>;
}
```

Registered in this slice (in confirmation-list order):

| Implementor | `category_key` | Backing |
|---|---|---|
| `CredentialStore` | `signout-category-credential` ("your sign-in credential") | Secure-store entry `session-credential` (+ `pending-authorization`) |
| `AccountStateStore` | `signout-category-account-details` ("account details (identifier, display name, subscription tier, expiry)") | `account.toml` |

Later slices push further implementors (offline cache, per-track state) onto
`AccountService::registry()`; the confirmation and report lists are generated
from the registry, never hard-coded (FR-015).

### 1.7 Tier

```rust
pub enum Tier { Premium, Free, Unknown }
```
Serialised as `"premium" | "free" | "unknown"`. `/v1/me` `product` mapping:
`premium → Premium`; `free | open → Free`; anything else / missing → `Unknown`.

## 2. State machines

### 2.1 SessionState (AccountSession lifecycle, DM-1)

```
                 ┌──────────────────────────────────────────────────────┐
                 │                                                      │
 SignedOut ──start_sign_in──▶ Authorizing ──code exchanged──▶ Checking ─┼─tier ok─▶ Active
     ▲            (probe ok)      │ cancel / timeout / error              │            │
     │                            ▼                                      │        expiry reached
     │                        SignedOut(note)                            │            ▼
     │                                                                   │         Expired
     │                                                                   │            │
     └────────── sign_out / revoked (delete everything) ─────────────────┴────────────┘
```

| State | Persisted marker | Notes |
|---|---|---|
| `SignedOut { note: Option<SignInNote> }` | no `account.toml`, no credential | `note` ∈ {Cancelled, TimedOut, ServiceError, PreviousDidNotFinish, Revoked, StoreUnreadable} drives the one-line text on the sign-in step. |
| `Authorizing { attempt_id, resumed: bool }` | `pending-authorization` entry | Listener thread running; `resumed` = re-bound after relaunch (R2). |
| `Checking { attempt_id }` | credential + `account.toml` (tier `Unknown`) | Tier check in flight, 30 s budget. |
| `Active` | credential + `account.toml` | Refresh scheduler armed. `tier` may be `Premium`, `Free`, or `Unknown` (after check failure). |
| `Expired` | credential (retained) + `account.toml` | `expires_at <= now` and no refresh succeeded; main window stays available; critical notification raised once. |
| `StoreUnreadable` | `account.toml` present, credential unreadable | Launch found a session but the secure store failed to read (FR-017); nothing cleared; sign-in step + warning + Retry. |

Transitions and their side effects are specified in
[contracts/account-session.md](contracts/account-session.md).

### 2.2 TierCheckOutcome

```
Checking ──/v1/me 200 product=premium──▶ Active(Premium) → Device Check (if unconfirmed) → main
Checking ──/v1/me 200 product=free|open─▶ Active(Free)   → non-Premium screen → main
Checking ──/v1/me 200 no product / 403──▶ Active(Unknown) → "couldn't verify" screen (Retry / Continue)
Checking ──timeout 30 s / transport / 429 exhausted──▶ Active(Unknown) → "couldn't verify" screen
Checking ──401──▶ one refresh attempt ──success──▶ retry /v1/me once
                                     └─invalid_grant──▶ Revoked path (§2.1)
```

### 2.3 RefreshScheduler (per Active session)

| Variable | Rule |
|---|---|
| `due_at` | `expires_at - 5 min`, or `expires_at - lifetime/2` when `lifetime < 10 min` (FR-013). |
| `backoff` | On transient failure: 1 s, 2 s, 4 s … capped 60 s (FR-014); reset on success. |
| `consecutive_failures` | Incremented on transient failure; at `3` raise warning `signin-again` once; reset to 0 and clear the notification on success. |
| Definitive rejection (`invalid_grant`) | → Revoked path; scheduler stops. |
| `now >= expires_at` | → `Expired` state; scheduler keeps retrying (a later success returns to `Active`). |
| Sign-out / revocation | Scheduler dropped; in-flight result discarded by `attempt_id` mismatch. |

### 2.4 LaunchFlow (pure function, `modplayer-account::launch_flow`)

```rust
pub enum LaunchStep { Welcome, SignIn, DeviceCheck, Main }

pub fn next_step(
    acknowledged_version: u32,
    session: &SessionState,
    tier: Tier,
    device_check_needed: bool,   // 001's should_show_device_check()
) -> LaunchStep
```

| Condition (first match wins) | Step |
|---|---|
| `acknowledged_version != DISCLOSURE_BUNDLE_VERSION` | `Welcome` |
| `session` ∈ {SignedOut, Authorizing, Checking, StoreUnreadable} | `SignIn` (the sign-in screen renders the sub-state) |
| `session` ∈ {Active, Expired} ∧ `tier == Premium` ∧ `device_check_needed` | `DeviceCheck` |
| otherwise | `Main` |

Evaluated every frame by `App` (cheap), so acknowledging, signing in, and
confirming the device each advance the flow without a restart (FR-001, FR-010,
FR-012).

## 3. Persistence summary

| Data | Location | Scope | Cleared by |
|---|---|---|---|
| DisclosureAcknowledgement | `settings.toml` `[disclosure]` | device | never (only a version bump re-prompts) |
| AccountSession (non-secret) | `account.toml` | account | sign-out, revocation |
| SessionCredential | OS secure store `ModPlayer/session-credential` | account | sign-out, revocation |
| PendingAuthorization | OS secure store `ModPlayer/pending-authorization` | transient | completion, cancel, timeout, error, replacement, stale-on-launch |
| Store probe | OS secure store `ModPlayer/probe-<pid>` | transient | immediately after the round-trip |

## 4. Validation rules

- `DISCLOSURE_BUNDLE_VERSION >= 1`; acknowledgement with version `0` is never written.
- `SessionCredential` JSON ≤ 2048 bytes (test), `access_token`/`refresh_token` non-empty.
- `PendingAuthorization.port` ∈ 1024..=65535; `state` compared with constant-time equality is unnecessary (public nonce) but must be exact.
- `/v1/me` response: `id` required non-empty; missing → treat as transport-level failure (tier `Unknown`, no session fields overwritten).
- `expires_in` missing → default 3600 s; `expires_in <= 0` → treated as already expired (immediate refresh).
- `account.toml` unreadable/unparseable at launch → treated as no session **only if** the credential is also absent; if a credential exists, rebuild `account.toml` with `tier = Unknown` and run launch validation.
