// SPDX-License-Identifier: MIT OR Apache-2.0

//! `MemorySecureStore`: the `SecureStore` test double
//! (contracts/secure-store.md), used across crate boundaries by
//! `modplayer-account` and `modplayer-ui` tests.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::store::{EntryName, SecureStore, SecureStoreError};

/// In-memory `SecureStore` double. `set_unavailable`/`set_fail_writes`
/// simulate a locked/refusing store (FR-017, FR-020, SC-004, SC-008); the
/// snapshot/log accessors back the credential-leak test (SC-004).
#[derive(Debug, Default)]
pub struct MemorySecureStore {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: HashMap<EntryName, Vec<u8>>,
    unavailable: bool,
    fail_writes: bool,
    written_bytes_log: Vec<Vec<u8>>,
}

impl MemorySecureStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make every call return `SecureStoreError::Unavailable` (simulates a
    /// locked/inaccessible store, FR-017).
    pub fn set_unavailable(&self, unavailable: bool) {
        self.lock().unavailable = unavailable;
    }

    /// Make `put` return `SecureStoreError::Platform` while `get`/`delete`/
    /// `probe` keep working (simulates a store that refuses writes).
    pub fn set_fail_writes(&self, fail_writes: bool) {
        self.lock().fail_writes = fail_writes;
    }

    /// Snapshot of every entry currently stored, for assertions.
    pub fn entries(&self) -> HashMap<EntryName, Vec<u8>> {
        self.lock().entries.clone()
    }

    /// Every byte slice ever passed to `put`, in call order — the FR-020
    /// leak test scans this alongside files/logs/notifications.
    pub fn written_bytes_log(&self) -> Vec<Vec<u8>> {
        self.lock().written_bytes_log.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned lock still holds valid data for a test double; recover
        // rather than propagate the panic into an unrelated test.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SecureStore for MemorySecureStore {
    fn platform_name_key(&self) -> &'static str {
        "store-name-keychain"
    }

    fn probe(&self) -> Result<(), SecureStoreError> {
        if self.lock().unavailable {
            return Err(SecureStoreError::Unavailable);
        }
        Ok(())
    }

    fn put(&self, entry: EntryName, secret: &[u8]) -> Result<(), SecureStoreError> {
        let mut inner = self.lock();
        if inner.unavailable {
            return Err(SecureStoreError::Unavailable);
        }
        if inner.fail_writes {
            return Err(SecureStoreError::Platform);
        }
        inner.written_bytes_log.push(secret.to_vec());
        inner.entries.insert(entry, secret.to_vec());
        Ok(())
    }

    fn get(&self, entry: EntryName) -> Result<Option<Vec<u8>>, SecureStoreError> {
        let inner = self.lock();
        if inner.unavailable {
            return Err(SecureStoreError::Unavailable);
        }
        Ok(inner.entries.get(&entry).cloned())
    }

    fn delete(&self, entry: EntryName) -> Result<(), SecureStoreError> {
        let mut inner = self.lock();
        if inner.unavailable {
            return Err(SecureStoreError::Unavailable);
        }
        inner.entries.remove(&entry);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_put_entry() {
        let store = MemorySecureStore::new();
        store.put(EntryName::SessionCredential, b"secret").unwrap();
        assert_eq!(
            store.get(EntryName::SessionCredential).unwrap(),
            Some(b"secret".to_vec())
        );
    }

    #[test]
    fn get_of_missing_entry_is_ok_none() {
        let store = MemorySecureStore::new();
        assert_eq!(store.get(EntryName::SessionCredential).unwrap(), None);
    }

    #[test]
    fn delete_of_missing_entry_is_ok() {
        let store = MemorySecureStore::new();
        assert!(store.delete(EntryName::SessionCredential).is_ok());
    }

    #[test]
    fn unavailable_fails_every_call() {
        let store = MemorySecureStore::new();
        store.set_unavailable(true);
        assert!(matches!(store.probe(), Err(SecureStoreError::Unavailable)));
        assert!(matches!(
            store.put(EntryName::SessionCredential, b"x"),
            Err(SecureStoreError::Unavailable)
        ));
        assert!(matches!(
            store.get(EntryName::SessionCredential),
            Err(SecureStoreError::Unavailable)
        ));
        assert!(matches!(
            store.delete(EntryName::SessionCredential),
            Err(SecureStoreError::Unavailable)
        ));
    }

    #[test]
    fn fail_writes_only_blocks_put() {
        let store = MemorySecureStore::new();
        store.set_fail_writes(true);
        assert!(matches!(
            store.put(EntryName::SessionCredential, b"x"),
            Err(SecureStoreError::Platform)
        ));
        assert!(store.get(EntryName::SessionCredential).unwrap().is_none());
        assert!(store.delete(EntryName::SessionCredential).is_ok());
        assert!(store.probe().is_ok());
    }

    #[test]
    fn written_bytes_log_records_every_put_in_order() {
        let store = MemorySecureStore::new();
        store.put(EntryName::SessionCredential, b"one").unwrap();
        store.put(EntryName::PendingAuthorization, b"two").unwrap();
        assert_eq!(
            store.written_bytes_log(),
            vec![b"one".to_vec(), b"two".to_vec()]
        );
    }
}
