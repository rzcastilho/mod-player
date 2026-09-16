// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! Account/session service: PKCE sign-in, loopback callback, tier
//! verification, refresh scheduling, and the account-scoped store
//! registry. Deliberately does not depend on `modplayer-core` so a
//! later receiver crate can take the credential from this crate alone
//! (contracts/authorization-service.md, contracts/account-session.md).

pub mod auth_service;
pub mod clock;
pub mod credential;
pub mod disclosure;
pub mod fake_auth;
pub mod launch_flow;
pub mod listener;
pub mod notify;
pub mod pending;
pub mod pkce;
pub mod refresh;
pub mod registry;
pub mod service;
pub mod session;
pub mod spotify;
pub mod state_store;

pub use auth_service::{
    AuthError, AuthorizationService, ClientConfig, PlaybackStateSummary, Profile, TokenSet,
};
pub use clock::{Clock, FakeClock, SystemClock};
pub use credential::{CredentialPayloadError, SessionCredential};
pub use disclosure::{DISCLOSURE_BUNDLE_VERSION, DISCLOSURE_EN_US_SHA256, TERMS_URL, UPGRADE_URL};
pub use fake_auth::{FakeAuthorizationService, LoggedCall, ScriptedCall};
pub use launch_flow::{LaunchStep, next_step};
pub use pending::PendingAuthorization;
pub use pkce::PkceMaterial;
pub use refresh::RefreshScheduler;
pub use registry::{AccountScopedStore, ClearError, CredentialStore};
pub use service::{AccountEvent, AccountService, LaunchOutcome, ReadOutcome, RequestId};
pub use session::{AccountSession, SessionState, SignInNote, Tier};
pub use spotify::SpotifyAuthorizationService;
pub use state_store::{AccountStateStore, LoadOutcome, PersistedAccount};
