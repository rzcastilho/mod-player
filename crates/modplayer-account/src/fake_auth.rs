// SPDX-License-Identifier: MIT OR Apache-2.0

//! `FakeAuthorizationService`: the `AuthorizationService` test double
//! (contracts/authorization-service.md). Scripted per-call results, a
//! call log for assertions, an optional per-call delay (paired with
//! `FakeClock` to exercise the 30 s timeout without sleeping —
//! Constitution VIII), and a `browser` hook that simulates the user
//! completing the flow in a real browser by driving the loopback callback
//! URL. `pub`, used across the crate boundary by `modplayer-ui` tests.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::auth_service::{AuthError, AuthorizationService, Profile, TokenSet};
use crate::pending::PendingAuthorization;

/// One scripted call: which result to return, and how long the caller
/// should have to wait for it (tests advance a `FakeClock` past `delay` to
/// exercise a timeout path rather than actually sleeping).
pub struct ScriptedCall<T> {
    pub result: Result<T, AuthError>,
    pub delay: Duration,
}

impl<T> ScriptedCall<T> {
    pub fn ok(value: T) -> Self {
        Self {
            result: Ok(value),
            delay: Duration::ZERO,
        }
    }

    pub fn err(error: AuthError) -> Self {
        Self {
            result: Err(error),
            delay: Duration::ZERO,
        }
    }

    #[must_use]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// A call this fake recorded, for test assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoggedCall {
    AuthorizationUrl,
    ExchangeCode,
    Refresh,
    FetchProfile,
}

/// Simulates the user's browser: called with the built authorization URL
/// from `authorization_url`, when set.
type BrowserHook = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
struct Script {
    exchange_code: Vec<ScriptedCall<TokenSet>>,
    refresh: Vec<ScriptedCall<TokenSet>>,
    fetch_profile: Vec<ScriptedCall<Profile>>,
    log: Vec<LoggedCall>,
    browser: Option<BrowserHook>,
}

/// `AuthorizationService` test double (contracts/authorization-service.md).
/// Queue results with `push_exchange_code`/`push_refresh`/
/// `push_fetch_profile`; each call pops the front of its queue (or, if
/// empty, returns `AuthError::Transient` as a safe default rather than
/// panicking). Cheaply `Clone`, so a test can keep a handle to push more
/// script/read the call log after handing an `Arc<dyn AuthorizationService>`
/// to an `AccountService`.
pub struct FakeAuthorizationService {
    script: Arc<Mutex<Script>>,
}

impl FakeAuthorizationService {
    pub fn new() -> Self {
        Self {
            script: Arc::new(Mutex::new(Script::default())),
        }
    }

    /// Set the hook that simulates the user completing sign-in in a real
    /// browser (contracts/authorization-service.md).
    pub fn set_browser(&self, hook: impl Fn(&str) + Send + Sync + 'static) {
        self.lock().browser = Some(Arc::new(hook));
    }

    /// Queue the result (and optional delay) for the next `exchange_code` call.
    pub fn push_exchange_code(&self, call: ScriptedCall<TokenSet>) {
        self.lock().exchange_code.push(call);
    }

    /// Queue the result (and optional delay) for the next `refresh` call.
    pub fn push_refresh(&self, call: ScriptedCall<TokenSet>) {
        self.lock().refresh.push(call);
    }

    /// Queue the result (and optional delay) for the next `fetch_profile` call.
    pub fn push_fetch_profile(&self, call: ScriptedCall<Profile>) {
        self.lock().fetch_profile.push(call);
    }

    /// Every call made so far, in order.
    pub fn call_log(&self) -> Vec<LoggedCall> {
        self.lock().log.clone()
    }

    /// The delay scripted for the *next* `exchange_code` call, without
    /// consuming it — lets a test advance a `FakeClock` by the right
    /// amount before making the real call.
    pub fn next_exchange_code_delay(&self) -> Option<Duration> {
        self.lock().exchange_code.first().map(|call| call.delay)
    }

    pub fn next_refresh_delay(&self) -> Option<Duration> {
        self.lock().refresh.first().map(|call| call.delay)
    }

    pub fn next_fetch_profile_delay(&self) -> Option<Duration> {
        self.lock().fetch_profile.first().map(|call| call.delay)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Script> {
        self.script
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Clone for FakeAuthorizationService {
    fn clone(&self) -> Self {
        Self {
            script: Arc::clone(&self.script),
        }
    }
}

impl Default for FakeAuthorizationService {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthorizationService for FakeAuthorizationService {
    fn authorization_url(&self, attempt: &PendingAuthorization) -> String {
        let url = {
            let mut inner = self.lock();
            inner.log.push(LoggedCall::AuthorizationUrl);
            format!(
                "https://accounts.spotify.com/authorize?state={}&port={}",
                attempt.state, attempt.port
            )
        };
        // Release the lock before invoking the hook: it may itself drive a
        // loopback connection that ends up calling back into this fake.
        let browser = self.lock().browser.clone();
        if let Some(browser) = browser {
            browser(&url);
        }
        url
    }

    fn exchange_code(
        &self,
        _code: &str,
        _verifier: &str,
        _redirect_uri: &str,
    ) -> Result<TokenSet, AuthError> {
        let mut inner = self.lock();
        inner.log.push(LoggedCall::ExchangeCode);
        pop_scripted(&mut inner.exchange_code)
    }

    fn refresh(&self, _refresh_token: &str) -> Result<TokenSet, AuthError> {
        let mut inner = self.lock();
        inner.log.push(LoggedCall::Refresh);
        pop_scripted(&mut inner.refresh)
    }

    fn fetch_profile(&self, _access_token: &str) -> Result<Profile, AuthError> {
        let mut inner = self.lock();
        inner.log.push(LoggedCall::FetchProfile);
        pop_scripted(&mut inner.fetch_profile)
    }
}

/// Pop the front of a scripted-call queue, defaulting to
/// `AuthError::Transient` when nothing was scripted (a test forgot to
/// script this call, or intentionally wants the default).
fn pop_scripted<T>(queue: &mut Vec<ScriptedCall<T>>) -> Result<T, AuthError> {
    if queue.is_empty() {
        return Err(AuthError::Transient);
    }
    queue.remove(0).result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn attempt() -> PendingAuthorization {
        PendingAuthorization {
            attempt_id: 1,
            pkce_verifier: "verifier".to_string(),
            state: "state-abc".to_string(),
            port: 4321,
            started_at: time::OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn token_set() -> TokenSet {
        TokenSet {
            access_token: "access".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_in: Duration::from_secs(3600),
            scope: "streaming".to_string(),
        }
    }

    #[test]
    fn unscripted_calls_default_to_transient() {
        let fake = FakeAuthorizationService::new();
        assert!(matches!(
            fake.exchange_code("c", "v", "r"),
            Err(AuthError::Transient)
        ));
        assert!(matches!(fake.refresh("r"), Err(AuthError::Transient)));
        assert!(matches!(fake.fetch_profile("a"), Err(AuthError::Transient)));
    }

    #[test]
    fn scripted_results_are_returned_in_order() {
        let fake = FakeAuthorizationService::new();
        fake.push_refresh(ScriptedCall::err(AuthError::Rejected));
        fake.push_refresh(ScriptedCall::ok(token_set()));

        assert!(matches!(fake.refresh("r"), Err(AuthError::Rejected)));
        assert!(fake.refresh("r").is_ok());
    }

    #[test]
    fn call_log_records_every_call_in_order() {
        let fake = FakeAuthorizationService::new();
        fake.push_fetch_profile(ScriptedCall::err(AuthError::Forbidden));
        let _ = fake.authorization_url(&attempt());
        let _ = fake.fetch_profile("token");

        assert_eq!(
            fake.call_log(),
            vec![LoggedCall::AuthorizationUrl, LoggedCall::FetchProfile]
        );
    }

    #[test]
    fn clone_shares_the_same_script_and_log() {
        let fake = FakeAuthorizationService::new();
        let handle = fake.clone();
        handle.push_refresh(ScriptedCall::ok(token_set()));

        assert!(fake.refresh("r").is_ok());
        assert_eq!(handle.call_log(), vec![LoggedCall::Refresh]);
    }

    #[test]
    fn browser_hook_is_invoked_with_the_built_url() {
        let fake = FakeAuthorizationService::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Mutex::new(String::new()));
        let calls_clone = Arc::clone(&calls);
        let seen_clone = Arc::clone(&seen);
        fake.set_browser(move |url| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            *seen_clone.lock().unwrap() = url.to_string();
        });

        let url = fake.authorization_url(&attempt());

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(*seen.lock().unwrap(), url);
        assert!(url.contains("state-abc"));
        assert!(url.contains("4321"));
    }

    #[test]
    fn next_delay_reflects_the_queued_but_unconsumed_call() {
        let fake = FakeAuthorizationService::new();
        assert_eq!(fake.next_exchange_code_delay(), None);
        fake.push_exchange_code(ScriptedCall::ok(token_set()).with_delay(Duration::from_secs(31)));
        assert_eq!(
            fake.next_exchange_code_delay(),
            Some(Duration::from_secs(31))
        );
    }
}
