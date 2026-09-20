// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tier`, `SignInNote`, `SessionState`, `AccountSession` (data-model.md
//! §1.3, §1.7, §2.1).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Subscription tier from the service profile (data-model.md §1.7).
/// `/v1/me` `product` mapping: `premium → Premium`; `free | open → Free`;
/// anything else / missing → `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Premium,
    Free,
    Unknown,
}

/// One-line explanation shown on the sign-in step for how the session got
/// back to `SignedOut` (data-model.md §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInNote {
    Cancelled,
    TimedOut,
    ServiceError,
    PreviousDidNotFinish,
    Revoked,
    StoreUnreadable,
}

/// `AccountSession`'s in-memory lifecycle state (data-model.md §2.1).
/// Transitions and side effects are specified in
/// contracts/account-session.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// No `account.toml`, no credential. `note` drives the sign-in step's
    /// one-line explanation.
    SignedOut { note: Option<SignInNote> },
    /// Listener thread running; `resumed` = re-bound after relaunch.
    Authorizing { attempt_id: u64, resumed: bool },
    /// Credential + `account.toml` written (tier `Unknown`); tier check in
    /// flight, 30 s budget.
    Checking { attempt_id: u64 },
    /// Credential + `account.toml` present; refresh scheduler armed.
    Active,
    /// `expires_at <= now` and no refresh has succeeded; credential
    /// retained, main window stays available.
    Expired,
    /// `account.toml` present but the secure store failed to read at
    /// launch; nothing cleared.
    StoreUnreadable,
}

impl SessionState {
    /// Whether the sign-in step should render for this state
    /// (data-model.md §2.4 `next_step`): every state except `Active`/
    /// `Expired`.
    pub fn is_signing_in(&self) -> bool {
        matches!(
            self,
            SessionState::SignedOut { .. }
                | SessionState::Authorizing { .. }
                | SessionState::Checking { .. }
                | SessionState::StoreUnreadable
        )
    }
}

/// The account session (data-model.md §1.3). The non-secret fields persist
/// in `account.toml` (see `state_store::PersistedAccount`); `state` is
/// derived at load time and never serialized. At most one exists at a time
/// (FR-012).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountSession {
    /// Service account identifier (`/v1/me` `id`). Non-empty.
    pub account_id: String,
    /// `/v1/me` `display_name`, falling back to `account_id` when null.
    pub display_name: String,
    pub tier: Tier,
    /// Always the secure-store entry name `"session-credential"` — never
    /// the credential value (FR-009).
    pub credential_ref: String,
    /// Access-token expiry (`now + expires_in`).
    pub expires_at: OffsetDateTime,
    /// When the browser flow completed (refresh-token lifetime reference).
    pub authorized_at: OffsetDateTime,
    /// Last successful online validation (`/v1/me` or refresh).
    pub last_validated_at: Option<OffsetDateTime>,
    pub state: SessionState,
}

impl AccountSession {
    /// Whether playback is permitted for the current session (FR-011).
    pub fn playback_permitted(&self) -> bool {
        self.tier == Tier::Premium
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn tier_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Tier::Premium).unwrap(),
            "\"premium\""
        );
        assert_eq!(serde_json::to_string(&Tier::Free).unwrap(), "\"free\"");
        assert_eq!(
            serde_json::to_string(&Tier::Unknown).unwrap(),
            "\"unknown\""
        );
    }

    #[test]
    fn tier_round_trips_through_json() {
        for tier in [Tier::Premium, Tier::Free, Tier::Unknown] {
            let json = serde_json::to_string(&tier).unwrap();
            let round_tripped: Tier = serde_json::from_str(&json).unwrap();
            assert_eq!(round_tripped, tier);
        }
    }

    #[test]
    fn only_premium_permits_playback() {
        let mut session = sample_session(Tier::Premium);
        assert!(session.playback_permitted());
        session.tier = Tier::Free;
        assert!(!session.playback_permitted());
        session.tier = Tier::Unknown;
        assert!(!session.playback_permitted());
    }

    #[test]
    fn active_and_expired_are_not_signing_in() {
        assert!(!SessionState::Active.is_signing_in());
        assert!(!SessionState::Expired.is_signing_in());
    }

    #[test]
    fn every_other_state_is_signing_in() {
        assert!(SessionState::SignedOut { note: None }.is_signing_in());
        assert!(
            SessionState::Authorizing {
                attempt_id: 1,
                resumed: false
            }
            .is_signing_in()
        );
        assert!(SessionState::Checking { attempt_id: 1 }.is_signing_in());
        assert!(SessionState::StoreUnreadable.is_signing_in());
    }

    fn sample_session(tier: Tier) -> AccountSession {
        AccountSession {
            account_id: "abc".to_string(),
            display_name: "Name".to_string(),
            tier,
            credential_ref: "session-credential".to_string(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
            authorized_at: OffsetDateTime::UNIX_EPOCH,
            last_validated_at: None,
            state: SessionState::Active,
        }
    }
}
