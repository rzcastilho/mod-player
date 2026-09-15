// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PendingAuthorization` (data-model.md §1.5, secure-store entry
//! `pending-authorization`).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// One in-flight PKCE sign-in attempt (data-model.md §1.5). At most one
/// exists at a time (FR-016, FR-018); creating a new one deletes the
/// previous entry first. Redacting `Debug` — `pkce_verifier` and `state`
/// never appear in full (EC-1.1).
#[derive(Clone, PartialEq, Eq)]
pub struct PendingAuthorization {
    /// Monotonic per process, used to discard stale worker results.
    /// In-memory only.
    pub attempt_id: u64,
    /// 43-128 chars, URL-safe base64 of 32 random bytes. Secret.
    pub pkce_verifier: String,
    /// URL-safe base64 of 16 random bytes; must match the callback.
    pub state: String,
    /// Loopback port bound for this attempt.
    pub port: u16,
    /// Browser was opened at this time; the attempt expires at
    /// `started_at + 5 min`.
    pub started_at: OffsetDateTime,
}

impl std::fmt::Debug for PendingAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingAuthorization")
            .field("attempt_id", &self.attempt_id)
            .field("pkce_verifier", &"<redacted>")
            .field("state", &"<redacted>")
            .field("port", &self.port)
            .field("started_at", &self.started_at)
            .finish()
    }
}

/// Errors turning a `PendingAuthorization` into/from its wire payload
/// (mirrors `credential::CredentialPayloadError`).
#[derive(Debug, thiserror::Error)]
pub enum PendingPayloadError {
    #[error("could not (de)serialize pending-authorization payload: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not format a pending-authorization timestamp: {0}")]
    Format(#[from] time::error::Format),
    #[error("could not parse a pending-authorization timestamp: {0}")]
    Parse(#[from] time::error::Parse),
}

/// The literal JSON shape written to the secure store. `attempt_id` is
/// deliberately excluded (data-model.md §1.5: "In-memory only") — a
/// resumed attempt is assigned a fresh one by whoever reads this back.
#[derive(Serialize, Deserialize)]
struct PendingPayload {
    pkce_verifier: String,
    state: String,
    port: u16,
    started_at: String,
}

impl PendingAuthorization {
    /// Serialize to compact JSON for the secure store
    /// (contracts/account-session.md "PKCE flow" step 2).
    pub fn to_payload(&self) -> Result<Vec<u8>, PendingPayloadError> {
        let payload = PendingPayload {
            pkce_verifier: self.pkce_verifier.clone(),
            state: self.state.clone(),
            port: self.port,
            started_at: self.started_at.format(&Rfc3339)?,
        };
        Ok(serde_json::to_vec(&payload)?)
    }

    /// Parse from the secure store's stored bytes, assigning `attempt_id`
    /// (a fresh in-memory value — see the field's own doc comment).
    pub fn from_payload(bytes: &[u8], attempt_id: u64) -> Result<Self, PendingPayloadError> {
        let payload: PendingPayload = serde_json::from_slice(bytes)?;
        Ok(Self {
            attempt_id,
            pkce_verifier: payload.pkce_verifier,
            state: payload.state,
            port: payload.port,
            started_at: OffsetDateTime::parse(&payload.started_at, &Rfc3339)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_verifier_or_state() {
        let pending = PendingAuthorization {
            attempt_id: 1,
            pkce_verifier: "super-secret-verifier".to_string(),
            state: "super-secret-state".to_string(),
            port: 12345,
            started_at: OffsetDateTime::UNIX_EPOCH,
        };
        let debug = format!("{pending:?}");
        assert!(!debug.contains("super-secret-verifier"));
        assert!(!debug.contains("super-secret-state"));
        assert!(debug.contains("12345"));
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
    fn payload_round_trips_everything_but_attempt_id() {
        let pending = PendingAuthorization {
            attempt_id: 42,
            pkce_verifier: "verifier-value".to_string(),
            state: "state-value".to_string(),
            port: 4321,
            started_at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(60),
        };
        let payload = pending.to_payload().expect("serialize");
        let round_tripped = PendingAuthorization::from_payload(&payload, 99).expect("parse");
        assert_eq!(round_tripped.pkce_verifier, pending.pkce_verifier);
        assert_eq!(round_tripped.state, pending.state);
        assert_eq!(round_tripped.port, pending.port);
        assert_eq!(round_tripped.started_at, pending.started_at);
        // attempt_id is never persisted (in-memory only) — the caller's
        // value wins, not the original's.
        assert_eq!(round_tripped.attempt_id, 99);
    }
}
