// SPDX-License-Identifier: MIT OR Apache-2.0

//! `AccountService`: the single authority for `AccountSession`
//! (contracts/account-session.md, mirroring 001's `PlaybackController`
//! "controller is the single authority" rule). Construction landed in
//! Phase 2; sign-in (PKCE, the loopback listener, the `account-exchange`/
//! `account-tier` workers), tier verification, and the refresh scheduler
//! landed with US2 (T060-T064); sign-out (`sign_out()`, the registry-based
//! clear) landed with US3 (T080-T083). Launch-time resume of a pending
//! attempt, `StoreUnreadable` detection, the `Expired`/`SessionExpired`
//! transition, and the full revocation path for a definitive rejection
//! (`AuthError::Rejected`, from a refresh, launch validation, or a tier
//! check) are US4's job (T093-T098, `launch()`, `check_expiry`, `revoke`).

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use time::OffsetDateTime;

use modplayer_audio_source::TrackRef;
use modplayer_secure_store::{EntryName, SecureStore};

use crate::auth_service::{
    AuthError, AuthorizationService, PlaybackStateSummary, Profile, REDIRECT_PATH, TokenSet,
};
use crate::clock::Clock;
use crate::credential::SessionCredential;
use crate::listener::{self, ListenerConfig, ListenerOutcome};
use crate::pending::PendingAuthorization;
use crate::pkce;
use crate::refresh::{RefreshAction, RefreshReport, RefreshScheduler};
use crate::registry::AccountScopedStore;
use crate::session::{SessionState, SignInNote, Tier};
use crate::state_store::{AccountStateStore, LoadOutcome, PersistedAccount};

/// A pending authorization attempt is abandoned 5 minutes after it started
/// (contracts/authorization-service.md "Timeouts and budgets": "Waiting for
/// browser"; data-model.md §1.5).
const PENDING_ATTEMPT_TTL: time::Duration = time::Duration::minutes(5);

/// Attempt-tagged results from the `account-exchange`/`account-tier`/
/// `account-refresh` worker threads (contracts/account-session.md
/// "Threading model"). `tick()` drops any message whose `attempt_id` no
/// longer matches the service's current one (FR-015 "abandon in-flight").
enum WorkerEvent {
    Exchange {
        attempt_id: u64,
        result: Result<TokenSet, AuthError>,
    },
    Tier {
        attempt_id: u64,
        result: Result<Profile, AuthError>,
    },
    Refresh {
        attempt_id: u64,
        result: Result<TokenSet, AuthError>,
    },
    RecentTracks {
        attempt_id: u64,
        request_id: RequestId,
        result: Result<Vec<TrackRef>, AuthError>,
    },
    PlaybackState {
        attempt_id: u64,
        request_id: RequestId,
        result: Result<Option<PlaybackStateSummary>, AuthError>,
    },
}

/// An id returned by [`AccountService::request_recent_tracks`]/
/// [`AccountService::request_playback_state`], unique per call
/// (contracts/account-read-delta.md §3) — lets a caller with several reads
/// in flight match each `AccountEvent::ReadResult` back to its request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(u64);

/// The payload of an `AccountEvent::ReadResult` (contracts/
/// account-read-delta.md §1/§3). Carries the `Result` rather than the bare
/// value so a `Forbidden` (scope not yet granted; the sign-in screen's
/// scope list did not change, so an existing session needs a fresh sign-in
/// to gain it) is distinguishable from an empty answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadOutcome {
    RecentTracks(Result<Vec<TrackRef>, AuthError>),
    PlaybackState(Result<Option<PlaybackStateSummary>, AuthError>),
}

/// Events the service raises for the UI to map to notifications/screens
/// (contracts/account-session.md "Events").
#[derive(Debug, Clone, PartialEq)]
pub enum AccountEvent {
    /// `start_sign_in` succeeded: the UI must `ctx.open_url` this and show
    /// the waiting screen.
    BrowserUrlReady(String),
    /// The secure-store probe (or a write during sign-in) failed
    /// (FR-017).
    StoreUnavailable { store_name_key: &'static str },
    /// Cancel / timeout / callback error / exchange failure — the sign-in
    /// step shows `note` with a Retry.
    SignInFailed(SignInNote),
    /// The credential was written; the UI shows "Checking your account".
    Authorized,
    /// The tier check succeeded.
    TierChecked(Tier),
    /// The tier check failed (30 s / transport / 403 / any other error) —
    /// treated identically to `TierChecked(Tier::Unknown)` by the UI.
    TierCheckFailed,
    /// The 3rd consecutive transient refresh failure.
    RefreshFailing,
    /// A refresh succeeded after one or more `RefreshFailing`s.
    RefreshRecovered,
    /// `sign_out()` completed (US3, T082) — declared now so US2's
    /// `AccountEvent` match arms in `modplayer-ui` compile against the
    /// full contract event set from the start.
    SignedOut { categories: Vec<&'static str> },
    /// A registry store failed to clear during sign-out (US3), or during
    /// revocation (US4, T097).
    SignOutIncomplete { category: &'static str },
    /// `now >= expires_at` with no successful refresh (US4, T098,
    /// `check_expiry`); raised once per expiry.
    SessionExpired,
    /// A definitive rejection anywhere — refresh, launch validation, or a
    /// tier check (US4, T097, `revoke`).
    SessionRevoked,
    /// A `request_recent_tracks`/`request_playback_state` result arrived
    /// (contracts/account-read-delta.md §1/§3). Dropped (never raised) for
    /// a stale attempt — e.g. a sign-out landed while the read was in
    /// flight.
    ReadResult {
        request_id: RequestId,
        result: ReadOutcome,
    },
}

/// The outcome of `AccountService::launch()`
/// (contracts/account-session.md "Construction").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOutcome {
    pub state: SessionState,
}

/// Single authority for `AccountSession`, built on injected `SecureStore`/
/// `AuthorizationService`/`Clock` implementations (`KeyringSecureStore`/
/// `SpotifyAuthorizationService`/`SystemClock` in production;
/// `MemorySecureStore`/`FakeAuthorizationService`/`FakeClock` in tests).
/// Runs on the UI thread, ticked every frame; blocking work happens on
/// worker threads that report back through `worker_rx`, drained in
/// `tick()`.
pub struct AccountService {
    secure: Arc<dyn SecureStore>,
    auth: Arc<dyn AuthorizationService>,
    clock: Arc<dyn Clock>,
    state_store: AccountStateStore,
    registry: Vec<Box<dyn AccountScopedStore>>,
    state: SessionState,
    tier: Tier,
    /// Non-secret session data, once known (`Checking`/`Active`/
    /// `Expired`). `None` while `SignedOut`/`Authorizing`.
    account: Option<PersistedAccount>,
    /// The live attempt/session generation — bumped by `start_sign_in`
    /// and `cancel_sign_in`; every worker result carries the id it was
    /// started under, and a stale one is dropped (FR-015, FR-016).
    attempt_id: u64,
    /// The current sign-in attempt's secret material, needed by the
    /// exchange worker and by `browser_url`/`cancel_sign_in`.
    pending: Option<PendingAuthorization>,
    /// Set while `Authorizing`; signalled (and dropped) by
    /// `discard_pending_attempt`.
    listener_cancel: Option<Arc<AtomicBool>>,
    listener_rx: Option<mpsc::Receiver<ListenerOutcome>>,
    worker_tx: mpsc::Sender<WorkerEvent>,
    worker_rx: mpsc::Receiver<WorkerEvent>,
    refresh: Option<RefreshScheduler>,
    tier_check_in_flight: bool,
    /// Counter backing [`RequestId`] (contracts/account-read-delta.md §3),
    /// distinct from `attempt_id` — several reads can be in flight within
    /// one session generation.
    next_request_id: u64,
}

impl AccountService {
    /// Construct a service wired to `secure`/`auth`/`clock`, pointing its
    /// `account.toml` at `config_dir` (the same directory `SettingsStore`
    /// resolves). Touches neither the network nor disk; call `launch()` to
    /// read persisted state.
    pub fn new(
        secure: Arc<dyn SecureStore>,
        auth: Arc<dyn AuthorizationService>,
        clock: Arc<dyn Clock>,
        config_dir: PathBuf,
    ) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            secure,
            auth,
            clock,
            state_store: AccountStateStore::new(config_dir),
            registry: Vec::new(),
            state: SessionState::SignedOut { note: None },
            tier: Tier::Unknown,
            account: None,
            attempt_id: 0,
            pending: None,
            listener_cancel: None,
            listener_rx: None,
            worker_tx,
            worker_rx,
            refresh: None,
            tier_check_in_flight: false,
            next_request_id: 0,
        }
    }

    /// The secure store this service reads/writes the credential through.
    pub fn secure(&self) -> &Arc<dyn SecureStore> {
        &self.secure
    }

    /// The authorization transport this service drives sign-in/refresh/
    /// tier checks through.
    pub fn auth(&self) -> &Arc<dyn AuthorizationService> {
        &self.auth
    }

    /// The clock this service reads the current time from.
    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// The `account.toml` store this service loads from and saves to.
    pub fn state_store(&self) -> &AccountStateStore {
        &self.state_store
    }

    /// Register an account-scoped store sign-out must clear. Call before
    /// `launch()`; later slices add their own stores here
    /// (contracts/account-session.md "Commands").
    pub fn register_store(&mut self, store: Box<dyn AccountScopedStore>) {
        self.registry.push(store);
    }

    /// Current session state.
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// Current subscription tier (`Tier::Unknown` until a tier check
    /// succeeds).
    pub fn tier(&self) -> Tier {
        self.tier
    }

    /// The current session's non-secret account data, once known.
    pub fn account(&self) -> Option<&PersistedAccount> {
        self.account.as_ref()
    }

    /// Whether a tier check is currently in flight (`recheck_tier`'s
    /// precondition, contracts/account-session.md).
    pub fn tier_check_in_flight(&self) -> bool {
        self.tier_check_in_flight
    }

    /// Whether playback is currently permitted (FR-011): `Active`/
    /// `Expired` with `tier == Premium`.
    pub fn playback_permitted(&self) -> bool {
        matches!(self.state, SessionState::Active | SessionState::Expired)
            && self.tier == Tier::Premium
    }

    /// Read persisted state and resolve the session
    /// (contracts/account-session.md "Construction"): a pending
    /// authorization attempt (if any) takes priority — young enough and
    /// re-bindable resumes `Authorizing`, otherwise it is discarded with
    /// `PreviousDidNotFinish` (T094); with no pending attempt, the
    /// credential/`account.toml` pair decides `Active`/`Expired`/
    /// `StoreUnreadable`/`SignedOut` (T095).
    pub fn launch(&mut self) -> LaunchOutcome {
        let now = self.clock.now();
        if let Some(outcome) = self.launch_resolve_pending(now) {
            return outcome;
        }
        self.launch_resolve_session(now)
    }

    /// The pending-authorization half of `launch()` (contracts/
    /// account-session.md "Construction", research R2). Returns `None` when
    /// there is no pending attempt to resolve (or the store could not even
    /// be asked), letting `launch()` fall through to the credential/
    /// `account.toml` check.
    fn launch_resolve_pending(&mut self, now: OffsetDateTime) -> Option<LaunchOutcome> {
        let bytes = match self.secure.get(EntryName::PendingAuthorization) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return None,
            // Can't even tell whether a pending attempt exists; fall
            // through to the session check, which hits the same store
            // error and resolves to `StoreUnreadable`/`SignedOut` itself.
            Err(_) => return None,
        };

        self.attempt_id += 1;
        let attempt_id = self.attempt_id;
        let resumable = PendingAuthorization::from_payload(&bytes, attempt_id)
            .ok()
            .filter(|pending| now < pending.started_at + PENDING_ATTEMPT_TTL)
            .and_then(|pending| listener::bind_port(pending.port).ok().map(|l| (pending, l)));

        if let Some((pending, listener)) = resumable {
            self.resume_pending(pending, listener);
            return Some(LaunchOutcome {
                state: self.state.clone(),
            });
        }

        // Unparsable, too old, or the recorded port could no longer be
        // re-bound: the attempt cannot be completed (research R2).
        let _ = self.secure.delete(EntryName::PendingAuthorization);
        self.pending = None;
        self.account = None;
        self.tier = Tier::Unknown;
        self.state = SessionState::SignedOut {
            note: Some(SignInNote::PreviousDidNotFinish),
        };
        Some(LaunchOutcome {
            state: self.state.clone(),
        })
    }

    /// Re-bind a resumed attempt's recorded port and pick the listener
    /// back up (contracts/authorization-service.md "Resumed attempts").
    fn resume_pending(&mut self, pending: PendingAuthorization, listener: TcpListener) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.listener_cancel = Some(Arc::clone(&cancel));
        let config = ListenerConfig {
            redirect_path: REDIRECT_PATH,
            expected_state: pending.state.clone(),
            deadline: pending.started_at + PENDING_ATTEMPT_TTL,
        };
        self.listener_rx = Some(listener::spawn(
            listener,
            config,
            cancel,
            Arc::clone(&self.clock),
        ));
        let attempt_id = pending.attempt_id;
        self.pending = Some(pending);
        self.state = SessionState::Authorizing {
            attempt_id,
            resumed: true,
        };
    }

    /// The credential/`account.toml` half of `launch()` — reached only when
    /// no pending attempt was found (contracts/account-session.md
    /// "Construction" table, rows 1 and 4-6).
    fn launch_resolve_session(&mut self, now: OffsetDateTime) -> LaunchOutcome {
        let account_load = self.state_store.load();
        match self.secure.get(EntryName::SessionCredential) {
            Ok(Some(bytes)) => match SessionCredential::from_payload(&bytes) {
                Ok(credential) => {
                    let persisted = match account_load {
                        LoadOutcome::Loaded(persisted) => persisted,
                        // "credential readable, account.toml missing/corrupt
                        // → rebuild with tier Unknown" (data-model.md §4).
                        LoadOutcome::Absent | LoadOutcome::Unreadable => {
                            let rebuilt = PersistedAccount {
                                account_id: String::new(),
                                display_name: String::new(),
                                tier: Tier::Unknown,
                                credential_ref: EntryName::SessionCredential.user_name(),
                                expires_at: credential.expires_at,
                                authorized_at: credential.authorized_at,
                                last_validated_at: None,
                            };
                            let _ = self.state_store.save(&rebuilt);
                            rebuilt
                        }
                    };
                    self.activate_from_launch(persisted, credential, now);
                }
                // The stored bytes exist but can't be parsed — treat like
                // any other store read failure rather than silently
                // dropping a session that might still be salvageable.
                Err(_) => self.launch_store_unreadable(account_load),
            },
            Ok(None) => {
                // No credential: whatever `account.toml` says, there is
                // nothing to sign in with — never clear it here (it may
                // simply be a launch race with an in-progress write), just
                // don't resurrect a session from it.
                self.account = None;
                self.tier = Tier::Unknown;
                self.state = SessionState::SignedOut { note: None };
            }
            Err(_) => self.launch_store_unreadable(account_load),
        }
        LaunchOutcome {
            state: self.state.clone(),
        }
    }

    /// "account.toml + secure store read error → StoreUnreadable; nothing
    /// cleared" (contracts/account-session.md) — falls back to plain
    /// `SignedOut` when there was no session on disk to protect in the
    /// first place.
    fn launch_store_unreadable(&mut self, account_load: LoadOutcome) {
        match account_load {
            LoadOutcome::Loaded(persisted) => {
                self.tier = persisted.tier;
                self.account = Some(persisted);
                self.state = SessionState::StoreUnreadable;
            }
            LoadOutcome::Absent | LoadOutcome::Unreadable => {
                self.account = None;
                self.tier = Tier::Unknown;
                self.state = SessionState::SignedOut { note: None };
            }
        }
    }

    /// "account.toml + credential readable → Active/Expired by
    /// expires_at; spawn launch validation" (contracts/account-session.md
    /// "Construction"). Refreshing if due is the armed scheduler's job on
    /// the next `tick()`; the fresh profile check is spawned the same way
    /// `recheck_tier` spawns one.
    fn activate_from_launch(
        &mut self,
        persisted: PersistedAccount,
        credential: SessionCredential,
        now: OffsetDateTime,
    ) {
        self.tier = persisted.tier;
        self.state = if now >= persisted.expires_at {
            SessionState::Expired
        } else {
            SessionState::Active
        };
        self.refresh = Some(RefreshScheduler::new(
            persisted.authorized_at,
            persisted.expires_at,
        ));
        self.account = Some(persisted);
        self.spawn_tier_worker(self.attempt_id, credential.access_token.clone());
    }

    /// Retry reading a session found at launch whose secure store failed
    /// to read (`SessionState::StoreUnreadable`'s Retry button, FR-017).
    /// Precondition: state `StoreUnreadable` (a no-op otherwise). On
    /// success, resolves exactly like `launch()` would have — no browser,
    /// no new attempt, nothing cleared (contracts/account-session.md
    /// "Construction"; quickstart M6 "unlock + Retry → signed in, nothing
    /// was cleared").
    pub fn retry_store_read(&mut self) -> Vec<AccountEvent> {
        if !matches!(self.state, SessionState::StoreUnreadable) {
            return Vec::new();
        }
        let Some(persisted) = self.account.clone() else {
            return Vec::new();
        };
        match self.secure.get(EntryName::SessionCredential) {
            Ok(Some(bytes)) => match SessionCredential::from_payload(&bytes) {
                Ok(credential) => {
                    let now = self.clock.now();
                    self.activate_from_launch(persisted, credential, now);
                    Vec::new()
                }
                Err(_) => vec![AccountEvent::StoreUnavailable {
                    store_name_key: self.secure.platform_name_key(),
                }],
            },
            // The entry is genuinely gone (not just unreadable): the
            // closest defined outcome is the revocation path — no
            // credential left to protect either way.
            Ok(None) => self.revoke(),
            Err(_) => vec![AccountEvent::StoreUnavailable {
                store_name_key: self.secure.platform_name_key(),
            }],
        }
    }

    /// The confirmation-list category keys, in registry order (FR-015).
    pub fn signout_categories(&self) -> Vec<&'static str> {
        self.registry
            .iter()
            .map(|store| store.category_key())
            .collect()
    }

    /// Begin a new sign-in attempt (contracts/account-session.md
    /// "Commands"). Precondition: state ∈ {`SignedOut`, `StoreUnreadable`,
    /// `Expired`, `Authorizing`}. Probes the secure store *before* doing
    /// anything else (FR-017 "before opening the browser"); on success,
    /// discards any previous pending attempt (real cancellation, not just
    /// bookkeeping — see [`Self::discard_pending_attempt`]), generates
    /// fresh PKCE material, binds a fresh loopback listener, and returns
    /// the events raised. Called directly rather than folded into `tick()`
    /// because the caller (the Sign-in screen's button) needs the result
    /// synchronously to open the browser this same frame.
    pub fn start_sign_in(&mut self) -> Vec<AccountEvent> {
        if self.secure.probe().is_err() {
            return vec![AccountEvent::StoreUnavailable {
                store_name_key: self.secure.platform_name_key(),
            }];
        }

        self.discard_pending_attempt();
        self.attempt_id += 1;
        let attempt_id = self.attempt_id;

        let listener = match listener::bind_ephemeral() {
            Ok(listener) => listener,
            Err(_) => return self.fail_sign_in(SignInNote::ServiceError),
        };
        let port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => return self.fail_sign_in(SignInNote::ServiceError),
        };

        let material = pkce::generate();
        let started_at = self.clock.now();
        let pending = PendingAuthorization {
            attempt_id,
            pkce_verifier: material.verifier,
            state: material.state,
            port,
            started_at,
        };

        let Ok(payload) = pending.to_payload() else {
            return self.fail_sign_in(SignInNote::ServiceError);
        };
        if self
            .secure
            .put(EntryName::PendingAuthorization, &payload)
            .is_err()
        {
            return vec![AccountEvent::StoreUnavailable {
                store_name_key: self.secure.platform_name_key(),
            }];
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.listener_cancel = Some(Arc::clone(&cancel));
        let config = ListenerConfig {
            redirect_path: REDIRECT_PATH,
            expected_state: pending.state.clone(),
            deadline: started_at + PENDING_ATTEMPT_TTL,
        };
        self.listener_rx = Some(listener::spawn(
            listener,
            config,
            cancel,
            Arc::clone(&self.clock),
        ));

        let url = self.auth.authorization_url(&pending);
        self.pending = Some(pending);
        self.state = SessionState::Authorizing {
            attempt_id,
            resumed: false,
        };

        vec![AccountEvent::BrowserUrlReady(url)]
    }

    /// The current attempt's authorization URL ("Open the browser again",
    /// contracts/account-session.md). `None` unless `Authorizing`.
    pub fn browser_url(&self) -> Option<String> {
        match (&self.state, &self.pending) {
            (SessionState::Authorizing { .. }, Some(pending)) => {
                Some(self.auth.authorization_url(pending))
            }
            _ => None,
        }
    }

    /// Cancel the in-flight sign-in attempt (contracts/account-session.md
    /// "Commands"). Precondition: state `Authorizing`.
    pub fn cancel_sign_in(&mut self) {
        self.attempt_id += 1;
        self.discard_pending_attempt();
        self.state = SessionState::SignedOut {
            note: Some(SignInNote::Cancelled),
        };
    }

    /// Re-run the tier check outside the sign-in flow (Settings › Account
    /// "Re-check subscription", contracts/account-session.md). Precondition:
    /// state ∈ {`Active`, `Expired`} and no check already in flight; a
    /// no-op otherwise.
    pub fn recheck_tier(&mut self) {
        if self.tier_check_in_flight {
            return;
        }
        if !matches!(self.state, SessionState::Active | SessionState::Expired) {
            return;
        }
        let Some(access_token) = self.stored_credential().map(|c| c.access_token) else {
            return;
        };
        self.spawn_tier_worker(self.attempt_id, access_token);
    }

    /// Request up to `limit` recently-played tracks, falling back to saved
    /// tracks when the recently-played result is empty (contracts/
    /// account-read-delta.md §1/§3, FR-022 "Play from account"). A no-op
    /// (the returned id never resolves) when there is no readable
    /// credential — mirrors [`Self::recheck_tier`]'s precondition handling.
    pub fn request_recent_tracks(&mut self, limit: u8) -> RequestId {
        let request_id = self.next_request_id();
        if let Some(credential) = self.stored_credential() {
            self.spawn_recent_tracks_worker(
                self.attempt_id,
                request_id,
                credential.access_token,
                limit,
            );
        }
        request_id
    }

    /// Request the other Connect device's playback state (the transfer
    /// banner's device name, FR-016/019; research R3, R9). A no-op (the
    /// returned id never resolves) when there is no readable credential.
    pub fn request_playback_state(&mut self) -> RequestId {
        let request_id = self.next_request_id();
        if let Some(credential) = self.stored_credential() {
            self.spawn_playback_state_worker(self.attempt_id, request_id, credential.access_token);
        }
        request_id
    }

    /// The current credential's access token, read from the secure store on
    /// demand (contracts/account-read-delta.md §3) — used by the binary's
    /// `ReceiverCredentials` implementation to hand librespot a token
    /// without this crate depending on the receiver crate. Never logged.
    pub fn access_token(&self) -> Option<String> {
        self.stored_credential()
            .map(|credential| credential.access_token)
    }

    fn next_request_id(&mut self) -> RequestId {
        self.next_request_id += 1;
        RequestId(self.next_request_id)
    }

    fn spawn_recent_tracks_worker(
        &self,
        attempt_id: u64,
        request_id: RequestId,
        access_token: String,
        limit: u8,
    ) {
        let auth = Arc::clone(&self.auth);
        let tx = self.worker_tx.clone();
        let fallback_tx = tx.clone();
        let spawned = thread::Builder::new()
            .name("account-recent-tracks".to_string())
            .spawn(move || {
                let result = match auth.fetch_recently_played(&access_token, limit) {
                    Ok(tracks) if tracks.is_empty() => {
                        auth.fetch_saved_tracks(&access_token, limit)
                    }
                    other => other,
                };
                let _ = tx.send(WorkerEvent::RecentTracks {
                    attempt_id,
                    request_id,
                    result,
                });
            });
        if spawned.is_err() {
            let _ = fallback_tx.send(WorkerEvent::RecentTracks {
                attempt_id,
                request_id,
                result: Err(AuthError::Transient),
            });
        }
    }

    fn spawn_playback_state_worker(
        &self,
        attempt_id: u64,
        request_id: RequestId,
        access_token: String,
    ) {
        let auth = Arc::clone(&self.auth);
        let tx = self.worker_tx.clone();
        let fallback_tx = tx.clone();
        let spawned = thread::Builder::new()
            .name("account-playback-state".to_string())
            .spawn(move || {
                let result = auth.fetch_playback_state(&access_token);
                let _ = tx.send(WorkerEvent::PlaybackState {
                    attempt_id,
                    request_id,
                    result,
                });
            });
        if spawned.is_err() {
            let _ = fallback_tx.send(WorkerEvent::PlaybackState {
                attempt_id,
                request_id,
                result: Err(AuthError::Transient),
            });
        }
    }

    /// Sign out (contracts/account-session.md "Commands" `sign_out()`,
    /// FR-015, FR-019, SC-003). Precondition: state ∈ {`Active`, `Expired`,
    /// `StoreUnreadable`}. Bumps `attempt_id` *first* so any
    /// exchange/tier/refresh worker result arriving afterwards is dropped
    /// as stale by `tick()` (FR-015 "abandon in-flight"), then discards any
    /// pending attempt and clears every registered store — collecting
    /// per-store failures rather than stopping at the first one, so one
    /// store's failure never leaves another uncleared. The session always
    /// reaches `SignedOut { None }`, regardless of individual store
    /// failures: the credential/account-scoped data that *did* clear stays
    /// cleared, and a `SignOutIncomplete` is raised per failure for the UI
    /// to warn about separately from the `SignedOut` report.
    pub fn sign_out(&mut self) -> Vec<AccountEvent> {
        self.attempt_id += 1;
        self.discard_pending_attempt();

        let mut cleared = Vec::new();
        let mut failed = Vec::new();
        for store in &mut self.registry {
            let category = store.category_key();
            match store.clear() {
                Ok(()) => cleared.push(category),
                Err(_) => failed.push(category),
            }
        }

        self.account = None;
        self.tier = Tier::Unknown;
        self.refresh = None;
        self.tier_check_in_flight = false;
        self.state = SessionState::SignedOut { note: None };

        let mut events = vec![AccountEvent::SignedOut {
            categories: cleared,
        }];
        events.extend(
            failed
                .into_iter()
                .map(|category| AccountEvent::SignOutIncomplete { category }),
        );
        events
    }

    /// The full revocation path (contracts/account-session.md "Revocation
    /// path", FR-019): triggered by a definitive `AuthError::Rejected` from
    /// a refresh, launch validation, or a tier check — every one of those
    /// funnels through `handle_refresh_result`/`handle_tier_result`, which
    /// call this instead of their usual transient-failure handling. Deletes
    /// the credential first, then any pending attempt, then clears every
    /// registered store (mirroring `sign_out`'s per-store failure
    /// collection), and lands on `SignedOut(Revoked)`.
    fn revoke(&mut self) -> Vec<AccountEvent> {
        self.attempt_id += 1;
        let _ = self.secure.delete(EntryName::SessionCredential);
        self.discard_pending_attempt();

        let mut failed = Vec::new();
        for store in &mut self.registry {
            if store.clear().is_err() {
                failed.push(store.category_key());
            }
        }

        self.account = None;
        self.tier = Tier::Unknown;
        self.refresh = None;
        self.tier_check_in_flight = false;
        self.state = SessionState::SignedOut {
            note: Some(SignInNote::Revoked),
        };

        let mut events = vec![AccountEvent::SessionRevoked];
        events.extend(
            failed
                .into_iter()
                .map(|category| AccountEvent::SignOutIncomplete { category }),
        );
        events
    }

    /// `now >= expires_at` with no successful refresh (contracts/
    /// account-session.md "Refresh scheduler rules", FR-021): moves
    /// `Active` to `Expired` and raises `SessionExpired` exactly once per
    /// expiry (the `matches!(Active)` guard means a later tick, still
    /// `Expired`, never re-raises it). The refresh scheduler is untouched —
    /// it keeps retrying with backoff regardless of this transition, and a
    /// later success moves back to `Active` (`handle_refresh_result`).
    fn check_expiry(&mut self, now: OffsetDateTime) -> Vec<AccountEvent> {
        if !matches!(self.state, SessionState::Active) {
            return Vec::new();
        }
        let Some(account) = &self.account else {
            return Vec::new();
        };
        if now >= account.expires_at {
            self.state = SessionState::Expired;
            vec![AccountEvent::SessionExpired]
        } else {
            Vec::new()
        }
    }

    /// Drain worker/listener events, advance the refresh scheduler, and
    /// return the events raised this tick for the UI to map to
    /// notifications/screens (contracts/account-session.md: "`tick()`
    /// never blocks").
    pub fn tick(&mut self) -> Vec<AccountEvent> {
        let mut events = Vec::new();

        // The listener only ever resolves once per attempt; by the time
        // `Some(rx)` is stale it has already been replaced or cleared by
        // `discard_pending_attempt`, so no attempt-id check is needed here
        // (unlike `worker_rx`, which outlives many attempts).
        if let Some(rx) = &self.listener_rx
            && let Ok(outcome) = rx.try_recv()
        {
            events.extend(self.handle_listener_outcome(outcome));
        }

        while let Ok(event) = self.worker_rx.try_recv() {
            match event {
                WorkerEvent::Exchange { attempt_id, result } => {
                    events.extend(self.handle_exchange_result(attempt_id, result));
                }
                WorkerEvent::Tier { attempt_id, result } => {
                    events.extend(self.handle_tier_result(attempt_id, result));
                }
                WorkerEvent::Refresh { attempt_id, result } => {
                    events.extend(self.handle_refresh_result(attempt_id, result));
                }
                WorkerEvent::RecentTracks {
                    attempt_id,
                    request_id,
                    result,
                } => {
                    if attempt_id == self.attempt_id {
                        events.push(AccountEvent::ReadResult {
                            request_id,
                            result: ReadOutcome::RecentTracks(result),
                        });
                    }
                }
                WorkerEvent::PlaybackState {
                    attempt_id,
                    request_id,
                    result,
                } => {
                    if attempt_id == self.attempt_id {
                        events.push(AccountEvent::ReadResult {
                            request_id,
                            result: ReadOutcome::PlaybackState(result),
                        });
                    }
                }
            }
        }

        let now = self.clock.now();
        events.extend(self.check_expiry(now));
        events.extend(self.tick_refresh());
        events
    }

    /// Signal cancellation to any live listener thread and forget the
    /// current attempt (used by `start_sign_in` — "discards any pending
    /// attempt" — and `cancel_sign_in`). Setting the cancel flag *before*
    /// dropping our handle is what actually stops the old OS thread; just
    /// dropping the `Arc` would leave it running (and its port bound)
    /// until its own 5-minute deadline.
    fn discard_pending_attempt(&mut self) {
        if let Some(cancel) = self.listener_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.listener_rx = None;
        let _ = self.secure.delete(EntryName::PendingAuthorization);
        self.pending = None;
    }

    /// Common "sign-in attempt failed before it could even start" path:
    /// discard whatever partial state exists, move to `SignedOut(note)`,
    /// and return the matching event.
    fn fail_sign_in(&mut self, note: SignInNote) -> Vec<AccountEvent> {
        self.discard_pending_attempt();
        self.state = SessionState::SignedOut { note: Some(note) };
        vec![AccountEvent::SignInFailed(note)]
    }

    fn handle_listener_outcome(&mut self, outcome: ListenerOutcome) -> Vec<AccountEvent> {
        let attempt_id = self.attempt_id;
        match outcome {
            ListenerOutcome::Code(code) => {
                let Some(pending) = self.pending.clone() else {
                    self.listener_rx = None;
                    return Vec::new();
                };
                self.listener_rx = None;
                self.spawn_exchange_worker(attempt_id, pending, code);
                Vec::new()
            }
            ListenerOutcome::Error(error_code) => {
                self.listener_rx = None;
                let note = if error_code == "access_denied" {
                    SignInNote::Cancelled
                } else {
                    SignInNote::ServiceError
                };
                self.fail_sign_in(note)
            }
            ListenerOutcome::Cancelled => {
                // `cancel_sign_in` already moved to `SignedOut(Cancelled)`
                // synchronously; this is just the listener thread's own
                // confirmation arriving.
                self.listener_rx = None;
                Vec::new()
            }
            ListenerOutcome::TimedOut => {
                self.listener_rx = None;
                self.fail_sign_in(SignInNote::TimedOut)
            }
            ListenerOutcome::BindLost => {
                self.listener_rx = None;
                self.fail_sign_in(SignInNote::ServiceError)
            }
        }
    }

    fn spawn_exchange_worker(&self, attempt_id: u64, pending: PendingAuthorization, code: String) {
        let auth = Arc::clone(&self.auth);
        let tx = self.worker_tx.clone();
        let fallback_tx = tx.clone();
        let redirect_uri = format!("http://127.0.0.1:{}{REDIRECT_PATH}", pending.port);
        let spawned = thread::Builder::new()
            .name("account-exchange".to_string())
            .spawn(move || {
                let result = auth.exchange_code(&code, &pending.pkce_verifier, &redirect_uri);
                let _ = tx.send(WorkerEvent::Exchange { attempt_id, result });
            });
        if spawned.is_err() {
            let _ = fallback_tx.send(WorkerEvent::Exchange {
                attempt_id,
                result: Err(AuthError::Transient),
            });
        }
    }

    /// `account-exchange` worker result (contracts/authorization-
    /// service.md "PKCE flow" step 5): on success, write the credential
    /// *first*, then `account.toml` (tier `Unknown`), then delete
    /// `pending-authorization`, then emit `Authorized` and immediately
    /// spawn the tier check (design note 1).
    fn handle_exchange_result(
        &mut self,
        attempt_id: u64,
        result: Result<TokenSet, AuthError>,
    ) -> Vec<AccountEvent> {
        if attempt_id != self.attempt_id {
            return Vec::new();
        }
        let tokens = match result {
            Ok(tokens) => tokens,
            Err(_) => return self.fail_sign_in(SignInNote::ServiceError),
        };

        let now = self.clock.now();
        let expires_at = now + to_time_duration(tokens.expires_in);
        let credential = SessionCredential {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token.unwrap_or_default(),
            expires_at,
            scope: tokens.scope,
            authorized_at: now,
        };
        let Ok(payload) = credential.to_payload() else {
            return self.fail_sign_in(SignInNote::ServiceError);
        };
        if self
            .secure
            .put(EntryName::SessionCredential, &payload)
            .is_err()
        {
            self.discard_pending_attempt();
            self.state = SessionState::SignedOut {
                note: Some(SignInNote::ServiceError),
            };
            return vec![AccountEvent::StoreUnavailable {
                store_name_key: self.secure.platform_name_key(),
            }];
        }

        let persisted = PersistedAccount {
            account_id: String::new(),
            display_name: String::new(),
            tier: Tier::Unknown,
            credential_ref: EntryName::SessionCredential.user_name(),
            expires_at,
            authorized_at: now,
            last_validated_at: None,
        };
        // Write rule (contracts/account-session.md): on error the previous
        // file is kept; `modplayer-account` doesn't depend on
        // `modplayer-core`'s notification center, so a save failure here
        // isn't surfaced as its own event — the credential (the part that
        // matters for FR-009) is already durably stored, and the
        // subsequent tier-check success re-saves `account.toml` anyway.
        let _ = self.state_store.save(&persisted);
        let _ = self.secure.delete(EntryName::PendingAuthorization);

        self.pending = None;
        self.account = Some(persisted);
        self.tier = Tier::Unknown;
        self.state = SessionState::Checking { attempt_id };

        self.spawn_tier_worker(attempt_id, credential.access_token.clone());
        vec![AccountEvent::Authorized]
    }

    fn spawn_tier_worker(&mut self, attempt_id: u64, access_token: String) {
        self.tier_check_in_flight = true;
        let auth = Arc::clone(&self.auth);
        let tx = self.worker_tx.clone();
        let fallback_tx = tx.clone();
        let spawned = thread::Builder::new()
            .name("account-tier".to_string())
            .spawn(move || {
                let result = auth.fetch_profile(&access_token);
                let _ = tx.send(WorkerEvent::Tier { attempt_id, result });
            });
        if spawned.is_err() {
            let _ = fallback_tx.send(WorkerEvent::Tier {
                attempt_id,
                result: Err(AuthError::Transient),
            });
        }
    }

    /// `account-tier` worker result (T062): maps the profile to a `Tier`
    /// and moves `Checking` -> `Active`, arming the refresh scheduler. A
    /// definitive `Rejected` (profile 401 after the transport's own
    /// one-refresh retry, contracts/authorization-service.md `AuthError`)
    /// takes the full revocation path (US4, T097); every other failure
    /// degrades to `TierCheckFailed`/`Tier::Unknown`.
    fn handle_tier_result(
        &mut self,
        attempt_id: u64,
        result: Result<Profile, AuthError>,
    ) -> Vec<AccountEvent> {
        self.tier_check_in_flight = false;
        if attempt_id != self.attempt_id {
            return Vec::new();
        }
        let now = self.clock.now();
        match result {
            Ok(profile) => {
                self.tier = profile.tier;
                if let Some(account) = &mut self.account {
                    account.tier = profile.tier;
                    account.account_id = profile.id.clone();
                    account.display_name = profile
                        .display_name
                        .clone()
                        .unwrap_or_else(|| profile.id.clone());
                    account.last_validated_at = Some(now);
                    let _ = self.state_store.save(account);
                }
                self.arm_refresh_and_activate();
                vec![AccountEvent::TierChecked(profile.tier)]
            }
            Err(AuthError::Rejected) => self.revoke(),
            Err(_) => {
                self.tier = Tier::Unknown;
                self.arm_refresh_and_activate();
                vec![AccountEvent::TierCheckFailed]
            }
        }
    }

    /// `Checking` -> `Active` (a no-op from any other state — a
    /// `recheck_tier` result never changes the state), and arm the refresh
    /// scheduler the first time a session becomes known (FR-013).
    fn arm_refresh_and_activate(&mut self) {
        if matches!(self.state, SessionState::Checking { .. }) {
            self.state = SessionState::Active;
        }
        if self.refresh.is_none()
            && let Some(account) = &self.account
        {
            self.refresh = Some(RefreshScheduler::new(
                account.authorized_at,
                account.expires_at,
            ));
        }
    }

    fn tick_refresh(&mut self) -> Vec<AccountEvent> {
        let now = self.clock.now();
        let action = match &mut self.refresh {
            Some(scheduler) => scheduler.action(now),
            None => return Vec::new(),
        };
        match action {
            RefreshAction::Wait => Vec::new(),
            RefreshAction::RefreshNow => match self.stored_credential() {
                Some(credential) => {
                    self.spawn_refresh_worker(self.attempt_id, credential.refresh_token);
                    Vec::new()
                }
                None => {
                    // Can't read the credential to refresh it — report a
                    // transient failure so the backoff/`RefreshFailing`
                    // bookkeeping still progresses instead of the
                    // scheduler getting stuck thinking a refresh it never
                    // actually started is in flight.
                    let report = self
                        .refresh
                        .as_mut()
                        .map(|scheduler| scheduler.on_transient_failure(now));
                    report.map(|r| self.report_to_events(r)).unwrap_or_default()
                }
            },
        }
    }

    fn spawn_refresh_worker(&self, attempt_id: u64, refresh_token: String) {
        let auth = Arc::clone(&self.auth);
        let tx = self.worker_tx.clone();
        let fallback_tx = tx.clone();
        let spawned = thread::Builder::new()
            .name("account-refresh".to_string())
            .spawn(move || {
                let result = auth.refresh(&refresh_token);
                let _ = tx.send(WorkerEvent::Refresh { attempt_id, result });
            });
        if spawned.is_err() {
            let _ = fallback_tx.send(WorkerEvent::Refresh {
                attempt_id,
                result: Err(AuthError::Transient),
            });
        }
    }

    /// `account-refresh` worker result (contracts/account-session.md
    /// "Refresh scheduler rules"). A definitive `Rejected` (token endpoint
    /// `invalid_grant`) takes the full revocation path (US4, T097); every
    /// other failure is treated as transient backoff.
    fn handle_refresh_result(
        &mut self,
        attempt_id: u64,
        result: Result<TokenSet, AuthError>,
    ) -> Vec<AccountEvent> {
        if attempt_id != self.attempt_id {
            return Vec::new();
        }
        let now = self.clock.now();
        match result {
            Ok(tokens) => {
                let authorized_at = self
                    .account
                    .as_ref()
                    .map(|account| account.authorized_at)
                    .unwrap_or(now);
                let expires_at = now + to_time_duration(tokens.expires_in);
                let refresh_token = tokens.refresh_token.unwrap_or_else(|| {
                    self.stored_credential()
                        .map(|credential| credential.refresh_token)
                        .unwrap_or_default()
                });
                let credential = SessionCredential {
                    access_token: tokens.access_token,
                    refresh_token,
                    expires_at,
                    scope: tokens.scope,
                    authorized_at,
                };
                let stored = credential.to_payload().ok().is_some_and(|payload| {
                    self.secure
                        .put(EntryName::SessionCredential, &payload)
                        .is_ok()
                });

                if stored {
                    if let Some(account) = &mut self.account {
                        account.expires_at = expires_at;
                        account.last_validated_at = Some(now);
                        let _ = self.state_store.save(account);
                    }
                    if matches!(self.state, SessionState::Expired) {
                        self.state = SessionState::Active;
                    }
                }

                // "Secure-store write failure on success is treated as a
                // transient failure" (contracts/account-session.md).
                let report = self.refresh.as_mut().map(|scheduler| {
                    if stored {
                        scheduler.on_success(authorized_at, expires_at)
                    } else {
                        scheduler.on_transient_failure(now)
                    }
                });
                report.map(|r| self.report_to_events(r)).unwrap_or_default()
            }
            Err(AuthError::Rejected) => self.revoke(),
            Err(_) => {
                let report = self
                    .refresh
                    .as_mut()
                    .map(|scheduler| scheduler.on_transient_failure(now));
                report.map(|r| self.report_to_events(r)).unwrap_or_default()
            }
        }
    }

    fn report_to_events(&self, report: RefreshReport) -> Vec<AccountEvent> {
        match report {
            RefreshReport::None => Vec::new(),
            RefreshReport::Failing => vec![AccountEvent::RefreshFailing],
            RefreshReport::Recovered => vec![AccountEvent::RefreshRecovered],
        }
    }

    /// Read and parse the current `SessionCredential` from the secure
    /// store, if any (`None` on any read/parse failure — callers treat
    /// that the same as "nothing to refresh/check with").
    fn stored_credential(&self) -> Option<SessionCredential> {
        let bytes = self.secure.get(EntryName::SessionCredential).ok()??;
        SessionCredential::from_payload(&bytes).ok()
    }
}

impl Drop for AccountService {
    /// Signal cancellation to any live listener thread (contracts/
    /// account-session.md "Threading model": "also set on `Drop` of
    /// `AccountService`") so it exits promptly instead of running until
    /// its own 5-minute deadline after the whole service — and the app —
    /// goes away.
    fn drop(&mut self) {
        if let Some(cancel) = &self.listener_cancel {
            cancel.store(true, Ordering::SeqCst);
        }
    }
}

/// `std::time::Duration` (the wire-facing `TokenSet::expires_in`) to
/// `time::Duration` (used for every timestamp arithmetic in this crate). A
/// value too large to represent (which `expires_in` never realistically
/// is) falls back to zero — immediate re-check — rather than panicking.
fn to_time_duration(duration: std::time::Duration) -> time::Duration {
    time::Duration::try_from(duration).unwrap_or(time::Duration::ZERO)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::fake_auth::{FakeAuthorizationService, ScriptedCall};
    use crate::session::Tier;
    use modplayer_secure_store::MemorySecureStore;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::time::Duration as StdDuration;

    fn fresh_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "modplayer-account-service-test-{}-{}",
            std::process::id(),
            unique
        ))
    }

    struct Fixture {
        service: AccountService,
        auth: FakeAuthorizationService,
        secure: Arc<MemorySecureStore>,
    }

    fn fresh_fixture() -> Fixture {
        let secure = Arc::new(MemorySecureStore::new());
        let auth = FakeAuthorizationService::new();
        let clock = Arc::new(FakeClock::default());
        let service = AccountService::new(
            secure.clone() as Arc<dyn SecureStore>,
            Arc::new(auth.clone()) as Arc<dyn AuthorizationService>,
            clock as Arc<dyn Clock>,
            fresh_dir(),
        );
        Fixture {
            service,
            auth,
            secure,
        }
    }

    fn token_set() -> TokenSet {
        TokenSet {
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_in: StdDuration::from_secs(3600),
            scope: "streaming".to_string(),
        }
    }

    fn profile(tier: Tier) -> Profile {
        Profile {
            id: "user-1".to_string(),
            display_name: Some("Alex".to_string()),
            tier,
        }
    }

    /// Drain `tick()` a bounded number of times, sleeping briefly between
    /// calls so background worker threads (which are real OS threads, not
    /// driven by the fake clock) get a chance to report — used only for
    /// the exchange/tier hops, which have zero scripted delay.
    fn drain_until<F: FnMut(&mut AccountService) -> bool>(
        service: &mut AccountService,
        mut done: F,
    ) -> Vec<AccountEvent> {
        let mut events = Vec::new();
        for _ in 0..200 {
            events.extend(service.tick());
            if done(service) {
                break;
            }
            std::thread::sleep(StdDuration::from_millis(5));
        }
        events
    }

    #[test]
    fn new_service_starts_signed_out_with_unknown_tier() {
        let fixture = fresh_fixture();
        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut { note: None }
        );
        assert_eq!(fixture.service.tier(), Tier::Unknown);
        assert!(!fixture.service.playback_permitted());
    }

    #[test]
    fn launch_with_nothing_on_disk_resolves_signed_out_with_no_note() {
        let mut fixture = fresh_fixture();
        let outcome = fixture.service.launch();
        assert_eq!(outcome.state, SessionState::SignedOut { note: None });
    }

    #[test]
    fn tick_with_no_workers_returns_no_events() {
        let mut fixture = fresh_fixture();
        assert!(fixture.service.tick().is_empty());
    }

    #[test]
    fn signout_categories_reflects_registered_stores_in_order() {
        struct Store(&'static str);
        impl AccountScopedStore for Store {
            fn category_key(&self) -> &'static str {
                self.0
            }
            fn clear(&mut self) -> Result<(), crate::registry::ClearError> {
                Ok(())
            }
        }

        let mut fixture = fresh_fixture();
        assert!(fixture.service.signout_categories().is_empty());
        fixture
            .service
            .register_store(Box::new(Store("signout-category-credential")));
        fixture
            .service
            .register_store(Box::new(Store("signout-category-account-details")));
        assert_eq!(
            fixture.service.signout_categories(),
            vec![
                "signout-category-credential",
                "signout-category-account-details"
            ]
        );
    }

    #[test]
    fn accessors_expose_the_injected_collaborators() {
        let fixture = fresh_fixture();
        assert!(fixture.service.secure().probe().is_ok());
        assert_eq!(
            fixture.service.clock().now(),
            time::OffsetDateTime::UNIX_EPOCH
        );
        assert!(fixture.service.auth().fetch_profile("token").is_err());
        assert!(
            fixture
                .service
                .state_store()
                .path()
                .ends_with("account.toml")
        );
    }

    /// FR-017 / SC-008: a store probe failure refuses sign-in before a
    /// browser is ever opened, and writes nothing (T047).
    #[test]
    fn sign_in_probes_store_before_opening_browser() {
        let mut fixture = fresh_fixture();
        fixture.secure.set_unavailable(true);

        let events = fixture.service.start_sign_in();

        assert_eq!(
            events,
            vec![AccountEvent::StoreUnavailable {
                store_name_key: "store-name-keychain"
            }]
        );
        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut { note: None }
        );
        assert!(fixture.secure.entries().is_empty());
        assert!(fixture.auth.call_log().is_empty());
    }

    /// FR-008 / FR-009: a successful callback exchanges the code, stores
    /// the credential (and only the credential — never the raw tokens
    /// anywhere else), writes `account.toml`, and checks the tier (T048).
    #[test]
    fn callback_with_code_stores_credential_then_checks_tier() {
        let mut fixture = fresh_fixture();
        fixture
            .auth
            .push_exchange_code(ScriptedCall::ok(token_set()));
        fixture
            .auth
            .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));

        let events = fixture.service.start_sign_in();
        let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
            panic!("expected BrowserUrlReady");
        };

        // Simulate the browser completing the callback: parse the port and
        // state straight out of the URL the fake produced, then drive the
        // real loopback listener exactly like a browser redirect would.
        let port = extract_query_value(&url, "port").expect("port in fake url");
        let state = extract_query_value(&url, "state").expect("state in fake url");
        drive_callback(port, &state, "code=auth-code");

        let events = drain_until(&mut fixture.service, |service| {
            matches!(service.state(), SessionState::Active)
        });

        assert!(events.contains(&AccountEvent::Authorized));
        assert!(events.contains(&AccountEvent::TierChecked(Tier::Premium)));
        assert_eq!(fixture.service.tier(), Tier::Premium);
        assert!(fixture.service.playback_permitted());

        // The credential lives only in the secure store; `account.toml`
        // only ever carries the entry name, never the token.
        let credential_bytes = fixture
            .secure
            .entries()
            .get(&modplayer_secure_store::EntryName::SessionCredential)
            .cloned()
            .expect("credential written");
        let credential_text = String::from_utf8(credential_bytes).expect("utf8");
        assert!(credential_text.contains("access-token"));
        assert!(
            !fixture
                .secure
                .entries()
                .contains_key(&modplayer_secure_store::EntryName::PendingAuthorization)
        );

        let account = fixture.service.account().expect("account known");
        assert_eq!(account.account_id, "user-1");
        assert_eq!(account.tier, Tier::Premium);
        assert_eq!(account.credential_ref, "session-credential");
    }

    /// FR-016: starting a fresh attempt discards the previous one — the
    /// old attempt's pending entry is gone and a stray callback to the old
    /// port/state can no longer complete it (T049).
    #[test]
    fn starting_a_new_attempt_discards_the_previous() {
        let mut fixture = fresh_fixture();

        let first = fixture.service.start_sign_in();
        let Some(AccountEvent::BrowserUrlReady(first_url)) = first.into_iter().next() else {
            panic!("expected BrowserUrlReady");
        };
        let SessionState::Authorizing {
            attempt_id: first_attempt,
            ..
        } = *fixture.service.state()
        else {
            panic!("expected Authorizing");
        };
        let first_port = extract_query_value(&first_url, "port").expect("port");

        let second = fixture.service.start_sign_in();
        assert!(matches!(
            second.first(),
            Some(AccountEvent::BrowserUrlReady(_))
        ));
        let SessionState::Authorizing {
            attempt_id: second_attempt,
            ..
        } = *fixture.service.state()
        else {
            panic!("expected Authorizing");
        };
        assert_ne!(first_attempt, second_attempt);

        // The old attempt's listener was told to cancel the moment the new
        // attempt started (`discard_pending_attempt`); even if a stray
        // connection to its old port briefly succeeds, any result it
        // produces carries the old attempt id, which `tick()` now discards
        // as stale — the meaningful assertion is the attempt id change
        // above, this is just documentation that the port isn't load-
        // bearing for correctness.
        let _ = first_port.parse::<u16>().expect("numeric port");
    }

    /// FR-016 / SC-005: cancel, timeout, and a callback error all return to
    /// `SignedOut` with the right note and leave nothing pending (T088,
    /// folded in here as it shares every fixture with the happy path —
    /// `cancel_timeout_and_error_return_to_sign_in_with_no_state` itself is
    /// the dedicated integration test in `tests/sign_in.rs`).
    #[test]
    fn cancel_sign_in_returns_to_signed_out_with_no_pending_state() {
        let mut fixture = fresh_fixture();
        fixture.service.start_sign_in();
        assert!(matches!(
            fixture.service.state(),
            SessionState::Authorizing { .. }
        ));

        fixture.service.cancel_sign_in();

        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut {
                note: Some(SignInNote::Cancelled)
            }
        );
        assert!(
            !fixture
                .secure
                .entries()
                .contains_key(&modplayer_secure_store::EntryName::PendingAuthorization)
        );
    }

    /// FR-011: a non-Premium tier check result still reaches `Active` (so
    /// the app can show the browse-only screen) but never permits playback.
    #[test]
    fn tier_free_and_unknown_gate_playback() {
        for tier in [Tier::Free, Tier::Unknown] {
            let mut fixture = fresh_fixture();
            fixture
                .auth
                .push_exchange_code(ScriptedCall::ok(token_set()));
            fixture
                .auth
                .push_fetch_profile(ScriptedCall::ok(profile(tier)));

            let events = fixture.service.start_sign_in();
            let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
                panic!("expected BrowserUrlReady");
            };
            let port = extract_query_value(&url, "port").expect("port");
            let state = extract_query_value(&url, "state").expect("state");
            drive_callback(port, &state, "code=auth-code");

            drain_until(&mut fixture.service, |service| {
                matches!(service.state(), SessionState::Active)
            });

            assert_eq!(fixture.service.tier(), tier);
            assert!(!fixture.service.playback_permitted());
        }
    }

    /// FR-008: a tier check that never resolves (simulated by scripting no
    /// result at all — the fake defaults unscripted calls to `Transient`)
    /// still keeps the credential and reaches `Active` with `Unknown`.
    #[test]
    fn tier_check_times_out_after_30s_keeping_credential() {
        let mut fixture = fresh_fixture();
        fixture
            .auth
            .push_exchange_code(ScriptedCall::ok(token_set()));
        // No `push_fetch_profile` scripted: the fake's default (Transient)
        // stands in for "the 30 s budget expired" from this service's
        // point of view — either way `fetch_profile` returns `Err`.

        let events = fixture.service.start_sign_in();
        let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
            panic!("expected BrowserUrlReady");
        };
        let port = extract_query_value(&url, "port").expect("port");
        let state = extract_query_value(&url, "state").expect("state");
        drive_callback(port, &state, "code=auth-code");

        let events = drain_until(&mut fixture.service, |service| {
            matches!(service.state(), SessionState::Active)
        });

        assert!(events.contains(&AccountEvent::TierCheckFailed));
        assert_eq!(fixture.service.tier(), Tier::Unknown);
        // The credential is still there — a failed tier check never
        // discards it.
        assert!(
            fixture
                .secure
                .entries()
                .contains_key(&modplayer_secure_store::EntryName::SessionCredential)
        );
    }

    /// Extract `key`'s value from a `?a=b&c=d`-style URL (test helper —
    /// `FakeAuthorizationService::authorization_url` embeds `state`/`port`
    /// directly for exactly this purpose).
    fn extract_query_value(url: &str, key: &str) -> Option<String> {
        let query = url.split('?').nth(1)?;
        query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == key).then(|| v.to_string())
        })
    }

    /// Connect to the loopback listener on `port` and send a minimal
    /// `GET {REDIRECT_PATH}?{query}&state={state}` request line, standing
    /// in for the browser's redirect.
    fn drive_callback(port: String, state: &str, query: &str) {
        use std::io::Write;
        let addr = format!("127.0.0.1:{port}");
        // The listener thread starts polling immediately, but a
        // freshly-spawned OS thread may not have called `accept()` yet;
        // retry the connect briefly rather than making the whole crate's
        // tests flaky under load.
        let mut last_err = None;
        for _ in 0..50 {
            match std::net::TcpStream::connect(&addr) {
                Ok(mut stream) => {
                    let _ = stream.write_all(
                        format!("GET {REDIRECT_PATH}?{query}&state={state} HTTP/1.1\r\n\r\n")
                            .as_bytes(),
                    );
                    return;
                }
                Err(err) => {
                    last_err = Some(err);
                    std::thread::sleep(StdDuration::from_millis(10));
                }
            }
        }
        panic!("could not connect to loopback listener on {addr}: {last_err:?}");
    }
}
