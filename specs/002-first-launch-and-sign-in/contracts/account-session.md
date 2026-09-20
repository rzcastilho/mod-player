# Contract: AccountService — commands, events, state, persistence (`modplayer-account`)

`AccountService` is the single authority for `AccountSession` (mirrors 001's
"controller is the single authority" rule). It owns the secure store, the
authorization service, the account-state file store, the account-scoped
registry, the refresh scheduler, and the worker threads. It runs on the UI
thread and is ticked every frame; all blocking work happens on worker
threads that report back through one `std::sync::mpsc` channel.

## Construction

```rust
pub struct AccountService {
    // generic over nothing: trait objects, because the UI must not be generic over 3 params
    secure: Arc<dyn SecureStore>,
    auth: Arc<dyn AuthorizationService>,
    clock: Arc<dyn Clock>,
    state_store: AccountStateStore,        // account.toml
    registry: Vec<Box<dyn AccountScopedStore>>,
    // … state, scheduler, channels
}
impl AccountService {
    pub fn new(secure, auth, clock, config_dir: PathBuf) -> Self;   // does NOT touch the network
    pub fn launch(&mut self) -> LaunchOutcome;   // reads stores; may spawn launch validation
}
```

`launch()` (called once by the binary after construction, mirroring
`PlaybackController::launch`):

| Found | Result |
|---|---|
| no `account.toml`, no credential, no pending | `SignedOut { note: None }` |
| pending attempt younger than 5 min, port re-bindable | `Authorizing { resumed: true }` (listener resumed, remaining deadline) |
| pending attempt otherwise | pending deleted; `SignedOut { note: PreviousDidNotFinish }` |
| `account.toml` + credential readable | `Active`/`Expired` by `expires_at`; spawn **launch validation** (refresh if due, then `fetch_profile`) |
| `account.toml` + secure store read error | `StoreUnreadable`; nothing cleared |
| credential readable, `account.toml` missing/corrupt | rebuild `account.toml` with tier `Unknown`, then as Active row |

## Commands (UI → service)

| Method | Precondition | Effect |
|---|---|---|
| `start_sign_in()` | state ∈ {SignedOut, StoreUnreadable, Expired, Authorizing} | Discards any pending attempt; `secure.probe()`; on `Err` → event `StoreUnavailable` and state unchanged; on `Ok` → new `PendingAuthorization`, listener spawned, state `Authorizing`, event `BrowserUrlReady(url)`. |
| `browser_url()` | state Authorizing | Returns the current attempt's URL ("Open the browser again"). |
| `cancel_sign_in()` | state Authorizing | Sets cancel flag; listener exits; pending deleted; `SignedOut { Cancelled }`. |
| `recheck_tier()` | state ∈ {Active, Expired} and no check in flight | Spawns `fetch_profile`; state stays; event `TierChecked` / `TierCheckFailed`. |
| `sign_out()` | state ∈ {Active, Expired, StoreUnreadable} | Bumps `attempt_id` (abandons in-flight work); for each registry store: `clear()`; deletes `pending-authorization`; state `SignedOut { None }`; event `SignedOut { categories }`. Errors from individual stores are collected: the event still fires, and `SignOutIncomplete { category }` is emitted for each failure (nothing else is retried automatically). |
| `signout_categories()` | any | `Vec<&'static str>` of `category_key`s in registry order (confirmation list). |
| `register_store(Box<dyn AccountScopedStore>)` | before `launch()` | Later slices add their stores. |
| `tick()` | every frame | Drains worker events, advances the refresh scheduler, detects expiry, returns `Vec<AccountEvent>` for the UI to map to notifications/screens. |

## Events (service → UI, returned by `tick()`)

| Event | Emitted when | UI reaction (see ui-surface.md) |
|---|---|---|
| `BrowserUrlReady(String)` | start_sign_in succeeded | `ctx.open_url`; show Waiting screen |
| `StoreUnavailable { store_name_key }` | probe failed / launch read failed | Sign-in step, `signin-store-unavailable` message + Retry |
| `SignInFailed(SignInNote)` | cancel / timeout / error / exchange failure | Sign-in step with one-line note + Retry |
| `Authorized` | credential stored | Show "Checking your account" |
| `TierChecked(Tier)` | profile fetched | Premium → continue flow; Free → non-Premium screen; Unknown → "couldn't verify" screen |
| `TierCheckFailed` | 30 s / transport / 403 | same as `TierChecked(Unknown)` |
| `RefreshFailing` | 3rd consecutive transient failure | raise Warning `signin-again` (once) |
| `RefreshRecovered` | refresh succeeded after failures | dismiss the `signin-again` notification |
| `SessionExpired` | `now >= expires_at` without refresh | raise Critical `session-expired` (once per expiry) |
| `SessionRevoked` | definitive rejection anywhere | everything cleared; sign-in step; raise Critical `session-revoked` |
| `SignedOut { categories }` | sign_out completed | sign-in step; raise Info `signed-out` with joined category labels |
| `SignOutIncomplete { category }` | a registry store failed to clear | raise Warning `signout-incomplete` |

`tick()` never blocks. Worker results carry the `attempt_id` they were started
with; results with a stale id are dropped silently (sign-out during a refresh
or tier check, FR-015 "abandon in-flight").

## Refresh scheduler rules (FR-013, FR-014, FR-021)

- Armed whenever state is `Active`/`Expired` with a readable credential.
- `due_at = expires_at − 5 min`, or `expires_at − lifetime/2` when `lifetime < 10 min`.
- At most one refresh in flight. Transient failure → next attempt after `min(60 s, 1 s × 2^n)`; `consecutive_failures += 1`; at exactly 3 emit `RefreshFailing`.
- Success → write credential (new refresh token if present) to the secure store **before** updating `account.toml` (`expires_at`, `last_validated_at`); reset counters; emit `RefreshRecovered` if a `RefreshFailing` was emitted; if state was `Expired`, return to `Active`.
- Secure-store write failure on success is treated as a transient failure (edge case "store locked mid-session"): the in-memory credential is updated, the persisted one is not, and the backoff loop continues.
- `Rejected` → `SessionRevoked` path.
- `now >= expires_at` while no refresh has succeeded → state `Expired`, emit `SessionExpired` once; the scheduler keeps retrying with backoff.

## Revocation path (FR-019)

Trigger: `AuthError::Rejected` from refresh, launch validation, or a tier
check (after the single refresh retry). Action, in order: delete credential
entry → delete `pending-authorization` → `clear()` every registry store →
state `SignedOut { note: Revoked }` → emit `SessionRevoked`. Stopping playback
is 003's concern (it observes `playback_permitted()` / the event).

## account.toml (account-scoped state file)

Location: same directory as `settings.toml` (001 contract: platform config dir
or `MODPLAYER_CONFIG_DIR`). Written by tmp + `sync_all` + `rename`.

```toml
schema_version = 1

[account]
id = "spotify-user-id"
display_name = "Display Name"
tier = "premium"            # "premium" | "free" | "unknown"
credential_ref = "session-credential"
expires_at = "2026-09-15T14:03:00Z"
authorized_at = "2026-09-15T13:03:00Z"
last_validated_at = "2026-09-15T13:03:02Z"   # optional
```

Read rules: missing file → no session; unparseable → see data-model §4;
unknown keys ignored; unknown `tier` string → `unknown`. Write rule: full
struct every time; on error the previous file is kept and the UI raises
Warning `settings-save-failed` (reusing 001's key).

## settings.toml additions (device-scoped, owned by `modplayer-core`)

```toml
[disclosure]
acknowledged_version = 1
acknowledged_at = "2026-09-15T13:00:00Z"
```

`schema_version` stays `1`. Absent section → `acknowledged_version = 0`.
Write failure on acknowledgement → Warning `settings-save-failed`, flow
proceeds for this run (FR-003).

## Threading model

| Thread | Spawned by | Lifetime | Reports |
|---|---|---|---|
| `account-listener` | `start_sign_in` / resumed launch | until callback, cancel, timeout | `ListenerResult` |
| `account-exchange` | listener `code` result | one request | `Authorized` / `SignInFailed` |
| `account-tier` | `Authorized`, `recheck_tier`, launch validation | one request | `TierChecked` / `TierCheckFailed` / `Rejected` |
| `account-refresh` | scheduler | one request | refresh outcome |

Threads are detached (`JoinHandle` dropped) but every result carries an
`attempt_id`; the service never blocks on a thread. The listener thread
additionally observes an `Arc<AtomicBool>` cancel flag and exits within 50 ms
of cancel (also set on `Drop` of `AccountService`).
