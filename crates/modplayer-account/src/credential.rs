// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SessionCredential` (data-model.md §1.4, secure-store entry
//! `session-credential`). Redacting `Debug`, no `Display`; the only path
//! to serialize one is through this module's private `CredentialPayload`
//! wire struct (FR-020).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// The secret token set held only in the OS secure store (data-model.md
/// §1.4). Never `Display`; `Debug` prints field names only, so it can
/// never reach a log via `{:?}` (FR-020).
#[derive(Clone, PartialEq, Eq)]
pub struct SessionCredential {
    pub access_token: String,
    /// Replaced when a refresh response carries a new one.
    pub refresh_token: String,
    /// Duplicated from `AccountSession` so the secret is self-describing.
    pub expires_at: OffsetDateTime,
    /// Space-separated granted scopes.
    pub scope: String,
    pub authorized_at: OffsetDateTime,
}

impl std::fmt::Debug for SessionCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionCredential")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .field("authorized_at", &self.authorized_at)
            .finish()
    }
}

/// Errors turning a `SessionCredential` into/from its wire payload.
#[derive(Debug, thiserror::Error)]
pub enum CredentialPayloadError {
    #[error("could not (de)serialize credential payload: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not format a credential timestamp: {0}")]
    Format(#[from] time::error::Format),
    #[error("could not parse a credential timestamp: {0}")]
    Parse(#[from] time::error::Parse),
}

/// The literal JSON shape written to the secure store. Kept private so the
/// only way to serialize a `SessionCredential` is through
/// `to_payload`/`from_payload` below — no `Serialize` impl on
/// `SessionCredential` itself, and no path into any other format
/// (FR-020).
#[derive(Serialize, Deserialize)]
struct CredentialPayload {
    access_token: String,
    refresh_token: String,
    expires_at: String,
    scope: String,
    authorized_at: String,
}

impl SessionCredential {
    /// Serialize to compact JSON for the secure store (size test:
    /// `< 2048` bytes with maximal realistic token lengths — Windows
    /// 2560-byte blob cap, research R5 — lands with US2 T055).
    pub fn to_payload(&self) -> Result<Vec<u8>, CredentialPayloadError> {
        let payload = CredentialPayload {
            access_token: self.access_token.clone(),
            refresh_token: self.refresh_token.clone(),
            expires_at: self.expires_at.format(&Rfc3339)?,
            scope: self.scope.clone(),
            authorized_at: self.authorized_at.format(&Rfc3339)?,
        };
        Ok(serde_json::to_vec(&payload)?)
    }

    /// Parse from the secure store's stored bytes.
    pub fn from_payload(bytes: &[u8]) -> Result<Self, CredentialPayloadError> {
        let payload: CredentialPayload = serde_json::from_slice(bytes)?;
        Ok(Self {
            access_token: payload.access_token,
            refresh_token: payload.refresh_token,
            expires_at: OffsetDateTime::parse(&payload.expires_at, &Rfc3339)?,
            scope: payload.scope,
            authorized_at: OffsetDateTime::parse(&payload.authorized_at, &Rfc3339)?,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn sample() -> SessionCredential {
        SessionCredential {
            access_token: "super-secret-access".to_string(),
            refresh_token: "super-secret-refresh".to_string(),
            expires_at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3600),
            scope: "streaming user-read-private".to_string(),
            authorized_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn debug_never_prints_token_bytes() {
        let debug = format!("{:?}", sample());
        assert!(!debug.contains("super-secret-access"));
        assert!(!debug.contains("super-secret-refresh"));
    }

    #[test]
    fn payload_round_trips() {
        let credential = sample();
        let payload = credential.to_payload().expect("serialize");
        let round_tripped = SessionCredential::from_payload(&payload).expect("parse");
        assert_eq!(round_tripped, credential);
    }

    #[test]
    fn payload_bytes_never_contain_the_field_names_only_marker() {
        // Sanity check that `to_payload` really carries the secret (unlike
        // `Debug`) — the FR-020 leak test scans *other* surfaces
        // (files/logs/notifications), never the secure-store payload
        // itself.
        let payload = sample().to_payload().expect("serialize");
        let text = String::from_utf8(payload).expect("utf8");
        assert!(text.contains("super-secret-access"));
    }

    /// R5: the Windows Credential Manager blob cap is 2560 bytes; the
    /// serialised payload must stay comfortably under that even with
    /// maximal realistic token lengths (T055).
    #[test]
    fn credential_payload_fits_windows_blob_limit() {
        let credential = SessionCredential {
            // Spotify access tokens run ≈300 chars; pad generously.
            access_token: "a".repeat(400),
            // Refresh tokens run ≈130 chars; pad generously.
            refresh_token: "r".repeat(200),
            expires_at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3600),
            scope: "streaming user-read-private user-read-email".to_string(),
            authorized_at: OffsetDateTime::UNIX_EPOCH,
        };
        let payload = credential.to_payload().expect("serialize");
        assert!(
            payload.len() < 2048,
            "credential payload is {} bytes, must stay under the 2048-byte test budget \
             (Windows blob cap is 2560 bytes, research R5)",
            payload.len()
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Arbitrary non-empty token-like strings (ASCII, no control
    /// characters) — realistic stand-ins for access/refresh tokens and
    /// scope strings.
    fn token_string() -> impl Strategy<Value = String> {
        "[!-~]{1,400}"
    }

    /// Arbitrary `OffsetDateTime`s within a wide, always-representable
    /// range (RFC 3339 formatting is defined for any UTC offset-zero
    /// instant in this range).
    fn timestamp() -> impl Strategy<Value = OffsetDateTime> {
        (0i64..=4_102_444_800i64) // 1970-01-01 .. 2100-01-01
            .prop_map(|secs| OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(secs))
    }

    proptest! {
        /// Constitution VIII state-serialization proptest (T108): any
        /// token set survives `SessionCredential` -> payload ->
        /// `SessionCredential`, and the payload never exceeds the R5 blob
        /// cap for inputs within the maximal realistic lengths asserted by
        /// `credential_payload_fits_windows_blob_limit`.
        #[test]
        fn credential_payload_round_trips_any_token_set(
            access_token in token_string(),
            refresh_token in token_string(),
            scope in "[!-~ ]{0,200}",
            expires_at in timestamp(),
            authorized_at in timestamp(),
        ) {
            let credential = SessionCredential {
                access_token,
                refresh_token,
                expires_at,
                scope,
                authorized_at,
            };
            let payload = credential.to_payload().expect("serialize");
            prop_assert!(payload.len() < 2560, "payload exceeds the Windows blob cap");
            let round_tripped = SessionCredential::from_payload(&payload).expect("parse");
            prop_assert_eq!(round_tripped, credential);
        }
    }
}
