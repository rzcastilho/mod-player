// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SecureStore` trait, `EntryName`, `SecureStoreError`
//! (contracts/secure-store.md).

use thiserror::Error;

/// Service string used for every entry in the OS credential store.
pub const SERVICE: &str = "ModPlayer";

/// Closed set of entry names — no free-form strings reach the OS store
/// (contracts/secure-store.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryName {
    SessionCredential,
    PendingAuthorization,
    /// Throw-away probe entry scoped to the current process id
    /// (`probe()`'s round-trip; contracts/secure-store.md guarantee 2).
    Probe(u32),
    /// Reserved for 003's reusable receiver credential.
    ReceiverCredential,
}

impl EntryName {
    /// The kebab-case username this entry is stored under (service string
    /// is always [`SERVICE`]).
    pub fn user_name(self) -> String {
        match self {
            EntryName::SessionCredential => "session-credential".to_string(),
            EntryName::PendingAuthorization => "pending-authorization".to_string(),
            EntryName::Probe(pid) => format!("probe-{pid}"),
            EntryName::ReceiverCredential => "receiver-credential".to_string(),
        }
    }
}

/// Errors from a `SecureStore` operation. `Display` never includes the
/// secret or entry payload (contracts/secure-store.md).
#[derive(Debug, Error)]
pub enum SecureStoreError {
    /// Store is locked or refuses access (keyring `NoStorageAccess`,
    /// `NoDefaultStore`).
    #[error("secure store unavailable")]
    Unavailable,
    /// Platform API failed for another reason (keyring `PlatformFailure`,
    /// `BadEncoding`, …).
    #[error("secure store failure")]
    Platform,
    /// Payload exceeds the platform limit (keyring `TooLong`).
    #[error("secret too long: {0} > {1} bytes")]
    TooLong(usize, u32),
}

/// Platform adapter over the OS credential store (Constitution VI, X).
pub trait SecureStore: Send + Sync {
    /// Human-readable, platform-specific store name Fluent key:
    /// `"store-name-keychain"` (macOS), `"store-name-credential-manager"`
    /// (Windows), `"store-name-secret-service"` (Linux/*BSD).
    fn platform_name_key(&self) -> &'static str;

    /// Write-read-delete round-trip of a throw-away `probe-<pid>` entry.
    /// `Ok(())` iff all three succeed and the read bytes equal the written
    /// bytes.
    fn probe(&self) -> Result<(), SecureStoreError>;

    fn put(&self, entry: EntryName, secret: &[u8]) -> Result<(), SecureStoreError>;

    /// `Ok(None)` when the entry does not exist.
    fn get(&self, entry: EntryName) -> Result<Option<Vec<u8>>, SecureStoreError>;

    /// `Ok(())` when the entry did not exist (idempotent).
    fn delete(&self, entry: EntryName) -> Result<(), SecureStoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_map_to_kebab_case() {
        assert_eq!(
            EntryName::SessionCredential.user_name(),
            "session-credential"
        );
        assert_eq!(
            EntryName::PendingAuthorization.user_name(),
            "pending-authorization"
        );
        assert_eq!(EntryName::Probe(1234).user_name(), "probe-1234");
        assert_eq!(
            EntryName::ReceiverCredential.user_name(),
            "receiver-credential"
        );
    }
}
