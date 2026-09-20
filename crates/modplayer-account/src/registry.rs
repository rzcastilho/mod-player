// SPDX-License-Identifier: MIT OR Apache-2.0

//! `AccountScopedStore` trait (data-model.md §1.6): everything sign-out
//! must clear, plus its `CredentialStore` implementor (contracts/
//! account-session.md "Commands" `sign_out()`, FR-015, FR-019).
//! `AccountStateStore`'s own impl of this trait lives in `state_store.rs`
//! (T081).

use std::sync::Arc;

use thiserror::Error;

use modplayer_secure_store::{EntryName, SecureStore};

/// A store failed to clear during sign-out. The category (from
/// `category_key`) is reported to the UI so `SignOutIncomplete` can name
/// the specific failure.
#[derive(Debug, Error)]
#[error("could not clear account-scoped store")]
pub struct ClearError;

/// Something scoped to the current account that sign-out must remove
/// (data-model.md §1.6, FR-015). Registered on `AccountService` in
/// confirmation-list order; later slices push further implementors
/// (offline cache, per-track state) onto the registry, and the
/// confirmation/report lists are generated from it, never hard-coded.
pub trait AccountScopedStore: Send {
    /// Fluent key for the sign-out confirmation list and the "Deleted:"
    /// report.
    fn category_key(&self) -> &'static str;
    /// Remove everything this store holds for the current account.
    /// Idempotent.
    fn clear(&mut self) -> Result<(), ClearError>;
}

/// The `session-credential` + `pending-authorization` secure-store entries
/// (T080). `clear()` deletes both — `SecureStore::delete` is itself
/// idempotent (`Ok(())` when an entry doesn't exist), so a repeated
/// sign-out, or one following a pending attempt that was already
/// discarded, still succeeds.
pub struct CredentialStore {
    secure: Arc<dyn SecureStore>,
}

impl CredentialStore {
    pub fn new(secure: Arc<dyn SecureStore>) -> Self {
        Self { secure }
    }
}

impl AccountScopedStore for CredentialStore {
    fn category_key(&self) -> &'static str {
        "signout-category-credential"
    }

    fn clear(&mut self) -> Result<(), ClearError> {
        self.secure
            .delete(EntryName::SessionCredential)
            .map_err(|_| ClearError)?;
        self.secure
            .delete(EntryName::PendingAuthorization)
            .map_err(|_| ClearError)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    /// A minimal in-memory implementor, exercising only that the trait is
    /// object-safe and its contract (idempotent `clear`) is satisfiable —
    /// the real implementors (`CredentialStore`, `AccountStateStore`) land
    /// in US3.
    struct FakeScopedStore {
        category: &'static str,
        cleared: bool,
    }

    impl AccountScopedStore for FakeScopedStore {
        fn category_key(&self) -> &'static str {
            self.category
        }

        fn clear(&mut self) -> Result<(), ClearError> {
            self.cleared = true;
            Ok(())
        }
    }

    #[test]
    fn trait_object_is_usable_through_a_box() {
        let mut store: Box<dyn AccountScopedStore> = Box::new(FakeScopedStore {
            category: "signout-category-credential",
            cleared: false,
        });
        assert_eq!(store.category_key(), "signout-category-credential");
        assert!(store.clear().is_ok());
    }

    #[test]
    fn credential_store_clears_credential_and_pending_entries() {
        let secure = Arc::new(modplayer_secure_store::MemorySecureStore::new());
        secure.put(EntryName::SessionCredential, b"secret").unwrap();
        secure
            .put(EntryName::PendingAuthorization, b"pending")
            .unwrap();

        let mut store = CredentialStore::new(secure.clone() as Arc<dyn SecureStore>);
        assert_eq!(store.category_key(), "signout-category-credential");
        assert!(store.clear().is_ok());

        assert!(secure.get(EntryName::SessionCredential).unwrap().is_none());
        assert!(
            secure
                .get(EntryName::PendingAuthorization)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn credential_store_clear_is_idempotent() {
        let secure = Arc::new(modplayer_secure_store::MemorySecureStore::new());
        let mut store = CredentialStore::new(secure as Arc<dyn SecureStore>);
        assert!(store.clear().is_ok());
        assert!(store.clear().is_ok());
    }
}
